//! Executes a card's attached pipeline: topological stage walk over the spec,
//! per-stage log, result merged into the card's agent state.

use std::collections::VecDeque;

use kanban_rs::store::{AgentState, CardRow, StoreError};
use piplines::agent::{AgentNode, META_AGENT};
use piplines::graph::{NodeDef, PipelineSpec};
use piplines::payload::{Payload, PayloadKind};
use piplines::stage::Stage;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::kanban::KanbanApp;

pub const RUN_KEY: &str = "run";
pub const RUNNER_NAME: &str = "pipeline-runner";
pub const STAGE_NOTE_INGEST: &str = "seeded card title + description";
pub const STAGE_NOTE_PASSTHROUGH: &str = "payload passed through";
pub const STAGE_NOTE_OUTPUT: &str = "captured pipeline output";
pub const TEXT_SEP: &str = "\n\n";

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
    pub finished_at: String,
}

/// Result of a run attempt: record to persist + whether it should replace the agent name.
pub struct RunOutcome {
    pub record: RunRecord,
    pub agent_name: String,
}

pub async fn run_card_pipeline(app: &KanbanApp, card_id: i64) -> Result<RunRecord, StoreError> {
    let card = app
        .cards
        .get(card_id)
        .await?
        .ok_or(StoreError::NoSuchCard)?;
    let pipeline_id = card.pipeline_id.ok_or(StoreError::NoSuchPipeline)?;
    let pipeline = app
        .pipelines
        .list()
        .await?
        .into_iter()
        .find(|p| p.id == pipeline_id)
        .ok_or(StoreError::NoSuchPipeline)?;
    let spec: PipelineSpec =
        serde_json::from_str(&pipeline.spec).map_err(|e| StoreError::BadSpec(e.to_string()))?;
    spec.validate()
        .map_err(|e| StoreError::BadSpec(e.to_string()))?;

    let mut payload = Some(seed_payload(&card));
    let outcome = execute(
        &spec,
        payload.take().expect("seeded"),
        pipeline_id,
        &pipeline.name,
    );
    persist(app, card_id, &card, outcome).await
}

fn seed_payload(card: &CardRow) -> Payload {
    let mut text = card.title.clone();
    if !card.description.is_empty() {
        text.push_str(TEXT_SEP);
        text.push_str(&card.description);
    }
    Payload::text(text)
}

fn execute(spec: &PipelineSpec, seed: Payload, pipeline_id: i64, name: &str) -> RunOutcome {
    let mut stages: Vec<RunStage> = Vec::new();
    let mut failed = false;
    let mut payload = Some(seed);
    for node in topo_order(spec) {
        let Some(current) = payload.take() else {
            stages.push(skipped(node));
            continue;
        };
        if failed {
            stages.push(skipped(node));
            continue;
        }
        match apply_node(node, current) {
            Ok(next) => {
                payload = Some(next.payload);
                stages.push(RunStage {
                    node: node.id.clone(),
                    stage: node.stage.clone(),
                    status: StageStatus::Ok,
                    note: next.note,
                });
            }
            Err(note) => {
                failed = true;
                stages.push(RunStage {
                    node: node.id.clone(),
                    stage: node.stage.clone(),
                    status: StageStatus::Failed,
                    note,
                });
            }
        }
    }
    let output = payload
        .as_ref()
        .and_then(Payload::as_str)
        .map(str::to_owned);
    RunOutcome {
        agent_name: payload
            .as_ref()
            .and_then(|p| p.get_meta(META_AGENT))
            .unwrap_or(RUNNER_NAME)
            .to_owned(),
        record: RunRecord {
            pipeline_id,
            pipeline_name: name.to_owned(),
            status: if failed {
                StageStatus::Failed
            } else {
                StageStatus::Ok
            },
            stages,
            output,
            finished_at: now_iso(),
        },
    }
}

struct NodeResult {
    payload: Payload,
    note: String,
}

