//! Readers: native session store → [`Session`] (docs/session-interchange-spec.md §8.1).

mod branches;
pub mod claude_code;
mod claude_history;
mod claude_live;
pub mod clip;
pub mod codex;
mod codex_state;
pub mod deepseek_harness;
pub mod hermes;
pub mod json;
pub mod opencode;
pub mod origin;
pub mod pi;
pub mod zstd_frames;

use crate::model::{Entry, HarnessId, Meta, ParentKind, Session};
use crate::roots::StoreRoots;

pub use clip::{Clip, Clipped, Clipping, MIN_STRING_BYTES, STREAM_FROM_BYTES, clip_session};

/// What a reader knows about a session before reading it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionInfo {
    pub id: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub updated: Option<String>,
    /// Whether the source marked this session archived.
    pub archived: bool,
    /// The native file (or store record) the session lives in.
    pub path: String,
    /// The native parent link, when the harness records one. `None` is "not recorded or no
    /// parent", never a guess.
    pub parent: Option<String>,
    /// How the session came from [`SessionInfo::parent`]: set whenever `parent` is.
    pub parent_kind: Option<ParentKind>,
    /// Why [`Reader::read`] cannot hand this session back, when the listing already knows: the
    /// store still names the session but its transcript is gone (a Codex `threads` row whose
    /// rollout was deleted, a Claude Code prompt in `history.jsonl` without a session file), or
    /// it lives in a store this listing was not pointed at. `None` is "nothing known against
    /// it", not a promise that the read will work.
    pub unreadable: Option<String>,
    /// Whether the harness has a process running this session now, for a harness whose store
    /// records that (Claude Code's `sessions/<pid>.json`). `None` is "not recorded, or not
    /// checkable from here"; it is never `false` by default.
    pub live: Option<bool>,
}

/// One reader per harness. It turns native records into [`Session`], keeping every source record
/// verbatim in the entry's `raw`.
pub trait Reader {
    fn harness(&self) -> HarnessId;

    /// Every root session in the store, in a stable order.
    fn list(&self, roots: &StoreRoots) -> anyhow::Result<Vec<SessionInfo>>;

    /// Every session the store holds, forks and subagent sessions included, in a stable order. A
    /// harness that has no cross-session parent link returns the same list as [`Reader::list`].
    fn list_all(&self, roots: &StoreRoots) -> anyhow::Result<Vec<SessionInfo>> {
        self.list(roots)
    }

    /// Read one session by its native id (an unambiguous prefix is accepted where the store's file
    /// names allow it).
    fn read(&self, id: &str, roots: &StoreRoots) -> anyhow::Result<Session>;

    /// What the listing says about one session, without listing the others: the same
    /// [`SessionInfo`] [`Reader::list_all`] gives for `id`, read from the session's own file at
    /// the two ends the listing reads, never its conversation. `id` is the whole id a listing
    /// shows. [`Lookup::NotFound`] and [`Lookup::Ambiguous`] say what a missing or repeated id is.
    ///
    /// The default lists the whole store and picks the row; Claude Code, Codex and Pi read one
    /// session's head and tail, so the cost does not grow with the session.
    fn overview(&self, id: &str, roots: &StoreRoots) -> anyhow::Result<SessionInfo> {
        pick_overview(self.list_all(roots)?, id, self.harness())
    }

    /// [`Reader::read`] that never refuses a session for its size: every string longer than
    /// `max_string_bytes` is cut to a prefix, and the session is built from the clipped records,
    /// so its entries, ids, kinds and order are the full read's. Up to [`STREAM_FROM_BYTES`] the
    /// session is read whole and clipped; past it the file is decoded one record at a time and
    /// its text is never held. `max_string_bytes` below [`MIN_STRING_BYTES`] is raised to it.
    /// The [`Clipped`] says what was cut. Claude Code, Codex and Pi stream; the default reads
    /// the session whole and clips it.
    fn read_clipped(
        &self,
        id: &str,
        roots: &StoreRoots,
        max_string_bytes: usize,
    ) -> anyhow::Result<Clipped> {
        clip_session(&self.read(id, roots)?, max_string_bytes)
    }

