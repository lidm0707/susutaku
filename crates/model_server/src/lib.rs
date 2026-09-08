//! Model server library: engine, HTTP API and client hub.

pub mod api;
pub mod engine;
pub mod hub;
pub mod ports;

pub const MODELS_ROOT: &str = "models";
pub const HTTP_PORT: u16 = 8992;
pub const TCP_PORT: u16 = 8993;
pub const HTTP_PORT_ENV: &str = "MODEL_SERVER_HTTP_PORT";
pub const TCP_PORT_ENV: &str = "MODEL_SERVER_TCP_PORT";
pub const DEFAULT_MAX_TOKENS: usize = 512;
