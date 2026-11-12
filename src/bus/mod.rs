//! Local chat domain, durable receipts and reduced views of read-only native stores.
mod chat_delivery;
mod chat_domain;
pub(crate) mod chat_mutation;
pub(crate) mod chat_observe;
pub(crate) mod chat_query;
mod chat_schema;
mod chat_tools;
mod error;
mod ledger;
mod observe;
mod room;
pub mod secrets;
mod store;
pub mod view;
pub use error::BusError;
pub use ledger::{EventRow, HistoryFilter, ObservationRow};
pub use observe::{Observation, ObserveOpts, ObservedKind, Shown};
pub use room::{BoundSession, JoinRequest, Member, Role, RoomRow, RoomSummary};
pub use store::Bus;

pub use chat_domain::ChatError;
pub(crate) use chat_observe::read_opencode as read_opencode_bound;
pub use chat_tools::chat_tool_definitions;
