//! DeepSeek Harness (`dsh`): session logs under `DSH_HOME/sessions/`.

use super::{Destination, Harness, InterchangeDocument, StoreStats};
use crate::hop::HopError;
use crate::model::{CanonicalTool, HarnessId, Opaque, Protocol, Session};
use crate::reader::Reader;
use crate::roots::StoreRoots;
use crate::summary::{Reply, SummarizeOpts, SummaryCommand};
use crate::tools::{self, EditCall, GrepCall, Shell, ToolSet, Translation};
use crate::writer::{WriteOpts, Writer, media::Image};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Why `--compaction summary` is refused for a DeepSeek Harness session.
const NO_SUMMARY: &str = "--compaction summary is not available from a DeepSeek Harness session: dsh has no run that saves nothing. `dsh --profile headless --session-id <id>` adopts the session and appends the question and the answer to its log; without `--session-id` the model cannot see the session, and no flag turns the log off or the tools away. Use --compaction native or --compaction portable instead";

pub(super) struct DeepSeekHarness;

impl Harness for DeepSeekHarness {
    fn id(&self) -> HarnessId {
        HarnessId::DeepSeekHarness
    }

    fn reader(&self) -> &'static dyn Reader {
        &crate::reader::deepseek_harness::DeepSeekHarness
    }

    fn destination(&self) -> Option<&dyn Destination> {
        Some(self)
    }

    fn read_file(&self, path: &Path) -> anyhow::Result<Session> {
        crate::reader::deepseek_harness::read_file(path)
    }

    /// A `user/message` whose `source.kind` is `user` is what the person typed.
    fn user_origin(&self, raw: Option<&Value>) -> crate::reader::origin::Origin {
        raw.map_or(crate::reader::origin::Origin::Unknown, |event| {
            crate::reader::deepseek_harness::user_origin(event)
        })
    }

    fn store_root<'a>(&self, roots: &'a StoreRoots) -> Option<&'a Path> {
        roots.dsh_home.as_deref()
    }

    fn store_root_mut<'a>(&self, roots: &'a mut StoreRoots) -> &'a mut Option<PathBuf> {
        &mut roots.dsh_home
    }

    fn apply_store_env(&self, roots: &StoreRoots, command: &mut std::process::Command) {
        let variable = crate::catalog::store_env(self.id());
        super::set_store_env(command, variable, self.store_root(roots));
    }

    /// A subagent's or a fork's log names its parent in its own header.
    fn store_stats(&self, roots: &StoreRoots) -> StoreStats {
        let counted = match roots.dsh_sessions() {
            Some(sessions) => crate::reader::deepseek_harness::store_counts(&sessions),
            None => Err(anyhow::anyhow!(
                "no DeepSeek Harness sessions folder to count"
            )),
        };
        match counted {
            Ok((sessions, forks)) => StoreStats {
                sessions: Ok(sessions),
                forks: Some(Ok(forks)),
            },
            Err(error) => {
                let error = format!("{error:#}");
                StoreStats {
                    sessions: Err(error.clone()),
                    forks: Some(Err(error)),
                }
            }
        }
    }

    /// dsh 0.2.2 ships the profiles `headless`, `web`, `acp` and `sdk`: none is a terminal UI that
    /// opens a session. The headless profile continues one with a task.
    fn resume_refusal(&self, info: &crate::reader::SessionInfo) -> Option<String> {
        Some(format!(
            "DeepSeek Harness has no terminal UI to open a session in; continue it with a task: dsh --profile headless --session-id {} \"<task>\"",
            info.id
        ))
    }

    /// Refused: dsh has no run that saves nothing. `--session-id <id>` adopts the session and
    /// appends to its log; a run without it cannot see the session; the shipped profiles all keep
    /// their log, and `dsh` has no flag that turns the tools off or the log away.
    fn summary_command(
        &self,
        _session: &Session,
        _opts: &SummarizeOpts,
        _scratch: &Path,
    ) -> Result<SummaryCommand, HopError> {
        Err(HopError::Unsupported(NO_SUMMARY.into()))
    }

    fn parse_summary_reply(
        &self,
        _stdout: &str,
        _output_file: Option<&str>,
    ) -> Result<Reply, String> {
        Err("DeepSeek Harness has no summary run".into())
    }
}

