//! Live check: fresh snapshot via `codex exec`, then store + read back.
//! Usage: cargo run -p codex-usage-rs --example fresh_smoke

use codex_usage_rs::Store;
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let home = PathBuf::from(std::env::var("HOME").unwrap()).join(".codex");
    let store = Store::connect("postgres://susutaku:susutaku@localhost:5434/susutaku")
        .await
        .expect("connect");
    store.ensure_table().await.expect("table");
    codex_usage_rs::poll_once(&store, &home, true).await;
    let latest = store.latest().await.expect("latest");
    println!("latest: {latest:?}");
}
