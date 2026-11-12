//! Authenticated local MCP chat service and official SDK client.
pub mod client;
pub mod lifecycle;
pub mod security;
mod http_body;
pub mod server;
pub use server::{ChatService, Health, ServiceConfig};
