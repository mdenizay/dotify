//! Dotify's Model Context Protocol server.
//!
//! The tools in [`tools`] drive `dotify_core::Editor`, the same command
//! engine as the desktop app and CLI. [`server`] adapts them to MCP and
//! [`http`] serves them over Streamable HTTP with bearer-token auth; the
//! binary also serves them over stdio.

pub mod args;
pub mod error;
pub mod http;
pub mod sandbox;
pub mod server;
pub mod tools;
pub mod workspace;

pub use error::{ToolError, ToolResult};
pub use sandbox::Sandbox;
pub use server::McpServer;
pub use tools::{Dotify, Options, ToolOutput};
