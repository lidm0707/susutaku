//! Scheduled z.ai "say hi": fires at the configured HH:MM in the configured
//! IANA timezone (server-local time when none is set). With an interval set it
//! repeats every N minutes after the start time; otherwise once per day.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use chrono::{DateTime, FixedOffset, Local, NaiveTime, Utc};
use chrono_tz::Tz;

use crate::infra::zai_settings::{parse_hhmm, SettingsState};

pub const TICK_SECS: u64 = 30;
const LAST_FIRED_LOCK: &str = "say hi last-fired lock";

#[derive(Default)]
pub(crate) struct LastFired(Option<String>);

pub fn spawn(state: Arc<SettingsState>) {
    let last = Arc::new(RwLock::new(LastFired::default()));
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(TICK_SECS));
        loop {
            tick.tick().await;
            run_once(&state, &last).await;
        }
    });
}

/// One scheduler pass; exposed for tests.
pub(crate) async fn run_once(state: &SettingsState, last: &RwLock<LastFired>) {
    let zai = state.zai();
    let Some(raw_time) = zai.say_hi_time.as_deref() else {
        return;
    };
    let Ok((h, m)) = parse_hhmm(raw_time) else {
        return;
    };
    let interval = zai.say_hi_interval_mins.unwrap_or(0);
    let now = now_tz(zai.timezone.as_deref());
    let key = match due_key(&now, (h, m), interval) {
        Some(k) => k,
        None => return,
    };
    if last.read().expect(LAST_FIRED_LOCK).0.as_deref() == Some(key.as_str()) {
        return;
    }
    *last.write().expect(LAST_FIRED_LOCK) = LastFired(Some(key));
    let token = match state.zai_token() {
        Ok(token) => token,
        Err(e) => {
            tracing::warn!("scheduled z.ai say hi skipped: {e}");
            return;
        }
    };
    let result = tokio::task::spawn_blocking(move || zai_api::quota::say_hi(&token)).await;
    match result {
        Ok(Ok(reply)) => tracing::info!("scheduled z.ai say hi: {reply}"),
        Ok(Err(e)) => tracing::warn!("scheduled z.ai say hi failed: {e}"),
        Err(e) => tracing::warn!("scheduled z.ai say hi join failed: {e}"),
    }
}

/// Fire key for this pass: `None` means not due. Without an interval this is
/// the date (once per day, at/after the start time); with one it is the
/// date plus the elapsed interval step, so every step fires exactly once.
fn due_key(now: &DateTime<FixedOffset>, start: (u32, u32), interval_mins: u64) -> Option<String> {
    const DAY_MINS: i64 = 24 * 60;
    let start = NaiveTime::from_hms_opt(start.0, start.1, 0)?;
    if now.time() < start {
        return None;
    }
    let date = now.format("%Y-%m-%d").to_string();
    if interval_mins == 0 {
        return Some(date);
    }
    let since = now.time() - start;
    let step = (since.num_minutes() / interval_mins as i64).clamp(0, DAY_MINS);
    Some(format!("{date}:{step}"))
}

fn now_tz(name: Option<&str>) -> DateTime<FixedOffset> {
    match name.and_then(|n| n.parse::<Tz>().ok()) {
        Some(tz) => Utc::now().with_timezone(&tz).fixed_offset(),
        None => Local::now().fixed_offset(),
    }
}
