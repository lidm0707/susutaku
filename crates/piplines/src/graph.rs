//! Serializable pipeline graph: node/link spec with validation.

use serde::{Deserialize, Serialize};

pub const MAX_NODES: usize = 64;

pub const STAGE_INGEST: &str = "ingest";
pub const STAGE_PARSE: &str = "parse";
pub const STAGE_TRANSFORM: &str = "transform";
pub const STAGE_MODEL_INFER: &str = "model_infer";
pub const STAGE_RENDER: &str = "render";

/// Default node kinds: built-in connectors.
pub const STAGE_FETCH: &str = "fetch";
pub const STAGE_SEARCH: &str = "search";
pub const STAGE_REF_IMAGE: &str = "ref_image";
pub const STAGE_OUTPUT_RESOURCE: &str = "output_resource";
pub const STAGE_AGENT: &str = "agent";

pub const STAGE_NAMES: [&str; 10] = [
    STAGE_INGEST,
    STAGE_PARSE,
    STAGE_TRANSFORM,
    STAGE_MODEL_INFER,
    STAGE_RENDER,
    STAGE_FETCH,
    STAGE_SEARCH,
    STAGE_REF_IMAGE,
    STAGE_OUTPUT_RESOURCE,
    STAGE_AGENT,
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeDef {
    pub id: String,
    pub stage: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PipelineSpec {
    pub nodes: Vec<NodeDef>,
    pub links: Vec<Link>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum GraphError {
    #[error("pipeline spec has no nodes")]
    Empty,
    #[error("pipeline spec exceeds {MAX_NODES} nodes")]
    TooManyNodes,
    #[error("duplicate node id: {0}")]
    DuplicateNode(String),
    #[error("bad link: {0}")]
    BadLink(String),
    #[error("pipeline graph contains a cycle")]
    Cycle,
}

impl PipelineSpec {
    pub fn validate(&self) -> Result<(), GraphError> {
        self.validate_nodes()?;
        self.validate_links()?;
        self.validate_acyclic()
    }

    /// Like validate, but permits an empty node set: the UI persists a draft
    /// pipeline with no nodes before the user places any on the canvas.
    pub fn validate_draft(&self) -> Result<(), GraphError> {
        if self.nodes.is_empty() {
            return Ok(());
        }
        self.validate()
    }

    fn validate_nodes(&self) -> Result<(), GraphError> {
        if self.nodes.is_empty() {
            return Err(GraphError::Empty);
        }
        if self.nodes.len() > MAX_NODES {
            return Err(GraphError::TooManyNodes);
        }
        for (i, node) in self.nodes.iter().enumerate() {
            if !STAGE_NAMES.contains(&node.stage.as_str()) {
                return Err(GraphError::BadLink(format!(
                    "unknown stage: {}",
                    node.stage
                )));
            }
            if self.nodes[..i].iter().any(|n| n.id == node.id) {
                return Err(GraphError::DuplicateNode(node.id.clone()));
            }
        }
        Ok(())
    }

    fn validate_links(&self) -> Result<(), GraphError> {
        for link in &self.links {
            if !self.nodes.iter().any(|n| n.id == link.from) {
                return Err(GraphError::BadLink(format!(
                    "unknown source: {}",
                    link.from
                )));
            }
            if !self.nodes.iter().any(|n| n.id == link.to) {
                return Err(GraphError::BadLink(format!("unknown target: {}", link.to)));
            }
        }
        Ok(())
    }

    fn validate_acyclic(&self) -> Result<(), GraphError> {
        let mut indegree = vec![0usize; self.nodes.len()];
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); self.nodes.len()];
        for link in &self.links {
            let from = self.index_of(&link.from).expect("validated");
            let to = self.index_of(&link.to).expect("validated");
            adj[from].push(to);
            indegree[to] += 1;
        }
        let mut queue: Vec<usize> = (0..self.nodes.len())
            .filter(|&i| indegree[i] == 0)
            .collect();
        let mut visited = 0;
        let mut head = 0;
        while head < queue.len() {
            let node = queue[head];
            head += 1;
            visited += 1;
            for &next in &adj[node] {
                indegree[next] -= 1;
                if indegree[next] == 0 {
                    queue.push(next);
                }
            }
        }
        if visited == self.nodes.len() {
            Ok(())
        } else {
            Err(GraphError::Cycle)
        }
    }

    fn index_of(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }
}
