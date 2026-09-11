use zai_api::quota::{Limit, Quota, parse_quota};

const SAMPLE: &str = r#"{"code":200,"msg":"ok","data":{"limits":[{"type":"TIME_LIMIT","unit":5,"number":1,"percentage":0,"nextResetTime":1789001066999},{"type":"TOKENS_LIMIT","percentage":19,"nextResetTime":1787248407405}],"level":"lite"},"success":true}"#;

#[test]
fn parses_real_quota_payload() {
    let value: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
    let quota = parse_quota(&value).unwrap();
    assert_eq!(quota.limits.len(), 2);

    let time = &quota.limits[0];
    assert_eq!(time.limit_type, "TIME_LIMIT");
    assert_eq!(time.percentage, 0.0);
    assert_eq!(time.next_reset_time, Some(1_789_001_066_999));

    let tokens = &quota.limits[1];
    assert_eq!(tokens.limit_type, "TOKENS_LIMIT");
    assert_eq!(tokens.percentage, 19.0);
    assert_eq!(tokens.next_reset_time, Some(1_787_248_407_405));
}

#[test]
fn next_reset_time_is_optional() {
    let value: serde_json::Value =
        serde_json::from_str(r#"{"data":{"limits":[{"type":"TOKENS_LIMIT","percentage":5}]}}"#)
            .unwrap();
    let quota = parse_quota(&value).unwrap();
    assert_eq!(
        quota.limits,
        vec![Limit {
            limit_type: "TOKENS_LIMIT".into(),
            percentage: 5.0,
            next_reset_time: None,
        }]
    );
}

#[test]
fn missing_limits_is_an_error() {
    let value: serde_json::Value = serde_json::from_str(r#"{"code":401}"#).unwrap();
    assert!(parse_quota(&value).is_err());
}

#[test]
fn quota_struct_holds_limits() {
    let quota = Quota { limits: Vec::new() };
    assert!(quota.limits.is_empty());
}
