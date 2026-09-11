use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use codex_usage_rs::{latest_rate_limits, newest_rollout, RateLimits, UsageWindow};

const CODEX_HOME_ENV: &str = "CODEX_USAGE_TEST_HOME";

fn temp_home(tag: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!(
        "{CODEX_HOME_ENV}-{tag}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

const LINE_WITH_LIMITS: &str = r#"{"timestamp":"2026-09-08T10:30:00Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","primary":{"used_percent":15.0,"window_minutes":43200,"resets_at":1789580296},"secondary":null,"plan_type":"free"}}}"#;

const LINE_WITHOUT_LIMITS: &str = r#"{"type":"session_meta","payload":{"id":"abc"}}"#;

#[test]
fn parses_last_rate_limits_in_file() {
    let home = temp_home("parse");
    let day = home.join("sessions/2026/09/08");
    fs::create_dir_all(&day).unwrap();
    let file = day.join("rollout-test.jsonl");
    fs::write(
        &file,
        format!("{LINE_WITHOUT_LIMITS}\n{LINE_WITH_LIMITS}\n{LINE_WITHOUT_LIMITS}\n"),
    )
    .unwrap();

    let limits = latest_rate_limits(&file).expect("limits");
    assert_eq!(
        limits,
        RateLimits {
            primary: Some(UsageWindow {
                used_percent: 15.0,
                window_minutes: 43200,
                resets_at: 1789580296,
            }),
            secondary: None,
            plan_type: Some("free".into()),
        }
    );
    fs::remove_dir_all(&home).ok();
}

#[test]
fn newest_rollout_picks_latest_mtime() {
    let home = temp_home("newest");
    let old = home.join("sessions/2026/09/07");
    let new = home.join("sessions/2026/09/08");
    fs::create_dir_all(&old).unwrap();
    fs::create_dir_all(&new).unwrap();
    fs::write(old.join("rollout-a.jsonl"), LINE_WITHOUT_LIMITS).unwrap();
    fs::write(new.join("rollout-b.jsonl"), LINE_WITH_LIMITS).unwrap();

    let found = newest_rollout(&home).expect("rollout");
    assert_eq!(found.file_name().unwrap(), "rollout-b.jsonl");
    fs::remove_dir_all(&home).ok();
}

#[test]
fn missing_sessions_dir_is_error_not_panic() {
    let home = temp_home("missing");
    assert!(newest_rollout(&home).is_err());
    fs::remove_dir_all(&home).ok();
}

#[test]
fn file_without_rate_limits_is_error() {
    let home = temp_home("nolimits");
    let day = home.join("sessions/2026/09/08");
    fs::create_dir_all(&day).unwrap();
    let file = day.join("rollout-x.jsonl");
    fs::write(&file, LINE_WITHOUT_LIMITS).unwrap();
    assert!(matches!(
        latest_rate_limits(&file),
        Err(codex_usage_rs::rollout::RolloutError::NoRateLimits(_))
    ));
    fs::remove_dir_all(&home).ok();
}
