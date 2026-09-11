//! Model selection port.

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
