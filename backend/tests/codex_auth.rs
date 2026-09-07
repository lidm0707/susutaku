use backend::infra::codex_auth::code_from_request;

#[test]
fn extracts_code_from_callback_request() {
    let req = "GET /auth/callback?code=abc-123&state=x HTTP/1.1\r\nHost: localhost\r\n\r\n";
    assert_eq!(code_from_request(req), Some("abc-123".into()));
}

#[test]
fn rejects_request_without_code() {
    let req = "GET /auth/callback?error=cancelled HTTP/1.1\r\n\r\n";
    assert_eq!(code_from_request(req), None);
}
