use piplines::payload::Payload;
use piplines::stage::{Stage, StageId};
use piplines::{Pipeline, PipelineError};

struct Uppercase;

impl Stage for Uppercase {
    fn id(&self) -> StageId {
        StageId::Transform
    }
    fn apply(&self, payload: Payload) -> Result<Payload, String> {
        let text = payload.as_str().ok_or("not text")?;
        Ok(Payload::text(text.to_uppercase()))
    }
}

struct Fail;

impl Stage for Fail {
    fn id(&self) -> StageId {
        StageId::Custom("fail")
    }
    fn apply(&self, _payload: Payload) -> Result<Payload, String> {
        Err("boom".into())
    }
}

#[test]
fn runs_stages_in_order() {
    let mut p = Pipeline::new();
    p.attach(Box::new(Uppercase)).unwrap();
    p.attach(Box::new(Uppercase)).unwrap();
    let out = p.run(Payload::text("abc")).unwrap();
    assert_eq!(out.as_str(), Some("ABC"));
}

#[test]
fn empty_pipeline_errors() {
    let mut p = Pipeline::new();
    assert!(matches!(p.run(Payload::text("x")), Err(PipelineError::Empty)));
}

#[test]
fn stage_failure_reports_id() {
    let mut p = Pipeline::new();
    p.attach(Box::new(Uppercase)).unwrap();
    p.attach(Box::new(Fail)).unwrap();
    match p.run(Payload::text("x")) {
        Err(PipelineError::StageFailed(StageId::Custom("fail"), msg)) => assert_eq!(msg, "boom"),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn payload_json_roundtrip() {
    let p = Payload::json(serde_json::json!({ "a": 1 }));
    assert_eq!(p.as_json().unwrap()["a"], 1);
}
