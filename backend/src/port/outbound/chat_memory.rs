//! Chat memory port: long-term semantic storage of chat exchanges.

/// One recalled past exchange.
pub struct MemoryHit {
    pub role: String,
    pub text: String,
}

/// Port: remembers chat entries and recalls the ones similar to a query.
pub trait ChatMemory: Send + Sync + 'static {
    fn remember(&self, role: &str, text: &str) -> Result<(), String>;
    fn recall(&self, query: &str, k: usize) -> Result<Vec<MemoryHit>, String>;
}
