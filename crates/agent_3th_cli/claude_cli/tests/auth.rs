use claude_cli::auth::{authorize_url, to_tokens, unix_millis, CLIENT_ID};

#[test]
fn authorize_url_contains_pkce() {
    let url = authorize_url("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
    assert!(url.contains("code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
    assert!(url.contains(CLIENT_ID));
    assert!(url.contains("claude.ai/oauth/authorize"));
}

#[test]
fn to_tokens_roundtrip() {
    let resp = serde_json::json!({
        "access_token": "at", "refresh_token": "rt",
        "expires_in": 3600, "scope": "user:profile user:inference"
    });
    let tokens = to_tokens(resp).unwrap();
    assert_eq!(tokens.access_token, "at");
    assert_eq!(tokens.scopes, vec!["user:profile", "user:inference"]);
    assert!(tokens.expires_at_millis > unix_millis());
}
