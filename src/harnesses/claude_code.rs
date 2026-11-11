//! Claude Code: sessions under `CLAUDE_CONFIG_DIR/projects/`.

use super::{Harness, InterchangeDocument, StoreStats};
use crate::model::{HarnessId, Opaque, Protocol, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::writer::{WriteOpts, Writer};
use std::path::Path;

pub(super) struct ClaudeCode;

impl Harness for ClaudeCode {
    fn id(&self) -> HarnessId {
        HarnessId::ClaudeCode
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::claude_code::ClaudeCode
    }

    fn writer(&self) -> Option<&'static dyn Writer> {
        Some(&crate::writer::claude_code::ClaudeCode)
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::claude_code::read_file(path)
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.claude_config_dir.as_deref()
    }

    fn set_store_root(&self, roots: &mut StoreRoots, root: &Path) {
        roots.claude_config_dir = Some(root.to_path_buf());
    }

    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        let variable = crate::catalog::store_env(self.id());
        super::set_store_env(command, variable, self.store_root(roots));
    }

    fn destination_protocol(&self) -> Option<Protocol> {
        Some(Protocol::AnthropicMessages)
    }

    /// A signature is written as `thinking`, an encrypted payload as `redacted_thinking`. A Chat
    /// envelope is not kept: only one with a scalar conversion fits.
    fn payload_has_place(&self, opaque: &Opaque) -> bool {
        crate::policy::scalar_chat_payload(opaque)
            && (opaque.signature.is_some() || opaque.encrypted.is_some())
    }

    fn validate_write(&self, opts: &WriteOpts) -> anyhow::Result<()> {
        super::validate_file_write(self, opts)
    }

    fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument> {
        None
    }

    fn store_stats(&self, roots: &StoreRoots) -> StoreStats {
        super::listed_stats(self, roots)
    }
}
