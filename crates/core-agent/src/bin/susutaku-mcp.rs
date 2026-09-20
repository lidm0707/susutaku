//! susutaku MCP stdio bridge: exposes core-agent tools to codex over
//! newline-delimited JSON-RPC on stdin/stdout.

fn main() {
    core_agent::toolcall::mcp::serve();
}
