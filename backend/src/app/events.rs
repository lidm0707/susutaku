//! Board event bus: broadcasts board mutations so websocket subscribers
//! (web UI) can refresh the page they are viewing.

use std::sync::OnceLock;

use serde::Serialize;
use tokio::sync::broadcast;

const CHANNEL_CAPACITY: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Card,
    Cron,
    Attachment,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardEvent {
    pub kind: EventKind,
}

#[derive(Clone)]
struct EventBus {
    tx: broadcast::Sender<BoardEvent>,
}

impl EventBus {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self { tx }
    }

    fn publish(&self, kind: EventKind) {
        let _ = self.tx.send(BoardEvent { kind });
    }
}

/// Process-wide bus: chat board ops and HTTP handlers mutate through
/// different services, so the bus is shared statically.
fn bus() -> &'static EventBus {
    static BUS: OnceLock<EventBus> = OnceLock::new();
    BUS.get_or_init(EventBus::new)
}

pub fn publish(kind: EventKind) {
    bus().publish(kind);
}

pub fn subscribe() -> broadcast::Receiver<BoardEvent> {
    bus().tx.subscribe()
}
