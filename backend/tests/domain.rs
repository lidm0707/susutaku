use backend::domain::ToolCall;

#[test]
fn parses_shell_tool_call() {
    let reply = "</think>\nTOOL: SHELL ls -la";
    assert_eq!(
        ToolCall::parse(reply),
        Some(ToolCall::Shell("ls -la".into()))
    );
}

#[test]
fn parses_coding_tool_call() {
    let reply = concat!(
        "</think>\n",
        "<invoke name=\"coding\">",
        "<parameter name=\"path\">src/main.rs</parameter>",
        "<parameter name=\"code\">fn main() {}\n// line two</parameter>",
        "</invoke>"
    );
    assert_eq!(
        ToolCall::parse(reply),
        Some(ToolCall::Coding {
            path: "src/main.rs".into(),
            code: "fn main() {}\n// line two".into(),
        })
    );
}

#[test]
fn coding_tool_call_without_path_is_ignored() {
    let reply = concat!(
        "<invoke name=\"coding\">",
        "<parameter name=\"code\">x</parameter>",
        "</invoke>"
    );
    assert_eq!(ToolCall::parse(reply), None);
}