    /// The entries of [`Reader::read`], in its order and with its full content, handed to
    /// `on_entry` one at a time; the returned [`Meta`] is the read's. Claude Code and Pi find the
    /// order from a clipped pass, then build each entry from the full records read back by
    /// offset, so memory follows the session's structure and its biggest entry, not its text. The
    /// default (and Codex) reads the session whole first.
    fn read_streamed(
        &self,
        id: &str,
        roots: &StoreRoots,
        on_entry: &mut dyn FnMut(Entry) -> anyhow::Result<()>,
    ) -> anyhow::Result<Meta> {
        self.read_streamed_with_meta(id, roots, &mut |_| Ok(()), on_entry)
    }

    /// [`Reader::read_streamed`] that calls `on_meta` **once, before the first entry**, with the
    /// session's [`Meta`]: what a document that states its meta first (`wire::DocumentWriter`)
    /// needs. Claude Code and Pi have it after the first pass, from clipped records: it equals the
    /// returned one unless a fact of the meta is longer than the clip (a working directory of over
    /// a kilobyte); the writer checks that and refuses rather than write a wrong one. The default
    /// has the whole session, so its meta is exact.
    fn read_streamed_with_meta(
        &self,
        id: &str,
        roots: &StoreRoots,
        on_meta: &mut dyn FnMut(&Meta) -> anyhow::Result<()>,
        on_entry: &mut dyn FnMut(Entry) -> anyhow::Result<()>,
    ) -> anyhow::Result<Meta> {
        let session = self.read(id, roots)?;
        on_meta(&session.meta)?;
        for entry in session.entries {
            on_entry(entry)?;
        }
        Ok(session.meta)
    }

    /// [`Reader::read`] with every branch the session left carried as a [`crate::model::Branch`]
    /// instead of left behind as off-branch records. `None`: a harness that keeps one branch only.
    fn read_all_branches(&self, _id: &str, _roots: &StoreRoots) -> Option<anyhow::Result<Session>> {
        None
    }

    /// [`Reader::read_all_branches`] for a session in a native file.
    fn read_file_all_branches(&self, _path: &std::path::Path) -> Option<anyhow::Result<Session>> {
        None
    }

    /// Whether [`Reader::read_all_branches`] reads this harness's branches, answered without
    /// reading anything. It holds exactly when the two reads above answer `Some`.
    fn reads_all_branches(&self) -> bool {
        false
    }
}

/// A session a reader could not hand back for a reason that is a refusal, not a file it failed to
/// understand: the store is not configured, the session is not there, or the id names several.
/// A hop refuses these (exit 2); any other read error means the source could not be read (exit 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    NoStore(String),
    NotFound(String),
    Ambiguous(String),
}

impl std::fmt::Display for Lookup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lookup::NoStore(what) | Lookup::NotFound(what) | Lookup::Ambiguous(what) => {
                f.write_str(what)
            }
        }
    }
}

impl std::error::Error for Lookup {}

/// The reader for a harness.
pub fn for_harness(harness: HarnessId) -> anyhow::Result<Box<dyn Reader>> {
    Ok(Box::new(crate::harnesses::get(harness).reader()))
}

/// The one row of `listed` that is session `id`, whole-id match.
pub(crate) fn pick_overview(
    listed: Vec<SessionInfo>,
    id: &str,
    harness: HarnessId,
) -> anyhow::Result<SessionInfo> {
    let mut found: Vec<SessionInfo> = listed.into_iter().filter(|info| info.id == id).collect();
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(Lookup::NotFound(format!("no {harness} session with id {id:?}")).into()),
        many => Err(Lookup::Ambiguous(format!("{id:?} names {many} {harness} sessions")).into()),
    }
}
