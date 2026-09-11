use prompt_sys::{MAX_PROMPT_CHARS, PromptBuilder, PromptError, Role};

#[test]
fn parses_role_names() {
    assert_eq!(Role::parse("system"), Some(Role::System));
    assert_eq!(Role::parse("instructions"), Some(Role::Instruction));
    assert_eq!(Role::parse("context"), Some(Role::Context));
    assert_eq!(Role::parse("tools"), Some(Role::Tool));
    assert_eq!(Role::parse("nope"), None);
}

#[test]
fn renders_sections_in_order() {
    let p = PromptBuilder::new()
        .system("be lean")
        .instruction("use rust")
        .context("crate: prompt-sys")
        .build()
        .unwrap();
    let rendered = p.render();
    let sys = p.parts()[0].clone();
    assert_eq!(sys.role(), Role::System);
    assert_eq!(sys.body(), "be lean");
    assert!(rendered.starts_with("# System\nbe lean\n"));
    assert!(rendered.contains("# Instructions\nuse rust\n"));
    assert!(rendered.contains("# Context\ncrate: prompt-sys\n"));
}

#[test]
fn empty_body_allowed() {
    let p = PromptBuilder::new().context("").build().unwrap();
    assert_eq!(p.render(), "# Context\n\n");
    assert!(!p.is_empty());
}

#[test]
fn rejects_oversized_prompt() {
    let big = "x".repeat(MAX_PROMPT_CHARS);
    let err = PromptBuilder::new().system(big).build().unwrap_err();
    assert!(matches!(err, PromptError::TooLarge { .. }));
    assert!(err.to_string().contains("exceeds max"));
}

#[test]
fn builder_len_matches_render() {
    let b = PromptBuilder::new().system("abc").tool("de");
    let rendered = b.clone().build().unwrap().render();
    assert_eq!(b.len(), rendered.len());
}

#[test]
fn empty_builder_is_empty_and_builds() {
    let b = PromptBuilder::default();
    assert!(b.is_empty());
    assert_eq!(b.len(), 0);
    assert!(b.build().unwrap().render().is_empty());
}

#[test]
fn display_matches_render() {
    let p = PromptBuilder::new().system("hi").build().unwrap();
    assert_eq!(p.to_string(), p.render());
}
