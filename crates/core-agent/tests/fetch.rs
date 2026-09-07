use core_agent::toolcall::fetch::{TEXT_MAX, fetch, to_text, truncate_chars};

#[test]
fn reduces_html_to_text() {
    let html = r#"<html><head><style>p{color:red}</style><script>evil()</script></head>
        <body><h1>Title</h1><p>Hello&nbsp;&amp; welcome</p></body></html>"#;
    let text = to_text(html);
    assert_eq!(text, "Title Hello & welcome");
}

#[test]
fn rejects_non_http() {
    assert!(fetch("ftp://example.com").is_err());
}

#[test]
fn truncates_on_char_boundary() {
    let long = "é".repeat(TEXT_MAX + 10);
    assert_eq!(
        truncate_chars(&long, TEXT_MAX).chars().count(),
        TEXT_MAX + 1
    );
}
