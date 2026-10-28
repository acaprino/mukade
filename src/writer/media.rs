//! Images between harnesses. Each harness records an image in its own provider's shape, so a block
//! copied from one harness into another is a block the destination does not read, and some
//! providers refuse the request outright. An image is carried by its media type and base64 data,
//! and written in the destination's own shape:
//!
//! - Claude Code: `{type:"image", source:{type:"base64", media_type, data}}` (Anthropic messages);
//! - Codex: `{type:"input_image", image_url:"data:<mime>;base64,<data>"}` (OpenAI responses);
//! - Pi: `{type:"image", data, mimeType}` (pi-ai `ImageContent`);
//! - OpenCode 2.1.4: a user attachment is `{data, mime, source:{type:"inline"}}`
//!   (`Prompt.FileAttachment`), an image in a tool result `{type:"file", uri, mime}`
//!   (`Tool.FileContent`).
//!
//! Anything else has no counterpart: the caller drops it and counts it.

use crate::model::HarnessId;
use serde_json::{Value, json};

/// An image, as every harness can hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub mime: String,
    pub data: String,
}

/// The image a block holds, in whichever harness's shape it was recorded.
pub fn image_of(block: &Value) -> Option<Image> {
    let text = |pointer: &str| block.pointer(pointer).and_then(Value::as_str).map(str::to_owned);
    let kind = block.get("type").and_then(Value::as_str);
    match kind {
        Some("image") if text("/source/type").as_deref() == Some("base64") => Some(Image {
            mime: text("/source/media_type")?,
            data: text("/source/data")?,
        }),
        Some("image") => Some(Image {
            mime: text("/mimeType")?,
            data: text("/data")?,
        }),
        Some("input_image") => from_data_uri(&text("/image_url")?),
        Some("file") => from_data_uri(&text("/uri")?),
        None if text("/source/type").as_deref() == Some("inline") => Some(Image {
            mime: text("/mime")?,
            data: text("/data")?,
        }),
        _ => None,
    }
    .filter(|image| image.mime.starts_with("image/"))
}

fn from_data_uri(uri: &str) -> Option<Image> {
    let (header, data) = uri.strip_prefix("data:")?.split_once(',')?;
    let mime = header.strip_suffix(";base64")?;
    Some(Image {
        mime: mime.to_string(),
        data: data.to_string(),
    })
}

/// The image as a user turn of `dest` carries it.
pub fn user_block(image: &Image, dest: HarnessId) -> Value {
    match dest {
        HarnessId::OpenCode => {
            json!({"data": image.data, "mime": image.mime, "source": {"type": "inline"}})
        }
        _ => result_block(image, dest),
    }
}

/// The image as a tool result of `dest` carries it.
pub fn result_block(image: &Image, dest: HarnessId) -> Value {
    match dest {
        HarnessId::ClaudeCode => json!({
            "type": "image",
            "source": {"type": "base64", "media_type": image.mime, "data": image.data},
        }),
        HarnessId::Codex => json!({
            "type": "input_image",
            "image_url": format!("data:{};base64,{}", image.mime, image.data),
        }),
        HarnessId::Pi => json!({"type": "image", "data": image.data, "mimeType": image.mime}),
        HarnessId::OpenCode => json!({
            "type": "file",
            "uri": format!("data:{};base64,{}", image.mime, image.data),
            "mime": image.mime,
        }),
    }
}
