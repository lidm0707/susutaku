use piplines::graph::{
    GraphError, Link, MAX_NODES, NodeDef, PipelineSpec, STAGE_INGEST, STAGE_PARSE,
};

const ID_A: &str = "a";
const ID_B: &str = "b";

fn node(id: &str, stage: &str) -> NodeDef {
    NodeDef {
        id: id.into(),
        stage: stage.into(),
        params: serde_json::Value::Null,
        x: None,
        y: None,
    }
}

#[test]
fn validate_draft_allows_empty_nodes() {
    let spec = PipelineSpec {
        nodes: vec![],
        links: vec![],
    };
    assert_eq!(spec.validate_draft(), Ok(()));
}

#[test]
fn validate_draft_still_validates_nonempty() {
    let mut spec = PipelineSpec {
        nodes: vec![node(ID_A, STAGE_INGEST)],
        links: vec![],
    };
    assert_eq!(spec.validate_draft(), Ok(()));

    spec.nodes.push(node(ID_A, STAGE_PARSE));
    assert_eq!(
        spec.validate_draft(),
        Err(GraphError::DuplicateNode(ID_A.into()))
    );
}

#[test]
fn too_many_nodes_boundary_is_max_plus_one() {
    let ok = PipelineSpec {
        nodes: (0..MAX_NODES)
            .map(|i| node(&format!("n{i}"), STAGE_PARSE))
            .collect(),
        links: vec![],
    };
    assert_eq!(ok.validate(), Ok(()));

    let bad = PipelineSpec {
        nodes: (0..=MAX_NODES)
            .map(|i| node(&format!("n{i}"), STAGE_PARSE))
            .collect(),
        links: vec![],
    };
    assert_eq!(bad.validate(), Err(GraphError::TooManyNodes));
}

#[test]
fn params_deserialize_defaults_to_null() {
    let json = r#"{"id":"a","stage":"ingest"}"#;
    let node: NodeDef = serde_json::from_str(json).expect("node");
    assert_eq!(node.params, serde_json::Value::Null);
    assert_eq!(
        node,
        NodeDef {
            id: ID_A.into(),
            stage: STAGE_INGEST.into(),
            params: serde_json::Value::Null,
            x: None,
            y: None,
        }
    );
}

#[test]
fn node_with_params_roundtrip() {
    let node = NodeDef {
        id: ID_A.into(),
        stage: STAGE_INGEST.into(),
        params: serde_json::json!({ "k": [1, 2] }),
        x: None,
        y: None,
    };
    let json = serde_json::to_string(&node).expect("serialize");
    let back: NodeDef = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, node);
}

#[test]
fn link_roundtrip() {
    let link = Link {
        from: ID_A.into(),
        to: ID_B.into(),
    };
    let json = serde_json::to_string(&link).expect("serialize");
    let back: Link = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, link);
}

#[test]
fn disconnected_nodes_are_acyclic() {
    let spec = PipelineSpec {
        nodes: vec![node(ID_A, STAGE_INGEST), node(ID_B, STAGE_PARSE)],
        links: vec![],
    };
    assert_eq!(spec.validate(), Ok(()));
}

#[test]
fn duplicate_link_passes_when_acyclic() {
    let spec = PipelineSpec {
        nodes: vec![node(ID_A, STAGE_INGEST), node(ID_B, STAGE_PARSE)],
        links: vec![
            Link {
                from: ID_A.into(),
                to: ID_B.into(),
            },
            Link {
                from: ID_A.into(),
                to: ID_B.into(),
            },
        ],
    };
    assert_eq!(spec.validate(), Ok(()));
}

#[test]
fn cycle_error_message() {
    let err = GraphError::Empty;
    assert_eq!(err.to_string(), "pipeline spec has no nodes");
    let err: GraphError = GraphError::TooManyNodes;
    assert_eq!(
        err.to_string(),
        format!("pipeline spec exceeds {MAX_NODES} nodes")
    );
    let err = GraphError::DuplicateNode(ID_A.into());
    assert_eq!(err.to_string(), "duplicate node id: a");
    let err = GraphError::BadLink("x".into());
    assert_eq!(err.to_string(), "bad link: x");
    let err = GraphError::Cycle;
    assert_eq!(err.to_string(), "pipeline graph contains a cycle");
}
