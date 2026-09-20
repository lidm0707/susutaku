#![allow(clippy::unwrap_used)] // tests: unwrap is the assertion tool

use backend::domain::{ToolCall, ToolKind, ToolSet};

#[test]
fn parses_math_tool_line() {
    assert_eq!(
        ToolCall::parse("TOOL: MATH pythag 3 4"),
        Some(ToolCall::Math("pythag 3 4".into()))
    );
    assert_eq!(
        ToolCall::parse("TOOL: GEOMATH polygon 0,0 4,0 4,4"),
        Some(ToolCall::Math("polygon 0,0 4,0 4,4".into()))
    );
    assert_eq!(ToolCall::parse("TOOL: MATH"), None);
}

#[test]
fn parses_math_xml_invoke() {
    assert_eq!(
        ToolCall::parse(
            r#"<invoke name="math"><parameter name="expr">heron 3 4 5</parameter></invoke>"#
        ),
        Some(ToolCall::Math("heron 3 4 5".into()))
    );
}

#[test]
fn math_tool_permission() {
    assert!(ToolSet::all().allows(ToolKind::Math));
    let set = ToolSet::from_names(&["math".to_string()]).unwrap();
    assert!(set.allows(ToolKind::Math));
    assert!(!set.allows(ToolKind::Shell));
    assert!(ToolSet::from_names(&["calc".to_string()]).is_err());
}
