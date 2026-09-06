//! Model selection port.

/// Port: selects which local model handles generations.
pub trait ModelSwitch: Send + Sync + 'static {
    fn select(&self, name: &str) -> Result<(), String>;
    fn selected(&self) -> Option<String>;
}
