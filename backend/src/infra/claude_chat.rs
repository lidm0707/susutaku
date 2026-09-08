//! Chat through the claude CLI (`claude -p --output-format stream-json`)
//! using the shared login.

use serde_json::Value;

use crate::infra::claude_auth::claude_home;

const RESULT_EVENT: &str = "result";
const ASSISTANT_EVENT: &str = "assistant";
const RESULT_TEXT: &str = "result";

/// Run one prompt through the claude CLI, return the final result text.
pub fn chat(prompt: &str) -> Result<String, String> {
    claude_cli::check_available().map_err(|e| format!("claude CLI unavailable: {e}"))?;
    let mut child = claude_cli::exec_json(prompt, &claude_home())
        .map_err(|e| format!("claude spawn failed: {e}"))?;
    let mut text = String::new();
    let mut err = String::new();
    claude_cli::drain_events(&mut child, |line| extract(line, &mut text, &mut err))
        .map_err(|e| format!("claude stream failed: {e}"))?;
    if text.is_empty() {
        return Err(if err.is_empty() {
            "claude returned no reply".to_string()
        } else {
            err
        });
    }
    Ok(text)
}

/// Prefer the final `result` payload; fall back to the last assistant text.
pub fn extract(line: &str, text: &mut String, err: &mut String) {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return;
    };
    match v.get("type").and_then(Value::as_str) {
        Some(RESULT_EVENT) => {
            if let Some(r) = v.get(RESULT_TEXT).and_then(Value::as_str) {
                text.clear();
                text.push_str(r);
            } else if let Some(e) = v.get("error").and_then(Value::as_str) {
                err.push_str(e);
            }
        }
        Some(ASSISTANT_EVENT) => {
            if let Some(t) = assistant_text(&v) {
                text.clear();
                text.push_str(&t);
            }
        }
        _ => {}
    }
}

/// Concatenate the text blocks of an assistant event message.
fn assistant_text(v: &Value) -> Option<String> {
    let content = v.get("message")?.get("content")?.as_array()?;
    let mut out = String::new();
    for block in content {
        if block.get("type").and_then(Value::as_str) == Some("text")
            && let Some(t) = block.get("text").and_then(Value::as_str)
        {
            out.push_str(t);
        }
    }
    (!out.is_empty()).then_some(out)
}
