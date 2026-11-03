//! MCP Stdio Transport Server.
//!
//! Listens for newline-delimited JSON-RPC 2.0 messages on `stdin`, dispatches
//! requests to the MCP tool handlers and Aurora LEAP client, and writes JSON-RPC
//! responses to `stdout`.

use std::io::{self, BufRead, Write};
use serde_json::{json, Value};

use crate::client::LeapClient;
use crate::mcp::tools::{dispatch_tool_call, get_tool_definitions};
use crate::mcp::types::{JsonRpcRequest, JsonRpcResponse};

/// Run the MCP server over standard I/O until EOF or termination.
pub fn run_mcp_stdio_server() -> io::Result<()> {
    eprintln!("[MCP Server] Starting Aurora-Viewer LEAP MCP stdio server...");
    let mut client = LeapClient::new();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[MCP Server] Error reading stdin: {e}");
                break;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(err) => {
                eprintln!("[MCP Server] JSON parse error: {err} on input: {trimmed}");
                let err_resp = JsonRpcResponse::error(None, -32700, format!("Parse error: {err}"));
                let _ = writeln!(stdout_lock, "{}", serde_json::to_string(&err_resp).unwrap());
                let _ = stdout_lock.flush();
                continue;
            }
        };

        // Handle notifications (no response required when id is None, except when spec calls for it)
        if request.method == "notifications/initialized" {
            eprintln!("[MCP Server] Client handshake completed (notifications/initialized received).");
            continue;
        }

        let id = request.id.clone();
        let resp = match request.method.as_str() {
            "initialize" => {
                eprintln!("[MCP Server] Handling initialize request...");
                JsonRpcResponse::ok(
                    id,
                    json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": {
                            "tools": {}
                        },
                        "serverInfo": {
                            "name": "aurora-viewer-mcp",
                            "version": "0.1.0"
                        }
                    }),
                )
            }

            "ping" => JsonRpcResponse::ok(id, json!({})),

            "tools/list" => {
                let tools = get_tool_definitions();
                JsonRpcResponse::ok(id, json!({ "tools": tools }))
            }

            "tools/call" => {
                let params = request.params.unwrap_or(Value::Null);
                let tool_name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let empty_map = json!({});
                let tool_args = params.get("arguments").unwrap_or(&empty_map);

                eprintln!("[MCP Server] Tool invoked: '{tool_name}'");
                let call_result = dispatch_tool_call(&mut client, tool_name, tool_args);

                JsonRpcResponse::ok(
                    id,
                    json!({
                        "content": call_result.content,
                        "isError": call_result.is_error
                    }),
                )
            }

            "resources/list" => JsonRpcResponse::ok(id, json!({ "resources": [] })),

            "prompts/list" => JsonRpcResponse::ok(id, json!({ "prompts": [] })),

            other => {
                eprintln!("[MCP Server] Unknown JSON-RPC method: '{other}'");
                JsonRpcResponse::error(id, -32601, format!("Method not found: '{other}'"))
            }
        };

        if let Ok(json_line) = serde_json::to_string(&resp) {
            let _ = writeln!(stdout_lock, "{json_line}");
            let _ = stdout_lock.flush();
        }
    }

    eprintln!("[MCP Server] Stdin closed, shutting down MCP server...");
    client.disconnect();
    Ok(())
}
