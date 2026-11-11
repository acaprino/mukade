//! One adapter per harness. Every decision that depends on which harness a session comes from or
//! goes to is a method of [`Harness`]; the adapter in `<harness>.rs` is the one place it is made.
//! The readers, writers and detectors stay in their own folders; the adapter refers to them.

mod claude_code;
mod codex;
mod opencode;
mod pi;

pub(crate) use opencode::fork_of as opencode_fork;

use crate::hop::HopError;
use crate::model::{CanonicalTool, HarnessId, Opaque, Protocol, Session};
use crate::native_compaction::NativeCompaction;
use crate::policy::Clock;
use crate::reader::{Reader, SessionInfo};
use crate::roots::StoreRoots;
use crate::summary::{Reply, SummarizeOpts, SummaryCommand};
use crate::tools::{self, Shell, ToolSet, Translation};
use crate::writer::{WriteOpts, Writer, Written, plan::Plan};
use serde_json::Value;
use std::path::Path;

/// What a harness's store holds, counted without opening a session.
#[derive(Debug)]
pub struct StoreStats {
    /// The number of sessions, or why they could not be counted.
    pub sessions: Result<usize, String>,
    /// The number of forks, or why they could not be counted. `None` is a store that does not
    /// record lineage.
    pub forks: Option<Result<usize, String>>,
}

/// What mukade knows about one harness.
pub trait Harness: Send + Sync {
    fn id(&self) -> HarnessId;

    /// The reader of this harness's sessions.
    fn reader(&self) -> &'static dyn Reader;

    /// The writer of new sessions in this harness's store. `None` is a harness mukade only reads.
    fn writer(&self) -> Option<&'static dyn Writer>;

    /// Read one session from its native file (or, for OpenCode, its interchange document).
    fn read_file(&self, path: &Path) -> anyhow::Result<Session>;

    /// This harness's store in `roots`.
    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path>;

    /// Point this harness at an explicit store.
    fn set_store_root(&self, roots: &mut StoreRoots, root: &Path);

    /// Point a process of this harness at its store in `roots`, through the variable it reads.
    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command);

    /// The wire protocol this harness speaks as a destination. `None` is a harness that speaks
    /// whatever its configured provider speaks.
    fn destination_protocol(&self) -> Option<Protocol>;

    /// Whether this harness's own format has a place for an opaque reasoning payload (A4).
    fn payload_has_place(&self, opaque: &Opaque) -> bool;

    /// Whether the destination's model `provider` reads the provider envelope an opaque payload
    /// carries. Only a harness that keeps the envelope has a provider to ask.
    fn provider_reads_envelope(&self, _opaque: &Opaque, _provider: Option<&str>) -> bool {
        true
    }

    /// Whether an opaque reasoning payload is replayed here: the format has a place for it, and the
    /// provider reads the envelope it keeps.
    fn payload_fits(&self, opaque: &Opaque, provider: Option<&str>) -> bool {
        self.payload_has_place(opaque) && self.provider_reads_envelope(opaque, provider)
    }

    /// Check what a write asks of this destination: a reserved id in its own format, a reserved
    /// file inside its store, an agent and metadata. An option it does not take is refused, never
    /// silently ignored.
    fn validate_write(&self, opts: &WriteOpts) -> anyhow::Result<()>;

    /// Whether `id` has this harness's own session id shape, so its verdict on the id is the one
    /// given before the destination is known.
    fn claims_id(&self, _id: &str) -> bool {
        false
    }

    /// The interchange document the `document` command prints, for a harness whose host imports
    /// one itself. `None` is a harness without one.
    fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument>;

    /// The provider and model this harness's own configuration names as a destination, in
    /// `roots`. `None` is a configuration that names none.
    fn configured_model(&self, _roots: &StoreRoots) -> Option<(String, Option<String>)> {
        None
    }

    /// The project this harness already files sessions of `cwd` under, asked of its own binary
    /// `exe` from inside `cwd`. Only a harness that files sessions under a project has one; the
    /// others answer `Ok(None)`. `cache` is the caller's `mukade-data/cache` folder.
    fn project_id(
        &self,
        _exe: &str,
        _cwd: &str,
        _roots: &StoreRoots,
        _cache: Option<&Path>,
    ) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    /// Count the sessions and forks of this harness's store in `roots`.
    fn store_stats(&self, roots: &StoreRoots) -> StoreStats;

    /// Read one session from its native file with every branch the harness keeps, abandoned ones
    /// included. `None` is a harness that keeps one branch only.
    fn read_all_branches(&self, _path: &Path) -> Option<anyhow::Result<Session>> {
        None
    }

    /// [`Harness::read_all_branches`] for a session named by its id, read from the store in
    /// `roots`.
    fn read_all_branches_by_id(
        &self,
        _id: &str,
        _roots: &StoreRoots,
    ) -> Option<anyhow::Result<Session>> {
        None
    }

    /// Whether a session is summarized from a file given on the command line. Otherwise it is
    /// summarized from this harness's own store, by id.
    fn summarizes_from_file(&self) -> bool {
        false
    }

    /// The command that asks this harness's own binary for a summary of `session` and saves
    /// nothing in its store. `scratch` is the folder of this run, for what the harness has to
    /// write somewhere.
    fn summary_command(
        &self,
        session: &Session,
        opts: &SummarizeOpts,
        scratch: &Path,
    ) -> Result<SummaryCommand, HopError>;

    /// The summary as this harness's run printed it: `stdout`, and the content of the file the
    /// command was told to write (`output_file`). The caller trims it and refuses an empty one.
    fn parse_summary_reply(&self, stdout: &str, output_file: Option<&str>)
    -> Result<Reply, String>;

    /// What a summarizing run leaves in this harness's store, cleaned up once it has finished,
    /// whatever came of it. `stdout` is what the run printed. The command that removes what could
    /// not be cleaned up, if any.
    fn after_summary(
        &self,
        _command: &SummaryCommand,
        _roots: &StoreRoots,
        _stdout: &str,
    ) -> Option<String> {
        None
    }

    /// The tools this harness offers as a destination, for the version the bench pinned (tables
    /// §2, L11).
    fn offered(&self) -> ToolSet;

    /// One canonical call in this destination's own tool shape, or its text form. `input` is the
    /// call's input (a Codex patch already read as the write or edit it makes), `source_name` the
    /// name the source harness gave it, `dest_shell` the shell this destination runs commands
    /// with, when known. The caller checks the keys against [`Harness::accepted_keys`] and the name
    /// against the offered set.
    fn translate(
        &self,
        canonical: &CanonicalTool,
        source_name: &str,
        input: &Value,
        dest_shell: Option<Shell>,
    ) -> Translation;

    /// The argument keys, in any harness's spelling, that a translation into this destination maps
    /// or may drop because they do not change the recorded result (tables §4).
    fn accepted_keys(&self, canonical: &CanonicalTool) -> &'static [&'static str] {
        tools::common_keys(canonical)
    }

    /// Whether a step that ended aborted or in error is written as an ordinary reply, because the
    /// format records no such end.
    fn failed_steps_are_replies(&self) -> bool {
        true
    }

    /// Whether this destination's writer flattens a result's structured blocks to text, so the
    /// plan keeps them as they are and counts what is not text.
    fn flattens_result_blocks(&self) -> bool {
        false
    }
}

