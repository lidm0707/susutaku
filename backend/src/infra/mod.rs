//! Infrastructure adapters: model/chat providers, persistence, client nodes,
//! sandbox, web search, settings and alerts.

pub mod alerts;
pub mod chat_memory;
pub mod claude;
pub mod client;
pub mod codex;

pub mod model_client;
pub mod postgres;
pub mod provider_quota;
pub mod sandbox_jail;
pub mod search;
pub mod settings;
pub mod zai;
