//! Minimal test hub: accepts sandbox-client registrations and auto-dispatches
//! a probe command to every new client, printing the sandbox result. Used by
//! docker/compose/client-test.yml to verify backend/worker → client
//! round trips without a model server.
//!
//! Registered clients: `backend` (its built-in worker node) and `client`
//! (worker-only node) — both run commands in their own rootless sandbox.

use std::sync::Arc;

use proto_rs::envelope::{Envelope, Kind};
use proto_rs::server::{ClientConn, Registry};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

const HUB_PORT_ENV: &str = "HUB_PORT";
const DEFAULT_HUB_PORT: u16 = 8993;
const AUTO_CMD: &str = "echo STUB-HUB-ROUNDTRIP; id -u; pwd";
const RESULT_TIMEOUT_SECS: u64 = 30;

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var(HUB_PORT_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_HUB_PORT);
    let listener = TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("bind hub");
    let registry = Arc::new(Registry::new());
    let (tx, mut rx) = mpsc::channel::<ClientConn>(proto_rs::INBOUND_CHANNEL_CAPACITY);
    tokio::spawn(proto_rs::server::serve(listener, tx, registry.clone()));
    println!("hub-stub listening on 0.0.0.0:{port}");

    while let Some(conn) = rx.recv().await {
        println!(
            "[stub] client registered: id={} peer={}",
            conn.id, conn.peer
        );
        let ClientConn { id, mut rx, .. } = conn;
        let reg = registry.clone();
        tokio::spawn(async move {
            let env = Envelope {
                id,
                kind: Kind::Command {
                    cmd: AUTO_CMD.to_string(),
                },
            };
            if let Err(e) = reg.send(id, env).await {
                eprintln!("[stub] dispatch to {id} failed: {e}");
                return;
            }
            println!("[stub] dispatched to {id}: {AUTO_CMD}");
            match timeout(Duration::from_secs(RESULT_TIMEOUT_SECS), rx.recv()).await {
                Ok(Some(res)) => match res.kind {
                    Kind::Result { output } => {
                        println!("[stub] RESULT from {id}:\n{output}");
                    }
                    other => eprintln!("[stub] unexpected envelope from {id}: {other:?}"),
                },
                _ => eprintln!("[stub] result from {id} timed out"),
            }
        });
    }
}
