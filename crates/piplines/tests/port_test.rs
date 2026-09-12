//! Integration tests: per-stage port kinds + link port validation.

use piplines::graph::{
    GraphError, Link, NodeDef, PipelineSpec, STAGE_AGENT, STAGE_FETCH, STAGE_INGEST,
    STAGE_OUTPUT_RESOURCE, STAGE_PARSE, STAGE_REF_IMAGE, STAGE_TRANSFORM,
};
use piplines::port::{PortKind, compatible, ports, schema_text};

const ID_A: &str = "a";
const ID_B: &str = "b";
const SCHEMA_REQUIRED_MARK: &str = "agent (required";
const SCHEMA_NO_UNWIRED: &str = "model_infer";

fn node(id: &str, stage: &str) -> NodeDef {
    node_with_params(id, stage, serde_json::Value::Null)
}

fn node_with_params(id: &str, stage: &str, params: serde_json::Value) -> NodeDef {
    NodeDef {
        id: id.into(),
        stage: stage.into(),
        params,
        x: None,
        y: None,
    }
}

fn spec(nodes: Vec<NodeDef>, links: Vec<Link>) -> PipelineSpec {
    PipelineSpec { nodes, links }
}

#[test]
fn schema_text_lists_wired_stages_with_required_params() {
    let text = schema_text();
    for stage in piplines::port::WIRED_STAGES {
        assert!(text.contains(stage), "missing stage {stage} in:\n{text}");
    }
    assert!(text.contains(SCHEMA_REQUIRED_MARK));
    assert!(!text.contains(SCHEMA_NO_UNWIRED));
}

#[test]
fn known_stage_ports() {
    assert_eq!(ports(STAGE_PARSE), (PortKind::Text, PortKind::Json));
    assert_eq!(ports(STAGE_TRANSFORM), (PortKind::Text, PortKind::Text));
    assert_eq!(ports(STAGE_FETCH).1, PortKind::Text);
    assert_eq!(ports(STAGE_REF_IMAGE).1, PortKind::Image);
    assert_eq!(ports(STAGE_INGEST), (PortKind::Any, PortKind::Any));
}

#[test]
fn compat_matrix() {
    assert!(compatible(PortKind::Any, PortKind::Text));
    assert!(compatible(PortKind::Text, PortKind::Any));
    assert!(compatible(PortKind::Text, PortKind::Text));
    assert!(!compatible(PortKind::Json, PortKind::Text));
    assert!(!compatible(PortKind::Image, PortKind::Text));
    assert!(compatible(PortKind::Image, PortKind::Image));
}

#[test]
fn text_to_text_link_ok() {
    let s = spec(
        vec![
            node_with_params(
                ID_A,
                STAGE_FETCH,
                serde_json::json!({ "url": "https://example.com" }),
            ),
            node_with_params(ID_B, STAGE_TRANSFORM, serde_json::json!({ "op": "trim" })),
        ],
        vec![Link {
            from: ID_A.into(),
            to: ID_B.into(),
        }],
    );
    s.validate().expect("text -> text is valid");
}

#[test]
fn any_feeds_all_and_all_feed_any() {
    let s = spec(
        vec![
            node(ID_A, STAGE_INGEST),
            node_with_params(
                ID_B,
                STAGE_REF_IMAGE,
                serde_json::json!({ "path": "img.png" }),
            ),
            node_with_params("c", STAGE_AGENT, serde_json::json!({ "agent": "qwen" })),
        ],
        vec![
            Link {
                from: ID_A.into(),
                to: ID_B.into(),
            },
            Link {
                from: ID_B.into(),
                to: "c".into(),
            },
        ],
    );
    s.validate().expect("any ports accept everything");
}

#[test]
fn json_into_text_fails() {
    let s = spec(
        vec![
            node(ID_A, STAGE_PARSE),
            node_with_params(ID_B, STAGE_TRANSFORM, serde_json::json!({ "op": "trim" })),
        ],
        vec![Link {
            from: ID_A.into(),
            to: ID_B.into(),
        }],
    );
    assert!(matches!(s.validate(), Err(GraphError::PortMismatch { .. })));
}

#[test]
fn unknown_link_still_rejected() {
    let s = spec(
        vec![node(ID_A, STAGE_OUTPUT_RESOURCE)],
        vec![Link {
            from: ID_A.into(),
            to: "ghost".into(),
        }],
    );
    assert!(matches!(s.validate(), Err(GraphError::BadLink(_))));
}

#[test]
fn schema_covers_all_stages() {
    let v = piplines::port::schema();
    for stage in piplines::graph::STAGE_NAMES {
        let entry = v.get(stage).expect("stage in schema");
        assert!(entry.get("input").is_some());
        assert!(entry.get("output").is_some());
    }
}

#[test]
fn schema_marks_wired_stages_and_params() {
    let v = piplines::port::schema();
    for stage in piplines::port::WIRED_STAGES {
        let entry = v.get(stage).expect("wired stage in schema");
        assert_eq!(entry.get("wired"), Some(&serde_json::json!(true)));
    }
    let parse = v
        .get(piplines::graph::STAGE_PARSE)
        .expect("parse in schema");
    assert_eq!(parse.get("wired"), Some(&serde_json::json!(false)));
    assert!(parse.get("doc").is_some());

    let search = v
        .get(piplines::graph::STAGE_SEARCH)
        .expect("search in schema");
    assert_eq!(search.get("wired"), Some(&serde_json::json!(true)));
    let search_params = search.get("params").expect("search params");
    assert!(search_params.to_string().contains("query"));

    let fetch = v
        .get(piplines::graph::STAGE_FETCH)
        .expect("fetch in schema");
    let params = fetch
        .get("params")
        .expect("fetch params")
        .as_array()
        .expect("array");
    assert!(
        params
            .iter()
            .any(|p| p.get("key") == Some(&serde_json::json!("url"))
                && p.get("required") == Some(&serde_json::json!(true)))
    );
}

#[test]
fn node_def_position_roundtrip() {
    let spec: PipelineSpec = serde_json::from_str(
        "{\"nodes\":[{\"id\":\"a\",\"stage\":\"fetch\",\"params\":{},\"x\":10.0,\"y\":20.0}],\"links\":[]}",
    )
    .expect("spec with position parses");
    assert_eq!(spec.nodes[0].x, Some(10.0));
    assert_eq!(spec.nodes[0].y, Some(20.0));

    let without: PipelineSpec = serde_json::from_str(
        "{\"nodes\":[{\"id\":\"a\",\"stage\":\"fetch\",\"params\":{}}],\"links\":[]}",
    )
    .expect("spec without position parses");
    assert_eq!(without.nodes[0].x, None);
    let json = serde_json::to_string(&without).expect("serialize");
    assert!(!json.contains("\"x\""), "none position omitted: {json}");
}
