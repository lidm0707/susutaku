//! Executes a card's attached pipeline: topological stage walk over the spec,
//! per-stage log, result merged into the card's agent state.

mod http;
mod model;
mod nodes;
mod topo;

pub use model::{RunOutcome, RunRecord, RunStage, StageStatus, now_iso};

use std::collections::HashMap;
use std::sync::Arc;

use kanban_rs::resource::UpsertResource;
use kanban_rs::store::{CardRow, RunRecordNew, StoreError};
use kanban_rs::{AgentConfigRow, COLUMN_DOING, COLUMN_DONE, COLUMN_FAILED};
use piplines::agent::META_AGENT;
use piplines::graph::{NodeDef, PipelineSpec};
use piplines::payload::{Payload, PayloadKind};

use crate::domain::CardMove;
use crate::port::outbound::Inference;

use super::kanban::KanbanApp;
use model::{NODE_PREPARE, STAGE_PREPARE, TEXT_SEP};
use nodes::{apply_node, skipped};
use topo::topo_order;

pub const RUN_KEY: &str = "run";
pub const RUNNER_NAME: &str = "pipeline-runner";
pub const MOVE_POSITION_TOP: i32 = 0;
pub const PIPELINE_ID_NONE: i64 = 0;

pub async fn run_card_pipeline(
    app: &KanbanApp,
    engine: Option<Arc<dyn Inference>>,
    card_id: i64,
    trigger: &'static str,
) -> Result<RunRecord, StoreError> {
    let card = app
        .cards
        .get(card_id)
        .await?
        .ok_or(StoreError::NoSuchCard)?;
    let outcome = match load_spec(app, &card).await {
        Ok((spec, pipeline_id, pipeline_name)) => {
            move_to_column(app, card_id, &card.column_id, COLUMN_DOING).await?;
            execute(
                app,
                engine.as_ref(),
                &spec,
                seed_payload(&card),
                pipeline_id,
                &pipeline_name,
            )
            .await
        }
        // The run cannot start: still record a failed run so the card moves
        // to the failed column instead of silently staying put.
        Err(e) => RunOutcome {
            record: RunRecord {
                pipeline_id: card.pipeline_id.unwrap_or(PIPELINE_ID_NONE),
                pipeline_name: String::new(),
                status: StageStatus::Failed,
                stages: vec![RunStage {
                    node: NODE_PREPARE.to_owned(),
                    stage: STAGE_PREPARE.to_owned(),
                    status: StageStatus::Failed,
                    note: e.to_string(),
                }],
                output: None,
                resources: Vec::new(),
                finished_at: now_iso(),
            },
            agent_name: RUNNER_NAME.to_owned(),
            resources: Vec::new(),
        },
    };
    persist(app, card_id, &card, outcome, trigger).await
}

/// Load and validate the spec attached to the card, if any.
async fn load_spec(
    app: &KanbanApp,
    card: &CardRow,
) -> Result<(PipelineSpec, i64, String), StoreError> {
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
    Ok((spec, pipeline_id, pipeline.name.clone()))
}

/// Move a card unless it already sits in the target column.
async fn move_to_column(
    app: &KanbanApp,
    card_id: i64,
    current: &str,
    target: &str,
) -> Result<(), StoreError> {
    if current == target {
        return Ok(());
    }
    app.cards
        .move_card(CardMove {
            id: card_id,
            column_id: target.to_owned(),
            position: MOVE_POSITION_TOP,
        })
        .await
}

/// Dry-run a saved pipeline with caller-supplied seed text: no card, no
/// resource writes, no agent-state changes. Returns the run record only.
pub async fn test_pipeline(
    app: &KanbanApp,
    engine: Option<Arc<dyn Inference>>,
    pipeline_id: i64,
    seed_text: &str,
) -> Result<RunRecord, StoreError> {
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
    let outcome = execute(
        app,
        engine.as_ref(),
        &spec,
        Payload::text(seed_text),
        pipeline_id,
        &pipeline.name,
    )
    .await;
    Ok(outcome.record)
}

fn seed_payload(card: &CardRow) -> Payload {
    let mut text = card.title.clone();
    if !card.description.is_empty() {
        text.push_str(TEXT_SEP);
        text.push_str(&card.description);
    }
    Payload::text(text)
}