impl Destination for DeepSeekHarness {
    fn writer(&self) -> &'static dyn Writer {
        &crate::writer::deepseek_harness::DeepSeekHarness
    }

    /// dsh speaks whatever its configured provider speaks.
    fn protocol(&self) -> Option<Protocol> {
        None
    }

    /// An assistant message in dsh's log keeps no signature or encrypted payload of its own: its
    /// provider's replay state is the adapter's, and a message without one is read as another
    /// provider's.
    fn payload_has_place(&self, _opaque: &Opaque) -> bool {
        false
    }

    /// A session is found by its id and its project folder, so the id is dsh's own shape
    /// (`session-<uuid>`) or a UUID, and a path inside the store is not asked for. Native compaction
    /// is not written, and the agent and metadata are OpenCode's.
    fn validate_write(&self, opts: &WriteOpts) -> anyhow::Result<()> {
        if opts.dest_agent.is_some() || opts.metadata.is_some() {
            anyhow::bail!(
                "--dest-agent and --metadata-file are supported only for an OpenCode destination"
            );
        }
        if opts.dest_path.is_some() {
            anyhow::bail!(
                "--dest-path is not supported toward DeepSeek Harness: dsh finds a session by its \
                 id and its project folder"
            );
        }
        if opts.native_budget.is_some() {
            anyhow::bail!("--compaction native is not supported toward DeepSeek Harness");
        }
        if let Some(id) = opts.dest_id.as_deref() {
            let uuid = id.strip_prefix("session-").unwrap_or(id);
            uuid::Uuid::parse_str(uuid)
                .map_err(|_| anyhow::anyhow!("invalid destination session ID"))?;
        }
        Ok(())
    }

    fn claims_id(&self, id: &str) -> bool {
        id.strip_prefix("session-")
            .is_some_and(|rest| uuid::Uuid::parse_str(rest).is_ok())
    }

    fn interchange_document(&self) -> Option<&'static dyn InterchangeDocument> {
        None
    }

    /// The tools the headless profile of dsh 0.2.2-rc.1 offers that a call can be translated
    /// into: the files, search and fetch tools, and the shell dsh runs where it is installed
    /// (`bash` away from Windows, `pwsh` on it).
    fn offered(&self) -> ToolSet {
        ToolSet {
            names: if cfg!(windows) { WINDOWS } else { POSIX },
        }
    }

    /// dsh's shell tool is the one of its platform.
    fn default_shell(&self, _roots: &StoreRoots, _cwd: Option<&Path>) -> Option<Shell> {
        Some(if cfg!(windows) {
            Shell::PowerShell
        } else {
            Shell::Posix
        })
    }

    fn translate(
        &self,
        canonical: &CanonicalTool,
        _source_name: &str,
        input: &Value,
        dest_shell: Option<Shell>,
    ) -> Translation {
        match canonical {
            CanonicalTool::Bash | CanonicalTool::PowerShell => shell(canonical, input, dest_shell),
            CanonicalTool::Read => read(input),
            CanonicalTool::Write => write(input),
            CanonicalTool::Edit | CanonicalTool::MultiEdit => edit(canonical, input),
            CanonicalTool::Grep => grep(input),
            CanonicalTool::Glob => match tools::glob_input(input) {
                Ok(input) => Translation::Call {
                    name: "glob",
                    input,
                },
                Err(text) => text,
            },
            CanonicalTool::Ls => Translation::Text {
                why: "no ls tool offered",
            },
            CanonicalTool::WebFetch => match tools::url_of(input) {
                Ok(url) => Translation::Call {
                    name: "web_fetch",
                    input: tools::obj([("url", url)]),
                },
                Err(text) => text,
            },
            CanonicalTool::WebSearch => Translation::Text {
                why: "no web search tool offered",
            },
            _ => tools::OTHER_CAPABILITY,
        }
    }

    /// Only the arguments dsh's own tool takes: any other changes what the call did.
    fn accepted_keys(&self, canonical: &CanonicalTool) -> &'static [&'static str] {
        match canonical {
            CanonicalTool::Bash | CanonicalTool::PowerShell => {
                &["command", "cmd", "timeout", "description", "workdir"]
            }
            CanonicalTool::Read => &["file_path", "path", "offset", "limit"],
            CanonicalTool::Grep => &["pattern", "path", "glob", "include"],
            CanonicalTool::Glob => &["pattern", "path"],
            CanonicalTool::WebFetch => &["url"],
            _ => tools::common_keys(canonical),
        }
    }

    /// dsh's tool results hold text and its own attachment parts: the writer keeps those and
    /// drops the rest.
    fn flattens_result_blocks(&self) -> bool {
        true
    }

    /// An image in dsh's log is a reference to a file in its attachment store, which mukade does
    /// not write: no image of another harness is carried.
    fn image_in_result(&self, _image: &Image) -> Value {
        Value::Null
    }
}