/// A destination's interchange document: a session laid out as the harness's own import takes it.
pub trait InterchangeDocument: Send + Sync {
    /// The tool bindings a session from `source_harness` carries into the document: calls a
    /// session of this same harness made keep their own tool, when the destination offers it.
    fn source_tool_map(
        &self,
        source_harness: HarnessId,
        source: &Session,
    ) -> Vec<(String, String, Value, Value)>;

    /// The document of a planned session, and the native compaction boundary it appended, if any.
    /// The id is `opts.dest_id`, or a placeholder in the harness's own format.
    fn document(
        &self,
        session: &Session,
        plan: &Plan,
        cwd: &str,
        opts: &WriteOpts,
        clock: &mut Clock,
    ) -> anyhow::Result<(Value, Option<NativeCompaction>)>;
}

/// The writer of a destination. A harness mukade only reads is refused (exit 2), before anything
/// is planned or written.
pub fn require_writer(harness: &dyn Harness) -> Result<&'static dyn Writer, HopError> {
    harness.writer().ok_or_else(|| {
        HopError::Unsupported(format!(
            "{} is a source only: mukade does not write its sessions",
            harness.id()
        ))
    })
}

/// The adapter of a harness.
pub fn get(id: HarnessId) -> &'static dyn Harness {
    match id {
        HarnessId::ClaudeCode => &claude_code::ClaudeCode,
        HarnessId::Codex => &codex::Codex,
        HarnessId::OpenCode => &opencode::OpenCode,
        HarnessId::Pi => &pi::Pi,
    }
}

/// Every adapter, in [`HarnessId::ALL`] order.
pub fn all() -> [&'static dyn Harness; 4] {
    HarnessId::ALL.map(get)
}

/// The counts of a harness whose sessions are listed by its reader and whose store does not
/// record lineage.
fn listed_stats(harness: &dyn Harness, roots: &StoreRoots) -> StoreStats {
    StoreStats {
        sessions: harness
            .reader()
            .list(roots)
            .map(|sessions| sessions.len())
            .map_err(|error| error.to_string()),
        forks: None,
    }
}

/// Set `variable` to the store root, when there is one; otherwise the process inherits it.
fn set_store_env(command: &mut std::process::Command, variable: &str, root: Option<&Path>) {
    if let Some(root) = root {
        command.env(variable, root);
    }
}

