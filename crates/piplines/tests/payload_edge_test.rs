use piplines::payload::{Payload, PayloadKind, SOURCE_KEY, TEXT_KEY};

#[test]
fn payload_as_json_rejects_non_json_kind() {
    let mut payload = Payload::text("not json");
    payload.kind = PayloadKind::Binary;
    assert_eq!(payload.as_json(), None);
}

#[test]
fn payload_as_json_rejects_malformed_data() {
    let mut payload = Payload::json(serde_json::json!({ "a": 1 }));
    payload.data = b"{broken".to_vec();
    assert_eq!(payload.as_json(), None);
}

#[test]
fn payload_binary_kind() {
    let mut payload = Payload::text("raw");
    payload.kind = PayloadKind::Binary;
    assert_eq!(payload.kind, PayloadKind::Binary);
    assert_eq!(payload.as_json(), None);
}

#[test]
fn payload_meta_set_get() {
    let mut payload = Payload::text("x");
    payload.set_meta(TEXT_KEY, "t");
    payload.set_meta(SOURCE_KEY, "s");
    assert_eq!(payload.get_meta(TEXT_KEY), Some("t"));
    assert_eq!(payload.get_meta(SOURCE_KEY), Some("s"));
    assert_eq!(payload.get_meta("missing"), None);
    payload.set_meta(TEXT_KEY, "t2");
    assert_eq!(payload.get_meta(TEXT_KEY), Some("t2"));
}

#[test]
fn payload_clone_derives() {
    let mut payload = Payload::text("x");
    payload.set_meta(TEXT_KEY, "t");
    let clone = payload.clone();
    assert_eq!(clone.data, payload.data);
    assert_eq!(clone.kind, payload.kind);
    assert_eq!(clone.meta, payload.meta);
    let text = format!("{payload:?}");
    assert!(text.contains("Text"));
    assert!(text.contains(TEXT_KEY));
}

#[test]
fn payload_json_scalars() {
    assert_eq!(
        Payload::json(serde_json::json!(42)).as_json(),
        Some(serde_json::json!(42))
    );
    assert_eq!(
        Payload::json(serde_json::json!("s")).as_json(),
        Some(serde_json::json!("s"))
    );
    assert_eq!(
        Payload::json(serde_json::json!(null)).as_json(),
        Some(serde_json::json!(null))
    );
}

#[test]
fn payload_kind_roundtrip() {
    for kind in [PayloadKind::Text, PayloadKind::Json, PayloadKind::Binary] {
        let json = serde_json::to_string(&kind).expect("serialize");
        let back: PayloadKind = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, kind);
    }
}
