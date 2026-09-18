//! Lenient tool-line recovery for card work runs: models that skip the
//! exact `TOOL: ` prefix but emit a bare verb line still get their tool
//! call executed instead of silently ending the run.

use backend::app::card_run::lenient_tool;
use backend::domain::service::tool_call::ToolCall;

#[test]
fn bare_shell_line_is_a_tool_call() {
    let call = lenient_tool("I'll start.\nSHELL echo hello").expect("shell call");
    assert!(matches!(call, ToolCall::Shell(_)));
}

#[test]
fn bare_git_line_keeps_its_arguments() {
    let call = lenient_tool("git status").expect("git call");
    assert!(matches!(call, ToolCall::Git { .. }));
}

#[test]
fn leading_whitespace_is_tolerated() {
    let call = lenient_tool("  AGENT_RUN ls -la").expect("agent_run call");
    assert!(matches!(call, ToolCall::Shell(_)));
}

#[test]
fn plain_prose_is_not_a_tool_call() {
    assert!(lenient_tool("I would add a settings page with env variables.").is_none());
    assert!(lenient_tool("").is_none());
}

#[test]
fn dollar_prompt_line_becomes_shell() {
    let call = lenient_tool("Let me check.\n$ ls -la").expect("shell call");
    assert!(matches!(call, ToolCall::Shell(cmd) if cmd == "ls -la"));
}

#[test]
fn fenced_shell_block_becomes_shell() {
    let reply = "Here:\n```sh\n$ git status\n```";
    let call = lenient_tool(reply).expect("shell call");
    assert!(matches!(call, ToolCall::Shell(cmd) if cmd == "git status"));
}

#[test]
fn non_shell_fence_is_ignored() {
    assert!(lenient_tool("```js\nconsole.log(1)\n```").is_none());
}

#[test]
fn existing_tool_prefix_is_left_to_the_strict_parser() {
    // lenient_tool only runs when the strict parse found nothing; a reply
    // that already carries TOOL: must not be rewritten.
    assert!(lenient_tool("TOOL: SHELL ls").is_none());
}
