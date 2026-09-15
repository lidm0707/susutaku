//! Scheduled work: polls routines with a cron expression and runs them when
//! due. Next-run bookkeeping lives in a `RwLock` state map. Routines are the
//! only scheduled entity — tasks are never recurring (Routine ≠ Task).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use utoipa::ToSchema;
use work::services::cron::Cron;

use super::task::TaskApp;
use crate::port::outbound::{Inference, ModelEngines};

pub const TICK_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CronJobState {
    pub card_id: i64,
    pub next_run: u64,
}

#[derive(Default)]
struct State {
    next: HashMap<i64, u64>,
}

/// Shared handle to inspect the scheduler's upcoming runs.
#[derive(Clone)]
pub struct ScheduleHandle {
    state: Arc<RwLock<State>>,
}

impl Default for ScheduleHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl ScheduleHandle {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(State::default())),
        }
    }

    pub fn entries(&self) -> Vec<CronJobState> {
        let state = self.state.read().expect(SCHEDULE_LOCK);
        state
            .next
            .iter()
            .map(|(card_id, next_run)| CronJobState {
                card_id: *card_id,
                next_run: *next_run,
            })
            .collect()
    }
}

pub const SCHEDULE_LOCK: &str = "schedule state lock";

/// One scheduler pass without spawning a task; used by tests and spawn.
pub async fn run_once(
    app: &TaskApp,
    engine: Option<&Arc<dyn Inference>>,
    engines: Option<&dyn ModelEngines>,
    handle: &ScheduleHandle,
) {
    run_due(app, engine, engines, &handle.state).await;
}

/// Spawns the background ticker; returns the shared inspection handle.
pub fn spawn(
    app: Arc<TaskApp>,
    engine: Option<Arc<dyn Inference>>,
    engines: Option<Arc<dyn ModelEngines>>,
) -> ScheduleHandle {
    let handle = ScheduleHandle::new();
    let state = Arc::clone(&handle.state);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(TICK_SECS));
        loop {
            tick.tick().await;
            run_due(&app, engine.as_ref(), engines.as_deref(), &state).await;
        }
    });
    handle
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Due decisions keep the lock scope tiny; pipeline runs happen unlocked.
async fn run_due(
    app: &TaskApp,
    engine: Option<&Arc<dyn Inference>>,
    engines: Option<&dyn ModelEngines>,
    state: &Arc<RwLock<State>>,
) {
    let now = unix_now();
    let routines = app
        .store
        .list_routines()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.enabled)
        .collect::<Vec<_>>();
    let scheduled: HashSet<i64> = routines.iter().map(|r| r.id).collect();
    let mut pending: Vec<(i64, Cron, u64)> = Vec::new();
    {
        let mut guard = state.write().expect(SCHEDULE_LOCK);
        for routine in &routines {
            let Ok(cron) = Cron::parse(&routine.cron) else {
                guard.next.remove(&routine.id);
                continue;
            };
            let due = *guard
                .next
                .entry(routine.id)
                .or_insert_with(|| cron.next_after(now));
            pending.push((routine.id, cron, due));
        }
        guard.next.retain(|id, _| scheduled.contains(id));
    }
    for (id, cron, due) in pending {
        if now < due {
            continue;
        }
        let run = async {
            let Some(routine) = app.store.get_routine(id).await.ok().flatten() else {
                return false;
            };
            super::routine_run::run_routine(
                app,
                &app.store,
                engine.cloned().as_ref(),
                engines,
                &routine,
                task_rs::ROUTINE_TRIGGER_CRON,
            )
            .await
            .is_ok()
        }
        .await;
        let next = if run {
            cron.next_after(now)
        } else {
            // retry a broken run on the next tick
            now + TICK_SECS
        };
        state.write().expect(SCHEDULE_LOCK).next.insert(id, next);
    }
}
