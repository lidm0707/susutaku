use backend::domain::ToolCall;

#[test]
fn parses_shell_tool_call() {
    let reply = "</think>\nTOOL: SHELL ls -la";
    assert_eq!(
        ToolCall::parse(reply),
        Some(ToolCall::Shell("ls -la".into()))
    );
}
