//! Integration tests: PipelineSpec validation (pure, no DB).

use piplines::graph::{
    GraphError, Link, MAX_NODES, NodeDef, PipelineSpec, STAGE_FETCH, STAGE_INGEST,
    STAGE_OUTPUT_RESOURCE, STAGE_PARSE, STAGE_REF_IMAGE, STAGE_RENDER, STAGE_SEARCH,
    STAGE_TRANSFORM,
};

const ID_A: &str = "a";
const ID_B: &str = "b";
const ID_C: &str = "c";

fn node(id: &str, stage: &str) -> NodeDef {
    NodeDef {
        id: id.into(),
        stage: stage.into(),
        params: serde_json::Value::Null,
        x: None,
        y: None,
    }
}

fn chain() -> PipelineSpec {
    PipelineSpec {
        nodes: vec![
            node(ID_A, STAGE_INGEST),
            node_with_params(ID_B, STAGE_TRANSFORM, serde_json::json!({ "op": "trim" })),
            node(ID_C, STAGE_RENDER),
        ],
        links: vec![
            Link {
                from: ID_A.into(),
                to: ID_B.into(),
            },
            Link {
                from: ID_B.into(),
                to: ID_C.into(),
            },
        ],
    }
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

#[test]
fn valid_spec_passes() {
    chain().validate().expect("valid chain");
}

#[test]
fn default_stages_pass() {
    let spec = PipelineSpec {
        nodes: vec![
            node_with_params(
                ID_A,
                STAGE_FETCH,
                serde_json::json!({ "url": "https://example.com" }),
            ),
            node_with_params(ID_B, STAGE_SEARCH, serde_json::json!({})),
            node_with_params(
                ID_C,
                STAGE_REF_IMAGE,
                serde_json::json!({ "path": "img.png" }),
            ),
        ],
        links: vec![
            Link {
                from: ID_A.into(),
                to: ID_B.into(),
            },
            Link {
                from: ID_B.into(),
                to: ID_C.into(),
            },
        ],
    };
    spec.validate().expect("default stages valid");
    PipelineSpec {
        nodes: vec![node_with_params(
            ID_A,
            STAGE_OUTPUT_RESOURCE,
            serde_json::json!({ "name": "out" }),
        )],
        links: vec![],
    }
    .validate()
    .expect("output_resource valid");
}

#[test]
fn missing_required_param_fails() {
    let spec = PipelineSpec {
        nodes: vec![node(ID_A, STAGE_FETCH), node(ID_B, STAGE_SEARCH)],
        links: vec![],
    };
    assert_eq!(
        spec.validate(),
        Err(GraphError::MissingParam {
            node: ID_A.into(),
            key: "url",
        })
    );
}

#[test]
fn empty_required_param_fails() {
    let spec = PipelineSpec {
        nodes: vec![node_with_params(
            ID_A,
            STAGE_FETCH,
            serde_json::json!({ "url": "" }),
        )],
        links: vec![],
    };
    assert!(matches!(
        spec.validate(),
        Err(GraphError::MissingParam { .. })
    ));
}

#[test]
fn empty_fails() {
    let spec = PipelineSpec {
        nodes: vec![],
        links: vec![],
    };
    assert_eq!(spec.validate(), Err(GraphError::Empty));
}

#[test]
fn too_many_nodes_fails() {
    let spec = PipelineSpec {
        nodes: (0..=MAX_NODES)
            .map(|i| node(&format!("n{i}"), STAGE_PARSE))
            .collect(),
        links: vec![],
    };
    assert_eq!(spec.validate(), Err(GraphError::TooManyNodes));
}

#[test]
fn duplicate_id_fails() {
    let mut spec = chain();
    spec.nodes.push(node(ID_A, STAGE_RENDER));
    assert_eq!(spec.validate(), Err(GraphError::DuplicateNode(ID_A.into())));
}

#[test]
fn bad_stage_fails() {
    let spec = PipelineSpec {
        nodes: vec![node(ID_A, "warp")],
        links: vec![],
    };
    assert!(matches!(spec.validate(), Err(GraphError::BadLink(_))));
}

#[test]
fn link_to_unknown_node_fails() {
    let mut spec = chain();
    spec.links.push(Link {
        from: ID_C.into(),
        to: "ghost".into(),
    });
    assert!(matches!(spec.validate(), Err(GraphError::BadLink(_))));

    let mut spec = chain();
    spec.links.push(Link {
        from: "ghost".into(),
        to: ID_A.into(),
    });
    assert!(matches!(spec.validate(), Err(GraphError::BadLink(_))));
}

#[test]
fn cycle_fails() {
    let mut spec = chain();
    spec.links.push(Link {
        from: ID_C.into(),
        to: ID_A.into(),
    });
    assert_eq!(spec.validate(), Err(GraphError::Cycle));
}

#[test]
fn self_link_fails() {
    let mut spec = chain();
    spec.links.push(Link {
        from: ID_A.into(),
        to: ID_A.into(),
    });
    assert_eq!(spec.validate(), Err(GraphError::Cycle));
}

#[test]
fn spec_serializes_roundtrip() {
    let json = serde_json::to_string(&chain()).expect("serialize");
    let back: PipelineSpec = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, chain());
}
