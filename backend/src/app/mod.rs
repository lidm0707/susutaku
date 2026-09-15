//! Application layer: compound services (use cases). Each orchestrates domain
//! services through ports, never touching infrastructure directly.

pub mod board;
pub mod card_run;
pub mod chat;
pub mod events;
pub mod task;
pub mod say_hi;
pub mod schedule_work;

pub use board::BoardService;
pub use chat::ChatUseCase;
