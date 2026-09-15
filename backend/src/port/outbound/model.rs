//! Model selection port.

use std::sync::Arc;

use super::Inference;

/// Port: maps an agent's configured model name to a live inference engine.
/// Unknown models resolve to `None` and the caller falls back to the shared
/// engine.
pub trait ModelEngines: Send + Sync + 'static {
    fn engine_for(&self, model: &str) -> Option<Arc<dyn Inference>>;
}

/// Port: runtime reconfiguration of the local-model endpoint.
pub trait ModelEndpoint: Send + Sync + 'static {
    fn set_base_url(&self, url: &str);
    fn base_url(&self) -> String;
}

/// Port: selects which local model handles generations.
pub trait ModelSwitch: Send + Sync + 'static {
    fn select(&self, name: &str) -> Result<(), String>;
    fn selected(&self) -> Option<String>;
}