fn apply_node(node: &NodeDef, payload: Payload) -> Result<NodeResult, String> {
    match node.stage.as_str() {
        piplines::graph::STAGE_INGEST => Ok(NodeResult {
            payload,
            note: STAGE_NOTE_INGEST.to_owned(),
        }),
        piplines::graph::STAGE_AGENT => {
            let agent = AgentNode::from_params(&node.params)?;
            let next = agent.apply(payload).map_err(|e| e.to_string())?;
            let note = format!(
                "agent set to {}",
                next.get_meta(META_AGENT).unwrap_or_default()
            );
            Ok(NodeResult {
                payload: next,
                note,
            })
        }
        piplines::graph::STAGE_TRANSFORM => transform(node, payload),
        piplines::graph::STAGE_RENDER | piplines::graph::STAGE_OUTPUT_RESOURCE => Ok(NodeResult {
            payload,
            note: if node.stage == piplines::graph::STAGE_OUTPUT_RESOURCE {
                STAGE_NOTE_OUTPUT.to_owned()
            } else {
                STAGE_NOTE_PASSTHROUGH.to_owned()
            },
        }),
        other => Err(format!(
            "stage \"{other}\" is not wired to an engine yet (fetch/search/ref_image/parse/model_infer pending)"
        )),
    }
}

pub const OP_UPPER: &str = "upper";
pub const OP_LOWER: &str = "lower";
pub const OP_TRIM: &str = "trim";

fn transform(node: &NodeDef, mut payload: Payload) -> Result<NodeResult, String> {
    let op = node
        .params
        .get("op")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "transform node params missing \"op\"".to_string())?;
    let text = payload.as_str().ok_or("transform needs a text payload")?;
    let transformed = match op {
        OP_UPPER => text.to_uppercase(),
        OP_LOWER => text.to_lowercase(),
        OP_TRIM => text.trim().to_owned(),
        other => return Err(format!("unknown transform op: {other}")),
    };
    payload.data = transformed.into_bytes();
    payload.kind = PayloadKind::Text;
    Ok(NodeResult {
        payload,
        note: format!("op {op} applied"),
    })
}

fn skipped(node: &NodeDef) -> RunStage {
    RunStage {
        node: node.id.clone(),
        stage: node.stage.clone(),
        status: StageStatus::Failed,
        note: "skipped: earlier stage failed".to_owned(),
    }
}

/// Kahn ordering over the validated acyclic spec; falls back to spec order.
fn topo_order(spec: &PipelineSpec) -> Vec<&NodeDef> {
    let idx = |id: &str| spec.nodes.iter().position(|n| n.id == id);
    let mut indegree = vec![0usize; spec.nodes.len()];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); spec.nodes.len()];
    for link in &spec.links {
        if let (Some(from), Some(to)) = (idx(&link.from), idx(&link.to)) {
            adj[from].push(to);
            indegree[to] += 1;
        }
    }
    let mut queue: VecDeque<usize> = (0..spec.nodes.len())
        .filter(|&i| indegree[i] == 0)
        .collect();
    let mut ordered: Vec<&NodeDef> = Vec::with_capacity(spec.nodes.len());
    while let Some(i) = queue.pop_front() {
        ordered.push(&spec.nodes[i]);
        for &next in &adj[i] {
            indegree[next] -= 1;
            if indegree[next] == 0 {
                queue.push_back(next);
            }
        }
    }
    if ordered.len() == spec.nodes.len() {
        ordered
    } else {
        spec.nodes.iter().collect()
    }
}

async fn persist(
    app: &KanbanApp,
    card_id: i64,
    card: &CardRow,
    outcome: RunOutcome,
) -> Result<RunRecord, StoreError> {
    let mut state = card
        .agent_state
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
    let obj = state
        .as_object_mut()
        .ok_or_else(|| StoreError::BadSpec("agent state is not a JSON object".into()))?;
    obj.insert(
        RUN_KEY.to_owned(),
        serde_json::to_value(&outcome.record).map_err(|e| StoreError::BadSpec(e.to_string()))?,
    );
    let name = if card.agent_name.is_some() {
        card.agent_name.clone().expect("checked above")
    } else {
        outcome.agent_name
    };
    app.cards
        .set_agent(card_id, &AgentState { name, state })
        .await?;
    Ok(outcome.record)
}

fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}
