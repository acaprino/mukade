//! Authenticated local MCP chat service and official SDK client.
pub mod client;
pub mod adapter;
pub mod connect;
mod http_body;
pub mod lifecycle;
pub mod security;
pub mod server;
pub use server::{ChatService, Health, ServiceConfig};
