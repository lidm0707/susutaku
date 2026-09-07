//! Integration tests: agent-select node.

use piplines::Pipeline;
use piplines::agent::{AgentNode, META_AGENT, PARAM_AGENT};
use piplines::graph::{Link, NodeDef, PipelineSpec, STAGE_AGENT, STAGE_INGEST};
use piplines::payload::Payload;
use piplines::stage::{Stage, StageId};

const ID_IN: &str = "in";
const ID_PICK: &str = "pick";
const AGENT_NAME: &str = "coder";

fn agent_params(name: &str) -> serde_json::Value {
    serde_json::json!({ PARAM_AGENT: name })
}

#[test]
fn agent_stage_accepted_by_graph() {
    let spec = PipelineSpec {
        nodes: vec![
            NodeDef {
                id: ID_IN.into(),
                stage: STAGE_INGEST.into(),
                params: serde_json::Value::Null,
            },
            NodeDef {
                id: ID_PICK.into(),
                stage: STAGE_AGENT.into(),
                params: agent_params(AGENT_NAME),
            },
        ],
        links: vec![Link {
            from: ID_IN.into(),
            to: ID_PICK.into(),
        }],
    };
    spec.validate().expect("agent node valid");
}

#[test]
fn from_params_reads_agent_name() {
    let node = AgentNode::from_params(&agent_params(AGENT_NAME)).expect("params ok");
    assert_eq!(node.agent, AGENT_NAME);
}

#[test]
fn from_params_rejects_missing_or_empty() {
    assert!(AgentNode::from_params(&serde_json::Value::Null).is_err());
    assert!(AgentNode::from_params(&agent_params("")).is_err());
}

#[test]
fn agent_stage_stamps_meta() {
    let node = AgentNode::from_params(&agent_params(AGENT_NAME)).expect("params ok");
    assert_eq!(node.id(), StageId::Agent);
    let out = node.apply(Payload::text("hi")).expect("apply ok");
    assert_eq!(out.get_meta(META_AGENT), Some(AGENT_NAME));
}

#[test]
fn agent_stage_runs_in_pipeline() {
    let node = AgentNode::from_params(&agent_params(AGENT_NAME)).expect("params ok");
    let mut p = Pipeline::new();
    p.attach(Box::new(node)).expect("attach");
    let out = p.run(Payload::text("hi")).expect("run ok");
    assert_eq!(out.get_meta(META_AGENT), Some(AGENT_NAME));
}
