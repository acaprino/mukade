//! The destination facts a harness states about itself.
//!
//! Two writers need a value only the destination's own configuration knows: OpenCode refuses a
//! model reference without a provider, and Codex stamps the provider its `config.toml` names.
//! The CLI used to read them privately; the TUI needs the same answer without a second copy.

use crate::model::HarnessId;

/// The provider and model the destination's own configuration names, when it names them.
/// OpenCode's `model` is `<provider>/<model>`; Codex states `model_provider` at the top level.
pub fn configured_model(harness: HarnessId) -> Option<(String, Option<String>)> {
    match harness {
        HarnessId::OpenCode => opencode_config_model(),
        HarnessId::Codex => codex_config_provider().map(|provider| (provider, None)),
        HarnessId::ClaudeCode | HarnessId::Pi => None,
    }
}

/// OpenCode's own config names its model as `<provider>/<model>`.
fn opencode_config_model() -> Option<(String, Option<String>)> {
    let path = std::env::var_os("OPENCODE_CONFIG")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME").map(|dir| {
                std::path::PathBuf::from(dir)
                    .join("opencode")
                    .join("opencode.json")
            })
        })?;
    let document: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let (provider, model) = document.get("model")?.as_str()?.split_once('/')?;
    Some((provider.to_string(), Some(model.to_string())))
}

/// Codex's `config.toml` states `model_provider = "<id>"`.
fn codex_config_provider() -> Option<String> {
    let home = std::env::var_os("CODEX_HOME").map(std::path::PathBuf::from)?;
    let text = std::fs::read_to_string(home.join("config.toml")).ok()?;
    top_level_string(&text, "model_provider")
}

/// A top-level `key = "value"` of a TOML file: before any `[table]`, the key exactly (not
/// `model_providers.x`), with a trailing `# comment` left out.
fn top_level_string(text: &str, key: &str) -> Option<String> {
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            return None;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        let value = value.trim();
        let value = match value.strip_prefix('"') {
            Some(quoted) => quoted.split('"').next().unwrap_or_default(),
            None => value.split('#').next().unwrap_or_default().trim(),
        };
        return Some(value.to_string()).filter(|value| !value.is_empty());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::top_level_string;

    /// Codex's own provider is the top-level key: not a provider table's field, not a profile's,
    /// and not the comment after it.
    #[test]
    fn the_codex_provider_is_the_top_level_key_only() {
        let config = "model = \"gpt\"\nmodel_provider = \"azure\" # the work account\n\n[model_providers.azure]\nname = \"Azure\"\n";
        assert_eq!(
            top_level_string(config, "model_provider").as_deref(),
            Some("azure")
        );
        let nested = "[profiles.fast]\nmodel_provider = \"openai\"\n";
        assert_eq!(top_level_string(nested, "model_provider"), None);
        let dotted = "model_providers.azure.name = \"Azure\"\n";
        assert_eq!(top_level_string(dotted, "model_provider"), None);
    }
}
