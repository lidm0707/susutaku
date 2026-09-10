use codex_cli::auth::{account_id_from_id_token, authorize_url};

const FAKE_ID_TOKEN: &str = concat!(
    "eyJhbGciOiJFUzI1NiJ9.",
    "eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGguY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjLTEyMyJ9.",
    "sig"
);

const AUD_PAYLOAD_B64: &str = "eyJhdWQiOiJhcHBfdGVzdCJ9";

#[test]
fn extracts_account_id() {
    assert_eq!(
        account_id_from_id_token(FAKE_ID_TOKEN),
        Some("acc-123".into())
    );
}

#[test]
fn authorize_url_contains_pkce() {
    const TEST_CLIENT_ID: &str = "app_test";
    let dir = std::env::temp_dir().join("susutaku-codex-auth-url-test");
    std::fs::create_dir_all(&dir).unwrap();
    let with_aud = FAKE_ID_TOKEN.replace(
        "eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGguY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjLTEyMyJ9",
        AUD_PAYLOAD_B64,
    );
    let doc = serde_json::json!({"tokens": {"id_token": with_aud}});
    std::fs::write(
        codex_cli::auth::auth_path(&dir),
        serde_json::to_vec(&doc).unwrap(),
    )
    .unwrap();
    // SAFETY: single-threaded test, no other reads of this env var.
    unsafe { std::env::set_var("CODEX_HOME", &dir) };
    let url =
        authorize_url("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk").expect("client id from auth");
    assert!(url.contains("code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
    assert!(url.contains(TEST_CLIENT_ID));
}

#[test]
fn client_id_from_aud_claim() {
    use codex_cli::auth::{audience_from_id_token, auth_path, client_id_from_codex_home};

    assert_eq!(audience_from_id_token(FAKE_ID_TOKEN), None);
    let with_aud = FAKE_ID_TOKEN.replace(
        "eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGguY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjLTEyMyJ9",
        "eyJhdWQiOiJhcHBfdGVzdCJ9",
    );
    assert_eq!(
        audience_from_id_token(&with_aud),
        Some("app_test".to_owned())
    );

    let dir = std::env::temp_dir().join("susutaku-codex-auth-test");
    std::fs::create_dir_all(&dir).unwrap();
    let doc = serde_json::json!({"tokens": {"id_token": with_aud}});
    std::fs::write(auth_path(&dir), serde_json::to_vec(&doc).unwrap()).unwrap();
    assert_eq!(client_id_from_codex_home(&dir), Some("app_test".to_owned()));
}
