use backend::domain::{GitOp, ToolCall, ToolKind, ToolSet};

fn git(op: GitOp) -> ToolCall {
    ToolCall::Git { op, agent: None }
}

#[test]
fn parses_git_tool_lines() {
    assert_eq!(
        ToolCall::parse("TOOL: GIT CLONE https://github.com/lidm0707/susutaku"),
        Some(git(GitOp::Clone {
            url: Some("https://github.com/lidm0707/susutaku".into()),
            token: None,
        }))
    );
    assert_eq!(
        ToolCall::parse("TOOL: GIT STATUS"),
        Some(git(GitOp::Status))
    );
    assert_eq!(ToolCall::parse("TOOL: GIT DIFF"), Some(git(GitOp::Diff)));
    assert_eq!(ToolCall::parse("TOOL: GIT"), None);
    assert_eq!(
        ToolCall::parse("TOOL: GIT CLONE"),
        Some(git(GitOp::Clone {
            url: None,
            token: None,
        }))
    );
}

#[test]
fn parses_agent_in_git_tool_lines() {
    assert_eq!(
        ToolCall::parse("TOOL: GIT COMMIT fix the bug @worker"),
        Some(ToolCall::Git {
            op: GitOp::Commit {
                message: "fix the bug".into(),
            },
            agent: Some("worker".into()),
        })
    );
    assert_eq!(
        ToolCall::parse("TOOL: GIT BRANCH feat/x @worker"),
        Some(ToolCall::Git {
            op: GitOp::Branch {
                name: "feat/x".into(),
            },
            agent: Some("worker".into()),
        })
    );
    // host ops never strip an @token (a url may legitimately contain one)
    assert_eq!(
        ToolCall::parse("TOOL: GIT CLONE https://x/@repo"),
        Some(git(GitOp::Clone {
            url: Some("https://x/@repo".into()),
            token: None,
        }))
    );
}

#[test]
fn parses_git_xml_invoke() {
    assert_eq!(
        ToolCall::parse(
            r#"<invoke name="git"><parameter name="op">clone</parameter><parameter name="url">https://example.com/r.git</parameter></invoke>"#
        ),
        Some(ToolCall::Git {
            op: GitOp::Clone {
                url: Some("https://example.com/r.git".into()),
                token: None,
            },
            agent: None,
        })
    );
    assert_eq!(
        ToolCall::parse(
            r#"<invoke name="git"><parameter name="op">commit</parameter><parameter name="message">m</parameter><parameter name="agent">worker</parameter></invoke>"#
        ),
        Some(ToolCall::Git {
            op: GitOp::Commit {
                message: "m".into(),
            },
            agent: Some("worker".into()),
        })
    );
}

#[test]
fn git_tool_permission() {
    assert!(ToolSet::all().allows(ToolKind::Git));
    let set = ToolSet::from_names(&["git".to_string()]).unwrap();
    assert!(set.allows(ToolKind::Git));
    assert!(!set.allows(ToolKind::Shell));
    assert!(!ToolSet::default().allows(ToolKind::Git));
}
