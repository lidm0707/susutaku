//! Client hub: proto-rs server side plus pending-command correlation.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Duration;

use proto_rs::server::{ClientConn, Registry};
use proto_rs::{Envelope, Kind};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

/// Seconds to wait for a client Result before giving up.
pub const COMMAND_TIMEOUT_SECS: u64 = 30;

pub struct Hub {
    registry: Arc<Registry>,
    pending: RwLock<HashMap<u64, oneshot::Sender<String>>>,
    next_id: AtomicU64,
}

impl Hub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            registry: Arc::new(Registry::new()),
            pending: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        })
    }

    pub fn registry(&self) -> Arc<Registry> {
        self.registry.clone()
    }

    pub fn client_ids(&self) -> Vec<u64> {
        self.registry.ids()
    }

    /// Binds is done by the caller; serves until the process exits.
    pub async fn run(self: Arc<Self>, listener: TcpListener) -> Result<(), String> {
        let (tx, mut connected) = mpsc::channel::<ClientConn>(proto_rs::INBOUND_CHANNEL_CAPACITY);
        let registry = self.registry();
        tokio::spawn(proto_rs::server::serve(listener, tx, registry));
        while let Some(conn) = connected.recv().await {
            let hub = Arc::clone(&self);
            tokio::spawn(hub.serve_conn(conn));
        }
        Ok(())
    }

    async fn serve_conn(self: Arc<Self>, mut conn: ClientConn) {
        while let Some(env) = conn.rx.recv().await {
            if let Kind::Result { output } = env.kind {
                let sender = self
                    .pending
                    .write()
                    .ok()
                    .and_then(|mut p| p.remove(&env.id));
                if let Some(tx) = sender {
                    let _ = tx.send(output);
                }
            }
        }
    }

    pub async fn dispatch(&self, client_id: u64, cmd: String) -> Result<String, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending
            .write()
            .map_err(|_| "pending map poisoned".to_string())?
            .insert(id, tx);
        self.registry
            .send(
                client_id,
                Envelope {
                    id,
                    kind: Kind::Command { cmd },
                },
            )
            .await?;
        let wait = Duration::from_secs(COMMAND_TIMEOUT_SECS);
        match timeout(wait, rx).await {
            Ok(Ok(output)) => Ok(output),
            Ok(Err(_)) => Err(format!("client {client_id} dropped command {id}")),
            Err(_) => {
                if let Ok(mut p) = self.pending.write() {
                    p.remove(&id);
                }
                Err(format!("client {client_id} timed out on command {id}"))
            }
        }
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self {
            registry: Arc::new(Registry::new()),
            pending: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }
}

pub async fn bind(addr: SocketAddr) -> Result<TcpListener, String> {
    TcpListener::bind(addr)
        .await
        .map_err(|e| format!("bind tcp hub on {addr}: {e}"))
}
