//! Application layer: compound services (use cases). Each orchestrates domain
//! services through ports, never touching infrastructure directly.

pub mod chat;
pub mod kanban;

pub use chat::ChatUseCase;
