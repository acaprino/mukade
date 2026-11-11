//! Codex CLI: rollouts under `CODEX_HOME/sessions/`.

use super::{Harness, InterchangeDocument};
use crate::model::{HarnessId, Opaque, Protocol, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::writer::{WriteOpts, Writer};
use std::path::Path;

pub(super) struct Codex;

impl Harness for Codex {
    fn id(&self) -> HarnessId {
        HarnessId::Codex
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::codex::Codex
    }

    fn writer(&self) -> Option<&'static dyn Writer> {
        Some(&crate::writer::codex::Codex)
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::codex::read_file(path)
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.codex_home.as_deref()
    }

    fn set_store_root(&self, roots: &mut StoreRoots, root: &Path) {
        roots.codex_home = Some(root.to_path_buf());
    }

    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        let variable = crate::catalog::store_env(self.id());
        super::set_store_env(command, variable, self.store_root(roots));
    }

    fn destination_protocol(&self) -> Option<Protocol> {
        Some(Protocol::OpenaiResponses)
    }

    /// Only `encrypted_content`. A Chat envelope is not kept: only one with a scalar conversion
    /// fits.
    fn payload_has_place(&self, opaque: &Opaque) -> bool {
        crate::policy::scalar_chat_payload(opaque) && opaque.encrypted.is_some()
    }

    fn validate_write(&self, opts: &WriteOpts) -> anyhow::Result<()> {
        super::validate_file_write(self, opts)
    }

    fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument> {
        None
    }
}
