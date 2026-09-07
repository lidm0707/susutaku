use codex_cli::auth::{account_id_from_id_token, authorize_url, CLIENT_ID};

const FAKE_ID_TOKEN: &str = concat!(
    "eyJhbGciOiJFUzI1NiJ9.",
    "eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGguY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjLTEyMyJ9.",
    "sig"
);

#[test]
fn extracts_account_id() {
    assert_eq!(
        account_id_from_id_token(FAKE_ID_TOKEN),
        Some("acc-123".into())
    );
}

#[test]
fn authorize_url_contains_pkce() {
    let url = authorize_url("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
    assert!(url.contains("code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
    assert!(url.contains(CLIENT_ID));
}
