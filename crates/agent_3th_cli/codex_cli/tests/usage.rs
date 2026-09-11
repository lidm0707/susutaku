use codex_cli::{parse_usage, FetchError, UsageStatus};

const FIXTURE_WRAPPED: &str = r#"{
  "rate_limit": {
    "primary": {"used_percent": 12.5, "window_minutes": 300, "resets_at": 1757600000},
    "secondary": {"used_percent": 3.0, "window_minutes": 10080, "resets_at": 1758200000}
  }
}"#;

const FIXTURE_FLAT: &str = r#"{
  "primary": {"used_percent": 40, "reset_after_seconds": 3600.5},
  "secondary": {"used_percent": 7.25}
}"#;

#[test]
fn parse_wrapped_fixture() {
    let usage = parse_usage(FIXTURE_WRAPPED).expect("parse");
    let primary = usage.primary.expect("primary");
    let secondary = usage.secondary.expect("secondary");
    assert_eq!(primary.used_percent, 12.5);
    assert_eq!(primary.window_minutes, Some(300));
    assert_eq!(primary.resets_at, Some(1_757_600_000));
    assert_eq!(secondary.used_percent, 3.0);
}

#[test]
fn parse_flat_fixture_derives_reset_from_seconds() {
    let usage = parse_usage(FIXTURE_FLAT).expect("parse");
    let primary = usage.primary.expect("primary");
    assert_eq!(primary.used_percent, 40.0);
    let reset = primary.resets_at.expect("reset derived from seconds");
    assert!(reset > codex_reset_floor());
}

fn codex_reset_floor() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[test]
fn parse_rejects_garbage() {
    assert!(parse_usage("not json").is_err());
    assert!(parse_usage("{}").is_ok(), "empty limits are valid: no windows");
}

#[test]
fn fetch_error_display_is_stable() {
    let e = FetchError::Other("boom".to_string());
    assert_eq!(format!("{e:?}"), r#"Other("boom")"#);
}

#[test]
fn usage_status_matches_logged_out_shape() {
    let status = UsageStatus::NotLoggedIn;
    assert!(!matches!(status, UsageStatus::Ok(_)));
}
