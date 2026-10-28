//! Mukade: move a coding-agent session between harnesses, as one of the destination's own.
//!
//! Reads and writes the native session stores of Claude Code, Codex CLI, OpenCode and Pi, so a
//! conversation held in one of them can be continued by another, natively: no proxy, no
//! re-prompting the history, no third-party converter.
//!
//! It began as the session-interchange library of Shinatama. The specification, the tool tables
//! and the conformance report ship with this repository under `docs/`; the bench itself — the
//! scripts that run the real harnesses over a mock model — belongs to the Shinatama product. Every
//! reader and writer here was built against that bench's fixtures, which the tests under
//! `fixtures/` carry.

pub mod cache;
pub mod cli;
pub mod detect;
pub mod exe;
pub mod hop;
pub mod manifest;
pub mod model;
pub mod policy;
pub mod portable;
pub mod reader;
pub mod roots;
pub mod sha256;
pub mod strict;
pub mod summary;
pub mod timing;
pub mod tools;
pub mod wire;
pub mod writer;

pub use hop::{HopError, HopRequest, Receipt, export, hop, import};
pub use manifest::Manifest;
pub use model::{Entry, HarnessId, Kind, Meta, Session};
pub use reader::Reader;
pub use writer::{WriteOpts, Writer, Written};
