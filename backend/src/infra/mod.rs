//! Infrastructure adapters: model/chat providers, persistence, client nodes,
//! sandbox, web search, settings and alerts.

pub mod alerts;
pub mod chat_memory;
pub mod claude;
pub mod client;
pub mod codex;

pub mod manager_git;
pub mod manager_run;
pub mod model_client;
pub mod podman;
pub mod postgres;
pub mod project_git;
pub mod provider_quota;
pub mod search;
pub mod settings;
pub mod thread_env;
pub mod zai;
