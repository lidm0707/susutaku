use std::path::PathBuf;

use backend::infra::provider_quota::{codex, platform_from_codex_claims};
use chrono::Utc;
use codex_usage_rs::Row;
use base64::Engine;
use serde_json::{Value, json};

const JWT_HEADER: &str = "eyJhbGciOiJSUzI1NiJ9";
const JWT_SIGNATURE: &str = "sig";

fn b64(value: &Value) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(value).unwrap())
}

fn fake_jwt(payload: &Value) -> String {
    format!("{JWT_HEADER}.{}.{JWT_SIGNATURE}", b64(payload))
}

fn rate_limit_claims() -> Value {
    json!({
        "email": "user@example.com",
        "chatgpt_rate_limits": {
            "primary": { "used_percent": 42.5, "resets_at": 1_700_000_000 },
            "secondary": { "used_percent": 7.25, "resets_at": 1_700_000_100 }
        }
    })
}

#[test]
fn codex_claims_map_to_platform_quota() {
    let quota = platform_from_codex_claims(&rate_limit_claims()).expect("rate limits parsed");

    assert_eq!(quota.platform, "codex");
    assert!(quota.available);
    assert_eq!(quota.reason, None);
    assert_eq!(quota.tokens_used_pct, Some(7.25));
    assert_eq!(quota.window_used_pct, Some(42.5));
    assert_eq!(quota.window_reset_ms, Some(1_700_000_000_000));
}

#[test]
fn codex_claims_without_rate_limits_are_unavailable() {
    let claims = json!({ "email": "user@example.com" });

    assert!(platform_from_codex_claims(&claims).is_none());
}

#[test]
fn codex_without_auth_file_is_unavailable() {
    let home = std::env::temp_dir().join(format!("susutaku-quota-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();

    let quota = codex(&home, None);

    assert_eq!(quota.platform, "codex");
    assert!(!quota.available);
    assert!(quota.reason.is_some());
    assert_eq!(quota.tokens_used_pct, None);
    assert_eq!(quota.window_used_pct, None);
    assert_eq!(quota.window_reset_ms, None);
}

#[test]
fn codex_db_snapshot_wins_over_claims() {
    let home = std::env::temp_dir().join(format!("susutaku-quota-db-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let doc = json!({
        "tokens": { "access_token": fake_jwt(&rate_limit_claims()) }
    });
    std::fs::write(home.join("auth.json"), serde_json::to_vec(&doc).unwrap()).unwrap();

    let row = Row {
        captured_at: Utc::now(),
        plan_type: Some("free".into()),
        primary_used_percent: Some(17.0),
        primary_resets_at: Some(Utc::now()),
        secondary_used_percent: None,
        secondary_resets_at: None,
    };
    let quota = codex(&home, Some(row));

    assert!(quota.available);
    // primary window from the fresh DB snapshot, not the stale JWT claims
    assert_eq!(quota.tokens_used_pct, Some(17.0));
    assert_eq!(quota.window_used_pct, None);
    assert!(quota.window_reset_ms.is_some());
}

#[test]
fn codex_auth_file_with_jwt_exposes_rate_limits() {
    let home = std::env::temp_dir().join(format!("susutaku-quota-jwt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let doc = json!({
        "tokens": { "access_token": fake_jwt(&rate_limit_claims()) }
    });
    std::fs::write(home.join("auth.json"), serde_json::to_vec(&doc).unwrap()).unwrap();
    let home: PathBuf = home;

    let quota = codex(&home, None);

    assert!(quota.available);
    assert_eq!(quota.window_used_pct, Some(42.5));
    assert_eq!(quota.tokens_used_pct, Some(7.25));
    assert_eq!(quota.window_reset_ms, Some(1_700_000_000_000));
}

#[test]
fn malformed_jwt_payload_is_unavailable() {
    let home = std::env::temp_dir().join(format!("susutaku-quota-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let doc = json!({
        "tokens": { "access_token": "not.a.jwt" }
    });
    std::fs::write(home.join("auth.json"), serde_json::to_vec(&doc).unwrap()).unwrap();

    let quota = codex(&home, None);

    assert!(!quota.available);
    assert!(quota.reason.is_some());
}
