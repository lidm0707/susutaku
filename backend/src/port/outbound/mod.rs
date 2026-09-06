//! Outbound ports: interfaces the application core needs from the outside
//! world. Implemented by infrastructure adapters.

mod inference;
mod model;
mod runner;
mod search;

pub use inference::{GenReply, Inference, ReplyRx};
pub use model::ModelSwitch;
pub use runner::Runner;
pub use search::{Fetcher, Searcher};
