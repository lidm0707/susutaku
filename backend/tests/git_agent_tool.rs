use backend::domain::{GitOp, ToolCall, ToolKind, ToolSet};

#[test]
fn parses_git_tool_lines() {
    assert_eq!(
        ToolCall::parse("TOOL: GIT CLONE https://github.com/lidm0707/susutaku"),
        Some(ToolCall::Git(GitOp::Clone {
            url: Some("https://github.com/lidm0707/susutaku".into()),
            token: None,
        }))
    );
    assert_eq!(
        ToolCall::parse("TOOL: GIT STATUS"),
        Some(ToolCall::Git(GitOp::Status))
    );
    assert_eq!(
        ToolCall::parse("TOOL: GIT DIFF"),
        Some(ToolCall::Git(GitOp::Diff))
    );
    assert_eq!(ToolCall::parse("TOOL: GIT"), None);
    assert_eq!(
        ToolCall::parse("TOOL: GIT CLONE"),
        Some(ToolCall::Git(GitOp::Clone {
            url: None,
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
        Some(ToolCall::Git(GitOp::Clone {
            url: Some("https://example.com/r.git".into()),
            token: None,
        }))
    );
}

#[test]
fn git_tool_permission() {
    assert!(ToolSet::all().allows(ToolKind::Git));
    let set = ToolSet::from_names(&["git".to_string()]).unwrap();
    assert!(set.allows(ToolKind::Git));
    assert!(!set.allows(ToolKind::Shell));
    assert!(ToolSet::default().allows(ToolKind::Git) == false);
}
