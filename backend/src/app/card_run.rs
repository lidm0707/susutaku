//! Runs a card's assigned agent: the agent config (persona, instruction,
//! output format) is prepended to the card's title/description and submitted
//! to the model; the run ledger is merged into the card's agent state.

use std::sync::Arc;

use task_rs::store::{CardRow, RunRecordNew, StoreError};
use task_rs::{AgentConfigRow, COLUMN_DOING, COLUMN_DONE, COLUMN_FAILED};

use crate::domain::CardMove;
use crate::port::outbound::Inference;

use super::task::TaskApp;

pub const RUN_KEY: &str = "run";
pub const MOVE_POSITION_TOP: i32 = 0;
pub const NOTE_NO_AGENT: &str = "no agent assigned to the card";
pub const NOTE_NO_ENGINE: &str = "no inference engine configured";
pub const INFER_MAX_TOKENS: usize = 1024;
pub const OUTPUT_PREVIEW_MAX: usize = 400;
pub const PROMPT_PERSONA: &str = "persona: ";
pub const PROMPT_INSTRUCTION: &str = "instruction: ";
pub const PROMPT_OUTPUT: &str = "output format: ";
pub const PROMPT_TASK: &str = "\n\ntask:\n";
pub const TEXT_SEP: &str = "\n\n";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Ok,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct RunRecord {
    pub agent: String,
    pub status: RunStatus,
    pub output: Option<String>,
    pub finished_at: String,
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Run the card's assigned agent once. A card without an agent still records
/// a failed run so the card moves on instead of silently staying put.
pub async fn run_card(
    app: &TaskApp,
    engine: Option<Arc<dyn Inference>>,
    card_id: i64,
    trigger: &'static str,
) -> Result<RunRecord, StoreError> {
    let card = app
        .cards
        .get(card_id)
        .await?
        .ok_or(StoreError::NoSuchCard)?;
    move_to_column(app, card_id, &card.column_id, COLUMN_DOING).await?;
    let record = execute(app, engine.as_ref(), &card).await;
    persist(app, card_id, &card, record, trigger).await
}

async fn execute(app: &TaskApp, engine: Option<&Arc<dyn Inference>>, card: &CardRow) -> RunRecord {
    let agent = card.agent_name.clone().unwrap_or_default();
    match (engine, card.agent_name.as_deref()) {
        (_, None) => fail(&agent, NOTE_NO_AGENT),
        (None, Some(_)) => fail(&agent, NOTE_NO_ENGINE),
        (Some(engine), Some(name)) => match app.agents.by_name(name).await.ok().flatten() {
            None => fail(&agent, NOTE_NO_AGENT),
            Some(cfg) => infer(engine, &cfg, card, &agent).await,
        },
    }
}

fn fail(agent: &str, note: &str) -> RunRecord {
    RunRecord {
        agent: agent.to_owned(),
        status: RunStatus::Failed,
        output: Some(note.to_owned()),
        finished_at: now_iso(),
    }
}

async fn infer(
    engine: &Arc<dyn Inference>,
    cfg: &AgentConfigRow,
    card: &CardRow,
    agent: &str,
) -> RunRecord {
    use susutaku_mlx::tok::TokKind;
    let mut prompt = String::new();
    for (header, body) in [
        (PROMPT_PERSONA, cfg.persona.as_str()),
        (PROMPT_INSTRUCTION, cfg.prompt.as_str()),
        (PROMPT_OUTPUT, cfg.output.as_str()),
    ] {
        if !body.is_empty() {
            prompt.push_str(header);
            prompt.push_str(body);
            prompt.push('\n');
        }
    }
    let mut task = card.title.clone();
    if !card.description.is_empty() {
        task.push_str(TEXT_SEP);
        task.push_str(&card.description);
    }
    prompt.push_str(PROMPT_TASK);
    prompt.push_str(&task);
    let rx = match engine.submit(prompt, INFER_MAX_TOKENS, TokKind::Normal, false) {
        Ok(rx) => rx,
        Err(e) => return fail(agent, &format!("{NOTE_INFER_ERR}{e}")),
    };
    match rx.await {
        Ok(Ok(reply)) => RunRecord {
            agent: agent.to_owned(),
            status: RunStatus::Ok,
            output: Some(reply.text),
            finished_at: now_iso(),
        },
        Err(_) | Ok(Err(_)) => fail(agent, NOTE_INFER_DROP),
    }
}

pub const NOTE_INFER_ERR: &str = "inference failed: ";
pub const NOTE_INFER_DROP: &str = "inference: engine dropped or rejected the job";

/// Move a card unless it already sits in the target column.
async fn move_to_column(
    app: &TaskApp,
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

async fn persist(
    app: &TaskApp,
    card_id: i64,
    card: &CardRow,
    record: RunRecord,
    trigger: &'static str,
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
        serde_json::to_value(&record).map_err(|e| StoreError::BadSpec(e.to_string()))?,
    );
    // Preference (`agent_name`) is never mutated by a run: the state write
    // touches only the ledger.
    app.cards
        .set_agent_state(
            card_id,
            &serde_json::to_string(&state).map_err(|e| StoreError::BadSpec(e.to_string()))?,
        )
        .await?;
    let ok = record.status == RunStatus::Ok;
    let summary = record.output.clone().unwrap_or_default();
    let summary: String = summary.chars().take(OUTPUT_PREVIEW_MAX).collect();
    app.cards
        .record_run(RunRecordNew {
            card_id,
            trigger: trigger.to_owned(),
            agent: record.agent.clone(),
            ok,
            summary,
        })
        .await?;
    let target = match record.status {
        RunStatus::Ok => COLUMN_DONE,
        RunStatus::Failed => COLUMN_FAILED,
    };
    move_to_column(app, card_id, &card.column_id, target).await?;
    Ok(record)
}
