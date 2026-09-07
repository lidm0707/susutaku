//! Scheduled work: polls cards with a cron expression and runs their attached
//! pipeline when due. Next-run bookkeeping lives in a `RwLock` state map.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use utoipa::ToSchema;
use work::services::cron::Cron;

use super::kanban::KanbanApp;

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
pub async fn run_once(app: &KanbanApp, handle: &ScheduleHandle) {
    run_due(app, &handle.state).await;
}

/// Spawns the background ticker; returns the shared inspection handle.
pub fn spawn(app: Arc<KanbanApp>) -> ScheduleHandle {
    let handle = ScheduleHandle::new();
    let state = Arc::clone(&handle.state);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(TICK_SECS));
        loop {
            tick.tick().await;
            run_due(&app, &state).await;
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
async fn run_due(app: &KanbanApp, state: &Arc<RwLock<State>>) {
    let now = unix_now();
    let cards = app.cards.list(None).await.unwrap_or_default();
    let scheduled: HashSet<i64> = cards
        .iter()
        .filter(|c| c.cron.is_some())
        .map(|c| c.id)
        .collect();
    let mut pending: Vec<(i64, Cron, u64)> = Vec::new();
    {
        let mut guard = state.write().expect(SCHEDULE_LOCK);
        for card in cards.iter().filter(|c| c.cron.is_some()) {
            let Some(Ok(cron)) = card.cron.as_deref().map(Cron::parse) else {
                guard.next.remove(&card.id);
                continue;
            };
            let due = *guard
                .next
                .entry(card.id)
                .or_insert_with(|| cron.next_after(now));
            pending.push((card.id, cron, due));
        }
        guard.next.retain(|id, _| scheduled.contains(id));
    }
    for (id, cron, due) in pending {
        if now < due {
            continue;
        }
        let next = if super::pipeline_run::run_card_pipeline(app, id)
            .await
            .is_ok()
        {
            cron.next_after(now)
        } else {
            // retry a broken pipeline on the next tick
            now + TICK_SECS
        };
        state.write().expect(SCHEDULE_LOCK).next.insert(id, next);
    }
}
