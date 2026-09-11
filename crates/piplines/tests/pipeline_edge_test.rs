use piplines::payload::Payload;
use piplines::stage::{Stage, StageId};
use piplines::{MAX_STAGES, Pipeline, PipelineError};

struct Identity;

impl Stage for Identity {
    fn id(&self) -> StageId {
        StageId::Raw
    }
    fn apply(&self, payload: Payload) -> Result<Payload, String> {
        Ok(payload)
    }
}

#[test]
fn default_pipeline_is_empty() {
    let mut p: Pipeline = Pipeline::default();
    assert_eq!(p.stage_count(), 0);
    assert!(matches!(
        p.run(Payload::text("x")),
        Err(PipelineError::Empty)
    ));
}

#[test]
fn stage_count_tracks_attach() {
    let mut p = Pipeline::new();
    assert_eq!(p.stage_count(), 0);
    p.attach(Box::new(Identity)).unwrap();
    p.attach(Box::new(Identity)).unwrap();
    assert_eq!(p.stage_count(), 2);
}

#[test]
fn attach_fails_past_max_stages() {
    let mut p = Pipeline::new();
    for _ in 0..MAX_STAGES {
        p.attach(Box::new(Identity)).unwrap();
    }
    assert!(matches!(
        p.attach(Box::new(Identity)),
        Err(PipelineError::TooManyStages)
    ));
}

#[test]
fn run_drains_stages() {
    let mut p = Pipeline::new();
    p.attach(Box::new(Identity)).unwrap();
    let out = p.run(Payload::text("x")).unwrap();
    assert_eq!(out.as_str(), Some("x"));
    assert_eq!(p.stage_count(), 0);
    assert!(matches!(
        p.run(Payload::text("y")),
        Err(PipelineError::Empty)
    ));
}

#[test]
fn non_utf8_payload_as_str_none() {
    let mut payload = Payload::text("ok");
    payload.data = vec![0xff, 0xfe];
    assert_eq!(payload.as_str(), None);
}