/// The tools of the headless profile, with the shell of the platform.
const WINDOWS: &[&str] = &["read", "write", "edit", "glob", "grep", "web_fetch", "pwsh"];
const POSIX: &[&str] = &["read", "write", "edit", "glob", "grep", "web_fetch", "bash"];

// ---------------------------------------------------------------------------------------------
// tools toward DeepSeek Harness

/// dsh's shell tools: `command` and a required `description`, `timeoutMs` in milliseconds (the
/// canonical unit) and `workdir`. A call without a description is not one dsh's tool accepts.
fn shell(canonical: &CanonicalTool, input: &Value, dest_shell: Option<Shell>) -> Translation {
    if let Some(text) = tools::other_dialect(canonical, dest_shell) {
        return text;
    }
    let Some(command) = tools::command_of(input) else {
        return Translation::Text { why: "no command" };
    };
    if tools::in_background(input) {
        return tools::NO_BACKGROUND;
    }
    let Some(description) = tools::required(input, "description") else {
        return Translation::Text {
            why: "dsh's shell tool requires a description",
        };
    };
    let mut pairs: Vec<(&str, Value)> = vec![("description", description), ("command", command)];
    if let Some(timeout) = tools::arg(input, "timeout") {
        pairs.push(("timeoutMs", timeout.clone()));
    }
    if let Some(workdir) = tools::arg(input, "workdir") {
        pairs.push(("workdir", workdir.clone()));
    }
    Translation::Call {
        name: if *canonical == CanonicalTool::PowerShell {
            "pwsh"
        } else {
            "bash"
        },
        input: tools::obj_pairs(pairs),
    }
}

/// dsh's `read`: `file_path`, `offset` (1-based) and `limit`.
fn read(input: &Value) -> Translation {
    if tools::arg(input, "pages").is_some() || tools::arg(input, "allow_large").is_some() {
        return Translation::Text {
            why: "pages/allow_large has no counterpart",
        };
    }
    let Some(path) = tools::path_of(input) else {
        return Translation::Text { why: "no path" };
    };
    let mut pairs: Vec<(&str, Value)> = vec![("file_path", path)];
    for key in ["offset", "limit"] {
        if let Some(value) = tools::arg(input, key) {
            pairs.push((key, value.clone()));
        }
    }
    Translation::Call {
        name: "read",
        input: tools::obj_pairs(pairs),
    }
}

fn write(input: &Value) -> Translation {
    match tools::path_of(input) {
        Some(path) => Translation::Call {
            name: "write",
            input: tools::obj([
                ("file_path", path),
                ("content", tools::required_content(input)),
            ]),
        },
        None => Translation::Text { why: "no path" },
    }
}

/// dsh's `edit` makes one change and has `replace_all`.
fn edit(canonical: &CanonicalTool, input: &Value) -> Translation {
    let (path, old, new, replace_all) = match tools::edit_call(canonical, input) {
        Ok(EditCall::One {
            path,
            old,
            new,
            replace_all,
        }) => (path, old, new, replace_all),
        Ok(EditCall::Several { .. }) => return tools::NO_MULTI_EDIT,
        Err(text) => return text,
    };
    let mut pairs: Vec<(&str, Value)> = vec![
        ("file_path", path),
        ("old_string", old),
        ("new_string", new),
    ];
    if replace_all {
        pairs.push(("replace_all", Value::Bool(true)));
    }
    Translation::Call {
        name: "edit",
        input: tools::obj_pairs(pairs),
    }
}

/// dsh's `grep`: `pattern`, `path` and `include`; it prints the matching lines with their numbers.
fn grep(input: &Value) -> Translation {
    let GrepCall {
        mut pairs,
        filter,
        limit,
        context,
        ignore_case,
        literal,
    } = match tools::grep_call(input) {
        Ok(grep) => grep,
        Err(text) => return text,
    };
    if limit.is_some() || context.is_some() || ignore_case.is_some() || literal.is_some() {
        return Translation::Text {
            why: "an argument has no counterpart",
        };
    }
    if let Some(filter) = filter {
        pairs.push(("include", filter));
    }
    Translation::Call {
        name: "grep",
        input: tools::obj_pairs(pairs),
    }
}
