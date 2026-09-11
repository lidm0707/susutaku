//! Inbound port: the interface driving adapters (HTTP API) use to drive
//! the application core. Traits only — data types live in the domain layer.

mod chat;

pub use chat::ChatHandling;
