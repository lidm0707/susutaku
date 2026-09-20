#![allow(clippy::unwrap_used)] // tests: unwrap is the assertion tool

use core_agent::toolcall::mcp::handle_line;
use serde_json::Value;

fn resp(line: &str) -> Value {
    serde_json::from_str(&handle_line(line).unwrap()).unwrap()
}

#[test]
fn initialize_returns_server_info() {
    let r = resp(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
    assert_eq!(r["result"]["serverInfo"]["name"], "susutaku");
    assert!(r["result"]["protocolVersion"].is_string());
}

#[test]
fn tools_list_exposes_platform_tools() {
    let r = resp(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
    let names: Vec<&str> = r["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["fetch", "web_search", "definition", "create_card"]);
}

#[test]
fn unknown_tool_is_tool_error() {
    let r = resp(
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
    );
    assert_eq!(r["result"]["isError"], true);
    assert!(
        r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("unknown tool")
    );
}

#[test]
fn missing_argument_is_tool_error() {
    let r = resp(
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"fetch","arguments":{}}}"#,
    );
    assert_eq!(r["result"]["isError"], true);
    assert!(
        r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("missing string argument: url")
    );
}

#[test]
fn notification_and_garbage_yield_no_response() {
    assert!(handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    assert!(handle_line("not json at all").is_none());
}

#[test]
fn unknown_method_is_jsonrpc_error() {
    let r = resp(r#"{"jsonrpc":"2.0","id":5,"method":"bogus/method"}"#);
    assert_eq!(r["error"]["code"], -32601);
}
