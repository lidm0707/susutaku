//! Env var settings: name/value validation, secret masking, persistence
//! round-trip through the settings doc.

use backend::infra::settings::env_vars::{self, EnvVar, SECRET_MASK};

#[test]
fn rejects_bad_names() {
    for name in [
        "",
        "1ABC",
        "HAS SPACE",
        "lower-case",
        "DASH-NAME",
        "dot.name",
    ] {
        assert!(
            env_vars::validate_name(name).is_err(),
            "must reject {name:?}"
        );
    }
    assert!(env_vars::validate_name(&"x".repeat(65)).is_err());
}

#[test]
fn accepts_env_style_names() {
    for name in ["PATH", "_PRIVATE", "ZAI_API_KEY", "A1", "x"] {
        assert!(
            env_vars::validate_name(name).is_ok(),
            "must accept {name:?}"
        );
    }
}

#[test]
fn rejects_bad_values() {
    assert!(env_vars::validate_value("ok\0nul").is_err());
    assert!(env_vars::validate_value(&"x".repeat(4097)).is_err());
    assert!(env_vars::validate_value("multi\nline is fine").is_ok());
}

#[test]
fn secrets_are_masked_in_display() {
    let secret = EnvVar {
        name: "ZAI_API_KEY".into(),
        value: "sk-super-secret".into(),
        secret: true,
    };
    let plain = EnvVar {
        name: "RUST_LOG".into(),
        value: "debug".into(),
        secret: false,
    };
    assert_eq!(secret.display_value(), SECRET_MASK);
    assert_eq!(plain.display_value(), "debug");
}

#[test]
fn doc_round_trip_preserves_vars() {
    let vars = vec![
        EnvVar {
            name: "RUST_LOG".into(),
            value: "debug".into(),
            secret: false,
        },
        EnvVar {
            name: "ZAI_API_KEY".into(),
            value: "sk-secret".into(),
            secret: true,
        },
    ];
    let mut doc = serde_json::json!({});
    env_vars::write(&mut doc, &vars);
    assert_eq!(env_vars::read(&doc), vars);
}

#[test]
fn read_missing_section_is_empty() {
    assert!(env_vars::read(&serde_json::json!({})).is_empty());
}
