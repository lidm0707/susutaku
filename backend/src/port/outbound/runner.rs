//! Port: runs shell commands inside the agent sandbox.

pub trait Runner: Send + Sync + 'static {
    fn run(&self, cmd: &str) -> Result<String, String>;
}
