//! Infrastructure: codex usage snapshot store + scheduler bootstrap.

use std::path::PathBuf;
use std::sync::Arc;

use codex_usage_rs::Store;

use crate::infra::kanban::DATABASE_URL_ENV;

/// Connect to the usage Postgres store (same DB as kanban).
pub async fn connect() -> Store {
    let url =
        std::env::var(DATABASE_URL_ENV).unwrap_or_else(|_| kanban_rs::Store::default_url().into());
    Store::connect(&url)
        .await
        .expect("codex usage postgres connect")
}

/// Create the table if missing and start the polling task.
pub fn spawn_scheduler(store: Arc<Store>, codex_home: PathBuf) {
    let ensure = store.clone();
    tokio::spawn(async move {
        if let Err(err) = ensure.ensure_table().await {
            tracing::warn!("codex usage ensure_table: {err}");
            return;
        }
        codex_usage_rs::spawn(store, codex_home, fresh_enabled());
    });
}

pub const FRESH_ENV: &str = "SUSUTAKU_CODEX_USAGE_FRESH";
pub const FRESH_DEFAULT: bool = true;

/// Fresh mode runs a cheap `codex exec` each poll so snapshots are current;
/// disable with `SUSUTAKU_CODEX_USAGE_FRESH=0` to never spend quota.
fn fresh_enabled() -> bool {
    std::env::var(FRESH_ENV)
        .ok()
        .map(|v| v != "0")
        .unwrap_or(FRESH_DEFAULT)
}
