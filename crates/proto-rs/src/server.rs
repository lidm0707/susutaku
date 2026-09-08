use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use crate::codec::{read_frame, write_frame};
use crate::envelope::{Envelope, Kind};
use crate::{INBOUND_CHANNEL_CAPACITY, OUTBOUND_CHANNEL_CAPACITY};

/// One accepted client: inbound envelopes arrive on `rx`.
pub struct ClientConn {
    pub id: u64,
    pub peer: SocketAddr,
    pub rx: mpsc::Receiver<Envelope>,
}

/// Maps client_id -> outbound command sender.
pub struct Registry {
    clients: RwLock<HashMap<u64, mpsc::Sender<Envelope>>>,
    next_id: AtomicU64,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    pub async fn send(&self, client_id: u64, env: Envelope) -> Result<(), String> {
        let tx = self
            .clients
            .read()
            .map_err(|_| "client registry poisoned".to_string())?
            .get(&client_id)
            .cloned()
            .ok_or_else(|| format!("no client {client_id}"))?;
        tx.send(env)
            .await
            .map_err(|_| format!("client {client_id} stopped"))
    }

    pub fn ids(&self) -> Vec<u64> {
        self.clients
            .read()
            .map(|c| c.keys().copied().collect())
            .unwrap_or_default()
    }

    fn insert(&self, id: u64, tx: mpsc::Sender<Envelope>) {
        if let Ok(mut clients) = self.clients.write() {
            clients.insert(id, tx);
        }
    }

    fn remove(&self, id: u64) {
        if let Ok(mut clients) = self.clients.write() {
            clients.remove(&id);
        }
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

/// Accept loop. Each accepted connection is announced on `connected` and
/// registered in `registry` for outbound command dispatch.
pub async fn serve(
    listener: TcpListener,
    connected: mpsc::Sender<ClientConn>,
    registry: Arc<Registry>,
) -> Result<(), String> {
    loop {
        let (stream, peer) = listener
            .accept()
            .await
            .map_err(|e| format!("accept: {e}"))?;
        let id = registry.next_id.fetch_add(1, Ordering::Relaxed);
        let (in_tx, in_rx) = mpsc::channel::<Envelope>(INBOUND_CHANNEL_CAPACITY);
        let (out_tx, out_rx) = mpsc::channel::<Envelope>(OUTBOUND_CHANNEL_CAPACITY);
        registry.insert(id, out_tx);
        let conn = ClientConn {
            id,
            peer,
            rx: in_rx,
        };
        if connected.send(conn).await.is_err() {
            registry.remove(id);
            continue;
        }
        let registry = registry.clone();
        tokio::spawn(async move {
            handle_conn(stream, id, in_tx, out_rx, registry).await;
        });
    }
}

async fn handle_conn(
    stream: TcpStream,
    id: u64,
    in_tx: mpsc::Sender<Envelope>,
    mut out_rx: mpsc::Receiver<Envelope>,
    registry: Arc<Registry>,
) {
    let (mut rd, mut wr) = stream.into_split();
    let writer = tokio::spawn(async move {
        while let Some(env) = out_rx.recv().await {
            if write_frame(&mut wr, &env).await.is_err() {
                break;
            }
        }
    });
    while let Ok(env) = read_frame(&mut rd).await {
        if matches!(env.kind, Kind::Register { .. }) {
            continue;
        }
        if in_tx.send(env).await.is_err() {
            break;
        }
    }
    registry.remove(id);
    writer.abort();
}
