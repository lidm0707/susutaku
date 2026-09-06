//! Inbound port: the interface driving adapters (HTTP API) use to drive
//! the application core.

mod chat;

pub use chat::{ChatCmd, ChatHandling, ChatOutcome};
