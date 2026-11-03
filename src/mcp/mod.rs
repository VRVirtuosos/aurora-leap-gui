//! Model Context Protocol (MCP) Server for Aurora-Viewer LEAP Studio.
//!
//! Exposes Second Life & OpenSim viewer subsystems to AI agents via JSON-RPC 2.0 stdio transport.

pub mod server;
pub mod tools;
pub mod types;

#[cfg(test)]
mod tests;

pub use server::run_mcp_stdio_server;
pub use tools::{dispatch_tool_call, get_tool_definitions};
pub use types::{McpCallResult, McpContent, McpToolDefinition};
