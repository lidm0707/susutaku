//! Minimal MCP (Model Context Protocol) stdio server: newline-delimited
//! JSON-RPC 2.0 exposing the agent `Tool` set to external agent CLIs
//! (codex) spawned by the backend. The binary is a thin stdin/stdout loop
//! around [`handle_line`].

use super::Tool;
use serde_json::{Value, json};
use std::io::{BufRead, Write};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "susutaku";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const METHOD_NOT_FOUND: i64 = -32601;

/// One MCP tool descriptor: name, model-facing description, JSON-schema.
struct ToolDef {
    name: &'static str,
    description: &'static str,
    schema: Value,
}

fn tool_defs() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "fetch",
            description: "Fetch a URL over HTTP(S) and return its content as markdown text.",
            schema: json!({
                "type": "object",
                "properties": { "url": { "type": "string" } },
                "required": ["url"]
            }),
        },
        ToolDef {
            name: "web_search",
            description: "Search the web and return a summarized result list.",
            schema: json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: "definition",
            description: "LSP go-to-definition: resolve the symbol at a file position.",
            schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "text": { "type": "string" },
                    "line": { "type": "integer" },
                    "col": { "type": "integer" }
                },
                "required": ["path", "text", "line", "col"]
            }),
        },
        ToolDef {
            name: "create_card",
            description: "Create a task card on the susutaku board.",
            schema: json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer" },
                    "title": { "type": "string" },
                    "description": { "type": ["string", "null"] }
                },
                "required": ["project_id", "title"]
            }),
        },
    ]
}

/// Handle one JSON-RPC line: `Some(response)` for requests, `None` for
/// notifications and unparseable input (both are silently dropped per spec).
pub fn handle_line(line: &str) -> Option<String> {
    let req: Value = serde_json::from_str(line).ok()?;
    let id = req.get("id")?.clone();
    let method = req.get("method").and_then(Value::as_str)?;
    match method {
        "initialize" => Some(response(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION }
            }),
        )),
        "tools/list" => Some(response(id, json!({ "tools": tool_list() }))),
        "tools/call" => Some(response(id, call(req.get("params")))),
        "ping" => Some(response(id, json!({}))),
        _ => Some(error(
            id,
            METHOD_NOT_FOUND,
            format!("unknown method: {method}"),
        )),
    }
}

fn tool_list() -> Vec<Value> {
    tool_defs()
        .iter()
        .map(|d| {
            json!({
                "name": d.name,
                "description": d.description,
                "inputSchema": d.schema
            })
        })
        .collect()
}

fn call(params: Option<&Value>) -> Value {
    let result = params
        .and_then(|p| dispatch(p).transpose())
        .unwrap_or_else(|| Err("missing params".into()));
    match result {
        Ok(text) => json!({
            "content": [{ "type": "text", "text": text }],
            "isError": false
        }),
        Err(e) => json!({
            "content": [{ "type": "text", "text": e }],
            "isError": true
        }),
    }
}

/// `Ok(None)` for an unknown tool name (mapped to an MCP tool error).
fn dispatch(params: &Value) -> Result<Option<String>, String> {
    const UNKNOWN: &str = "unknown tool";
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or("missing tool name")?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let tool = match name {
        "fetch" => Tool::Fetch {
            url: arg(&args, "url")?,
        },
        "web_search" => Tool::WebSearch {
            query: arg(&args, "query")?,
        },
        "definition" => Tool::Definition {
            path: arg(&args, "path")?,
            text: arg(&args, "text")?,
            line: num(&args, "line")?,
            col: num(&args, "col")?,
        },
        "create_card" => Tool::CreateCard {
            project_id: num(&args, "project_id")?,
            title: arg(&args, "title")?,
            description: args
                .get("description")
                .and_then(Value::as_str)
                .map(String::from),
        },
        _ => return Err(format!("{UNKNOWN}: {name}")),
    };
    Ok(Some(tool.run()?))
}

fn arg(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| format!("missing string argument: {key}"))
}

fn num<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(
        args.get(key)
            .cloned()
            .ok_or_else(|| format!("missing argument: {key}"))?,
    )
    .map_err(|e| format!("bad argument {key}: {e}"))
}

fn response(id: Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn error(id: Value, code: i64, message: String) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

/// Stdio entry point: read request lines, write response lines.
pub fn serve() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if let Some(resp) = handle_line(&line) {
            if writeln!(out, "{resp}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
}