async fn execute(
    app: &KanbanApp,
    engine: Option<&Arc<dyn Inference>>,
    spec: &PipelineSpec,
    seed: Payload,
    pipeline_id: i64,
    name: &str,
) -> RunOutcome {
    let mut stages: Vec<RunStage> = Vec::new();
    let mut failed = false;
    // Output of each completed node, routed to consumers via links.
    let mut done: HashMap<&str, Payload> = HashMap::new();
    let mut resources: Vec<(String, String)> = Vec::new();
    let mut final_output: Option<Payload> = None;
    let mut agent_name = RUNNER_NAME.to_owned();
    let mut agent_cfg: Option<AgentConfigRow> = None;
    for node in topo_order(spec) {
        let inputs = inputs_for(spec, node, &seed, &done);
        let Some(payload) = inputs else {
            stages.push(skipped(node));
            continue;
        };
        if failed {
            stages.push(skipped(node));
            continue;
        }
        match apply_node(node, payload, engine, agent_cfg.as_ref()).await {
            Ok(next) => {
                if let Some(res) = next.resource {
                    resources.push(res);
                }
                if let Some(agent) = next.payload.get_meta(META_AGENT)
                    && agent_name != agent
                {
                    agent_name = agent.to_owned();
                    agent_cfg = app.agents.by_name(agent).await.ok().flatten();
                }
                final_output = Some(next.payload.clone());
                done.insert(node.id.as_str(), next.payload);
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
    // Output = last text payload produced, falling back to the seed.
    let output = final_output.and_then(|p| p.as_str().map(str::to_owned));
    let record = RunRecord {
        pipeline_id,
        pipeline_name: name.to_owned(),
        status: if failed {
            StageStatus::Failed
        } else {
            StageStatus::Ok
        },
        stages,
        output,
        resources: resources.iter().map(|(name, _)| name.clone()).collect(),
        finished_at: now_iso(),
    };
    RunOutcome {
        agent_name,
        resources,
        record,
    }
}

/// Gather inputs for a node: join its link predecessors' outputs; roots take
/// the seed. None if a predecessor did not produce output (failed/skipped).
fn inputs_for(
    spec: &PipelineSpec,
    node: &NodeDef,
    seed: &Payload,
    done: &HashMap<&str, Payload>,
) -> Option<Payload> {
    let preds: Vec<&str> = spec
        .links
        .iter()
        .filter(|l| l.to == node.id)
        .map(|l| l.from.as_str())
        .collect();
    if preds.is_empty() {
        return Some(seed.clone());
    }
    let mut texts: Vec<&str> = Vec::new();
    let mut first: Option<Payload> = None;
    for pred in preds {
        let payload = done.get(pred)?;
        if let Some(text) = payload.as_str() {
            texts.push(text);
        }
        if first.is_none() {
            first = Some(payload.clone());
        }
    }
    let mut merged = first?;
    if texts.len() > 1 {
        merged.data = texts.join(TEXT_SEP).into_bytes();
        merged.kind = PayloadKind::Text;
    }
    Some(merged)
}

async fn persist(
    app: &KanbanApp,
    card_id: i64,
    card: &CardRow,
    outcome: RunOutcome,
    trigger: &'static str,
) -> Result<RunRecord, StoreError> {
    for (name, content) in &outcome.resources {
        app.resources
            .upsert(UpsertResource {
                card_id,
                name,
                content,
            })
            .await?;
    }
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
    // Preference (`agent_name`) is never mutated by a run: the state write
    // touches only the ledger; the run's agent goes to the record fields.
    app.cards
        .set_agent_state(
            card_id,
            &serde_json::to_string(&state).map_err(|e| StoreError::BadSpec(e.to_string()))?,
        )
        .await?;
    let ok = outcome.record.status == StageStatus::Ok;
    let summary = outcome.record.output.clone().unwrap_or_default();
    app.cards
        .record_run(RunRecordNew {
            card_id,
            trigger: trigger.to_owned(),
            agent: outcome.agent_name.clone(),
            ok,
            summary,
        })
        .await?;
    let target = match outcome.record.status {
        StageStatus::Ok => COLUMN_DONE,
        StageStatus::Failed => COLUMN_FAILED,
    };
    move_to_column(app, card_id, &card.column_id, target).await?;
    Ok(outcome.record)
}