/// The write rules of a harness that keeps one file per session under its store, named by a UUID:
/// no agent or metadata, a reserved file inside the store, a reserved id that is a UUID.
fn validate_file_write(harness: &dyn Harness, opts: &WriteOpts) -> anyhow::Result<()> {
    if opts.dest_agent.is_some() || opts.metadata.is_some() {
        anyhow::bail!(
            "--dest-agent and --metadata-file are supported only for an OpenCode destination"
        );
    }
    if let Some(path) = &opts.dest_path {
        let root = harness
            .store_root(&opts.roots)
            .ok_or_else(|| anyhow::anyhow!("a destination root is required with --dest-path"))?;
        anyhow::ensure!(
            path.is_absolute() && path.starts_with(root),
            "--dest-path must stay inside the destination root"
        );
        anyhow::ensure!(
            !path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir)),
            "--dest-path must not contain parent traversal"
        );
    }
    if let Some(id) = opts.dest_id.as_deref() {
        uuid::Uuid::parse_str(id).map_err(|_| anyhow::anyhow!("invalid destination session ID"))?;
    }
    Ok(())
}

// The adapters hand out `&'static` readers and writers; `reader::for_harness` and
// `writer::for_harness` box them. A reference forwards every method, defaults included. A method
// added to `Reader` or `Writer` is forwarded here too, or a boxed reader or writer silently runs
// the trait's default.

impl<R: Reader + ?Sized> Reader for &R {
    fn harness(&self) -> HarnessId {
        (**self).harness()
    }

    fn list(&self, roots: &StoreRoots) -> anyhow::Result<Vec<SessionInfo>> {
        (**self).list(roots)
    }

    fn list_all(&self, roots: &StoreRoots) -> anyhow::Result<Vec<SessionInfo>> {
        (**self).list_all(roots)
    }

    fn read(&self, id: &str, roots: &StoreRoots) -> anyhow::Result<Session> {
        (**self).read(id, roots)
    }
}

impl<W: Writer + ?Sized> Writer for &W {
    fn harness(&self) -> HarnessId {
        (**self).harness()
    }

    fn offered(&self) -> crate::tools::ToolSet {
        (**self).offered()
    }

    fn write(&self, session: &Session, opts: &WriteOpts) -> anyhow::Result<Written> {
        (**self).write(session, opts)
    }

    fn write_planned(
        &self,
        session: &Session,
        plan: &Plan,
        opts: &WriteOpts,
    ) -> anyhow::Result<Written> {
        (**self).write_planned(session, plan, opts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hop::HopError;
    use crate::model::{Opaque, Protocol};

    /// A harness mukade only reads, as ADR-0002 makes Hermes.
    struct SourceOnly;

    impl Harness for SourceOnly {
        fn id(&self) -> HarnessId {
            HarnessId::Pi
        }

        fn reader(&self) -> &'static dyn Reader {
            &crate::reader::pi::Pi
        }

        fn writer(&self) -> Option<&'static dyn Writer> {
            None
        }

        fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
            anyhow::bail!("not read in this test: {}", path.display())
        }

        fn store_root<'a>(&self, _roots: &'a StoreRoots) -> Option<&'a Path> {
            None
        }

        fn set_store_root(&self, _roots: &mut StoreRoots, _root: &Path) {}

        fn apply_store_env(&self, _roots: &StoreRoots, _command: &mut std::process::Command) {}

        fn destination_protocol(&self) -> Option<Protocol> {
            None
        }

        fn payload_has_place(&self, _opaque: &Opaque) -> bool {
            false
        }

        fn validate_write(&self, _opts: &WriteOpts) -> anyhow::Result<()> {
            Ok(())
        }

        fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument> {
            None
        }

        fn store_stats(&self, _roots: &StoreRoots) -> StoreStats {
            StoreStats {
                sessions: Err("not counted in this test".into()),
                forks: None,
            }
        }

        fn summary_command(
            &self,
            _session: &Session,
            _opts: &SummarizeOpts,
            _scratch: &Path,
        ) -> Result<SummaryCommand, HopError> {
            Err(HopError::Unsupported("not summarized in this test".into()))
        }

        fn parse_summary_reply(
            &self,
            _stdout: &str,
            _output_file: Option<&str>,
        ) -> Result<Reply, String> {
            Err("not summarized in this test".into())
        }

        fn offered(&self) -> ToolSet {
            ToolSet { names: &[] }
        }

        fn translate(
            &self,
            _canonical: &CanonicalTool,
            _source_name: &str,
            _input: &Value,
            _dest_shell: Option<Shell>,
        ) -> Translation {
            tools::OTHER_CAPABILITY
        }
    }

    #[test]
    fn a_destination_without_a_writer_is_refused_before_planning() {
        let Err(refusal) = require_writer(&SourceOnly) else {
            panic!("a harness without a writer is not a destination");
        };
        assert!(matches!(refusal, HopError::Unsupported(_)), "{refusal}");
        assert_eq!(refusal.exit_code(), 2, "refused with a reason");
        for adapter in all() {
            assert!(require_writer(adapter).is_ok(), "{}", adapter.id());
        }
    }
}
