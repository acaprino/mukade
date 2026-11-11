//! Pi: sessions under `PI_CODING_AGENT_DIR/sessions/`.

use super::{Harness, InterchangeDocument, StoreStats};
use crate::model::{HarnessId, Opaque, Protocol, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::writer::{WriteOpts, Writer};
use std::path::Path;

pub(super) struct Pi;

impl Harness for Pi {
    fn id(&self) -> HarnessId {
        HarnessId::Pi
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::pi::Pi
    }

    fn writer(&self) -> Option<&'static dyn Writer> {
        Some(&crate::writer::pi::Pi)
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::pi::read_file(path)
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.pi_agent_dir.as_deref()
    }

    fn set_store_root(&self, roots: &mut StoreRoots, root: &Path) {
        roots.pi_agent_dir = Some(root.to_path_buf());
    }

    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        let variable = crate::catalog::store_env(self.id());
        super::set_store_env(command, variable, self.store_root(roots));
    }

    /// Pi speaks whatever its configured provider speaks.
    fn destination_protocol(&self) -> Option<Protocol> {
        None
    }

    /// A `thinkingSignature` is replayed, and an Anthropic encrypted payload as Pi's own
    /// `redacted: true` block. A Chat envelope is not kept: only one with a scalar conversion fits.
    fn payload_has_place(&self, opaque: &Opaque) -> bool {
        crate::policy::scalar_chat_payload(opaque)
            && (opaque.signature.is_some()
                || (opaque.encrypted.is_some() && opaque.protocol == Protocol::AnthropicMessages))
    }

    fn validate_write(&self, opts: &WriteOpts) -> anyhow::Result<()> {
        super::validate_file_write(self, opts)
    }

    fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument> {
        None
    }

    /// Pi records a fork in the header of the session it forks into.
    fn store_stats(&self, roots: &StoreRoots) -> StoreStats {
        let counted = match roots.pi_agent_dir.as_deref() {
            Some(dir) => {
                crate::reader::pi::store_summary(dir).map_err(|error| format!("{error:#}"))
            }
            None => Err(format!(
                "{} is not set: no Pi store to list",
                crate::catalog::store_env(self.id())
            )),
        };
        match counted {
            Ok(summary) => StoreStats {
                sessions: Ok(summary.sessions),
                forks: Some(summary.forks.map_err(|error| error.to_string())),
            },
            Err(error) => StoreStats {
                sessions: Err(error.clone()),
                forks: Some(Err(error)),
            },
        }
    }

    /// Pi forks a session from a file, so it summarizes one from a file too.
    fn summarizes_from_file(&self) -> bool {
        true
    }
}
