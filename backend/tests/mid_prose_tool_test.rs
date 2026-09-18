//! Tool markers embedded mid-sentence: models often write
//! "... sent. TOOL: SHELL ls" inside prose instead of on its own line, and
//! the parser must still find and execute the call.

use backend::domain::service::tool_call::ToolCall;

#[test]
fn marker_after_prose_on_the_same_line_parses() {
    let call = ToolCall::parse("request sent. TOOL: SHELL pwd && ls").expect("shell call");
    assert_eq!(call, ToolCall::Shell("pwd && ls".to_owned()));
}

#[test]
fn marker_case_is_irrelevant() {
    let call = ToolCall::parse("done — tool: GIT STATUS").expect("git call");
    assert!(matches!(call, ToolCall::Git { .. }));
}

#[test]
fn first_parsable_candidate_wins() {
    // An unknown kind in prose must not block a real call later in the reply.
    let reply = "a tool: would help. TOOL: SHELL echo hi";
    let call = ToolCall::parse(reply).expect("shell call");
    assert_eq!(call, ToolCall::Shell("echo hi".to_owned()));
}

#[test]
fn argument_stops_at_the_newline() {
    let reply = "TOOL: SHELL echo one\nplain text after";
    let call = ToolCall::parse(reply).expect("shell call");
    assert_eq!(call, ToolCall::Shell("echo one".to_owned()));
}

#[test]
fn prose_mentioning_a_tool_marker_is_not_parsed() {
    // "tool:" as part of running text with no known kind after it yields no
    // call — the reply is treated as plain text.
    assert!(ToolCall::parse("the tool: is what we call it").is_none());
}
