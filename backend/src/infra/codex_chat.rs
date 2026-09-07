//! Chat through the codex CLI (`codex exec --json`) using the shared login.

use serde_json::Value;
use std::path::Path;

use crate::infra::codex_auth::codex_home;

const AGENT_EVENT: &str = "agent_message";
const ERROR_EVENT: &str = "error";

/// Run one prompt through codex inside `workspace`, return the final agent text.
pub fn chat(prompt: &str, model: Option<&str>, workspace: &Path) -> Result<String, String> {
    codex_cli::check_available().map_err(|e| format!("codex CLI unavailable: {e}"))?;
    let mut child = codex_cli::exec_json(prompt, model, &codex_home(), workspace)
        .map_err(|e| format!("codex spawn failed: {e}"))?;
    let mut text = String::new();
    let mut err = String::new();
    codex_cli::drain_events(&mut child, |line| extract(line, &mut text, &mut err))
        .map_err(|e| format!("codex stream failed: {e}"))?;
    if text.is_empty() {
        return Err(if err.is_empty() {
            "codex returned no reply".to_string()
        } else {
            err
        });
    }
    Ok(text)
}

/// Collect agent messages and error payloads from the codex JSONL event stream.
fn extract(line: &str, text: &mut String, err: &mut String) {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return;
    };
    if let Some(m) = v.get("msg") {
        if m.get("type").and_then(Value::as_str) == Some(AGENT_EVENT) {
            push_str(text, m.get("message"));
        }
        if m.get("type").and_then(Value::as_str) == Some(ERROR_EVENT) {
            push_str(err, m.get("message"));
        }
    }
    if let Some(item) = v.get("item")
        && item.get("type").and_then(Value::as_str) == Some(AGENT_EVENT)
    {
        push_str(text, item.get("text"));
    }
}

fn push_str(into: &mut String, v: Option<&Value>) {
    if let Some(s) = v.and_then(Value::as_str) {
        into.push_str(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_agent_message_new_and_old_shapes() {
        let mut text = String::new();
        let mut err = String::new();
        extract(
            r#"{"id":"1","item":{"type":"agent_message","text":"hello"}}"#,
            &mut text,
            &mut err,
        );
        extract(
            r#"{"msg":{"type":"agent_message","message":"world"}}"#,
            &mut text,
            &mut err,
        );
        extract(
            r#"{"msg":{"type":"error","message":"boom"}}"#,
            &mut text,
            &mut err,
        );
        assert_eq!(text, "helloworld");
        assert_eq!(err, "boom");
    }

    #[test]
    fn ignores_non_json_lines() {
        let mut text = String::new();
        let mut err = String::new();
        extract("not json", &mut text, &mut err);
        assert!(text.is_empty() && err.is_empty());
    }
}
