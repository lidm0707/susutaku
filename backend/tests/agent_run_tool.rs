#![allow(clippy::unwrap_used)] // tests: unwrap is the assertion tool

use backend::domain::{ToolCall, ToolKind, ToolSet};

#[test]
fn parses_agent_run_tool_lines() {
    assert_eq!(
        ToolCall::parse("TOOL: AGENT_RUN zai cargo test --test multi_chat_test"),
        Some(ToolCall::AgentRun {
            agent: "zai".into(),
            cmd: "cargo test --test multi_chat_test".into(),
        })
    );
    // command is required
    assert_eq!(
        ToolCall::parse("TOOL: AGENT_RUN zai"),
        None
    );
    // agent name is required
    assert_eq!(ToolCall::parse("TOOL: AGENT_RUN"), None);
}

#[test]
fn parses_agent_run_xml_invoke() {
    assert_eq!(
        ToolCall::parse(
            r#"<invoke name="agent_run"><parameter name="agent">zai</parameter><parameter name="command">git status</parameter></invoke>"#
        ),
        Some(ToolCall::AgentRun {
            agent: "zai".into(),
            cmd: "git status".into(),
        })
    );
}

#[test]
fn agent_run_permission() {
    assert!(ToolSet::all().allows(ToolKind::Agent));
    let set = ToolSet::from_names(&["agent".to_string()]).unwrap();
    assert!(set.allows(ToolKind::Agent));
    assert!(!set.allows(ToolKind::Shell));
    assert!(!ToolSet::default().allows(ToolKind::Agent));
}
