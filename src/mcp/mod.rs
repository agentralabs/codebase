//! Model Context Protocol server interface.
//!
//! Provides a synchronous JSON-RPC 2.0 server implementation for the
//! Model Context Protocol (MCP). Exposes code graph operations as
//! tools, resources, and prompts.

pub mod protocol;
pub mod server;
pub mod sse;
pub mod tenant;

pub use protocol::{parse_request, JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub use server::McpServer;

use std::io::{self, BufRead, Write};

/// Run the MCP server on stdin/stdout.
///
/// Reads newline-delimited JSON-RPC messages from stdin and writes
/// responses to stdout.  This is the entry point used by the
/// `agentic-codebase-mcp` binary.
pub fn serve_stdio() {
    let mut server = McpServer::new();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = server.handle_raw(trimmed);
        if response.is_empty() {
            continue;
        }
        if writeln!(stdout, "{}", response).is_err() {
            break;
        }
        if stdout.flush().is_err() {
            break;
        }
    }
}
