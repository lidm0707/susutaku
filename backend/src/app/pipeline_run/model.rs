//! Run-record types and note constants shared by the pipeline runner.

use piplines::payload::Payload;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const STAGE_NOTE_INGEST: &str = "seeded card title + description";
pub const STAGE_NOTE_PASSTHROUGH: &str = "payload passed through";
pub const STAGE_NOTE_RESOURCE: &str = "stored resource: ";
pub const TEXT_SEP: &str = "\n\n";
pub const INFER_MAX_TOKENS: usize = 1024;
pub const NOTE_NO_ENGINE: &str = "model_infer: no inference engine configured";
pub const NOTE_INFER_ERR: &str = "model_infer: engine rejected the job: ";
pub const NOTE_INFER_DROP: &str = "model_infer: engine dropped the job";
pub const NOTE_INFER_DONE: &str = "model replied";
pub const PROMPT_PERSONA: &str = "persona: ";
pub const PROMPT_INSTRUCTION: &str = "instruction: ";
pub const PROMPT_OUTPUT: &str = "output format: ";
pub const PROMPT_INPUT: &str = "\n\ninput:\n";
pub const NODE_PREPARE: &str = "pipeline";
pub const STAGE_PREPARE: &str = "prepare";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Ok,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunStage {
    pub node: String,
    pub stage: String,
    pub status: StageStatus,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunRecord {
    pub pipeline_id: i64,
    pub pipeline_name: String,
    pub status: StageStatus,
    pub stages: Vec<RunStage>,
    pub output: Option<String>,
    pub resources: Vec<String>,
    pub finished_at: String,
}

/// Result of a run attempt: record to persist + whether it should replace the agent name.
pub struct RunOutcome {
    pub record: RunRecord,
    pub agent_name: String,
    pub resources: Vec<(String, String)>,
}

/// Per-node execution result passed between node executors.
pub struct NodeResult {
    pub payload: Payload,
    pub note: String,
    /// (name, content) for output_resource nodes, persisted post-run.
    pub resource: Option<(String, String)>,
}

impl NodeResult {
    pub fn passthrough(payload: Payload, note: String) -> Self {
        Self {
            payload,
            note,
            resource: None,
        }
    }
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}
