//! Backend library: API, application, domain, infrastructure and port layers.

pub mod api;
pub mod app;
pub mod domain;
pub mod infra;
pub mod port;

pub use infra::local_settings::{FIELD_ENDPOINT, LOCAL_SECTION};
