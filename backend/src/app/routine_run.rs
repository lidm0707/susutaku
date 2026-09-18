//! Runs a routine: submits its instruction to the model (through the named
//! agent's config when set) and records a `RoutineRun`. Routines never touch
//! the task board — recurring automation is a separate entity from tasks.

use std::sync::Arc;

use task_rs::{ROUTINE_TRIGGER_MANUAL, RoutineRow, RoutineRunRow, StoreError};

use crate::port::outbound::{Inference, ModelEngines};

use super::task::TaskApp;

pub const INFER_MAX_TOKENS: usize = 1024;
pub const SUMMARY_CHARS: usize = 400;
pub const NOTE_NO_ENGINE: &str = "no inference engine configured";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoutineRunStatus {
    Ok,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct RoutineRun {
    pub status: RoutineRunStatus,
    pub output: String,
}

/// Execute one routine run and store its record. Owner-driven: the API gate
/// keeps this behind the owner role; the scheduler passes the same path.
pub async fn run_routine(
    app: &TaskApp,
    store: &crate::infra::postgres::Store,
    engine: Option<&Arc<dyn Inference>>,
    engines: Option<&dyn ModelEngines>,
    routine: &RoutineRow,
    trigger: &'static str,
) -> Result<RoutineRun, StoreError> {
    let outcome = execute(app, engine, engines, routine).await;
    let ok = outcome.status == RoutineRunStatus::Ok;
    store
        .record_routine_run(routine.id, trigger, ok, &summarize(&outcome))
        .await?;
    Ok(outcome)
}

/// Manual run from the API; returns the run plus the freshly stored row.
pub async fn run_routine_manual(
    app: &TaskApp,
    store: &crate::infra::postgres::Store,
    engine: Option<&Arc<dyn Inference>>,
    engines: Option<&dyn ModelEngines>,
    routine: &RoutineRow,
) -> Result<(RoutineRun, RoutineRunRow), StoreError> {
    let run = run_routine(app, store, engine, engines, routine, ROUTINE_TRIGGER_MANUAL).await?;
    let row = store
        .routine_runs(routine.id)
        .await?
        .into_iter()
        .next()
        .ok_or(StoreError::NoSuchRoutine)?;
    Ok((run, row))
}

async fn execute(
    app: &TaskApp,
    engine: Option<&Arc<dyn Inference>>,
    engines: Option<&dyn ModelEngines>,
    routine: &RoutineRow,
) -> RoutineRun {
    let Some(engine) = engine else {
        return fail(NOTE_NO_ENGINE);
    };
    // A named agent contributes its persona/prompt/output-format; the
    // routine's instruction is the task text. The agent's configured model
    // routes through `engines` first (cloud models), else the shared engine.
    let cfg = if routine.agent.is_empty() {
        None
    } else {
        app.agents.by_name(&routine.agent).await.ok().flatten()
    };
    let mut prompt = String::new();
    if let Some(cfg) = &cfg {
        for body in [&cfg.persona, &cfg.prompt, &cfg.output] {
            if !body.is_empty() {
                prompt.push_str(body);
                prompt.push('\n');
            }
        }
    }
    prompt.push_str(&routine.instruction);
    let model = cfg.as_ref().map(|c| c.model.trim()).unwrap_or("");
    let routed = engines
        .and_then(|e| e.engine_for(model))
        .filter(|_| !model.is_empty());
    let target = routed.as_ref().unwrap_or(engine);
    let rx = match target.submit(
        prompt,
        INFER_MAX_TOKENS,
        susutaku_mlx::tok::TokKind::Normal,
        false,
    ) {
        Ok(rx) => rx,
        Err(e) => return fail(&format!("inference failed: {e}")),
    };
    match rx.await {
        Ok(Ok(reply)) => RoutineRun {
            status: RoutineRunStatus::Ok,
            output: reply.text,
        },
        Err(_) | Ok(Err(_)) => fail("inference: engine dropped or rejected the job"),
    }
}

fn fail(note: &str) -> RoutineRun {
    RoutineRun {
        status: RoutineRunStatus::Failed,
        output: note.to_owned(),
    }
}

/// Summary preview for storage.
pub fn summarize(run: &RoutineRun) -> String {
    run.output.chars().take(SUMMARY_CHARS).collect()
}
