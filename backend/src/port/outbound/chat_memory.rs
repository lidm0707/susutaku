//! Chat memory port: long-term semantic storage of chat exchanges.

use crate::domain::MemoryHit;

/// Port: remembers chat entries and recalls the ones similar to a query.
pub trait ChatMemory: Send + Sync + 'static {
    fn remember(&self, thread: &str, role: &str, text: &str) -> Result<(), String>;
    fn recall(&self, thread: &str, query: &str, k: usize) -> Result<Vec<MemoryHit>, String>;
}
