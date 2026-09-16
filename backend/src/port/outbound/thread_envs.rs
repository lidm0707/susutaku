//! Port: per-thread sandbox environments.

use crate::port::outbound::Runner;
use std::sync::Arc;

pub trait ThreadEnvs: Send + Sync + 'static {
    /// Returns the runner rooted at this thread's own sandbox, creating the
    /// environment on first use. `agent` names the agent config the thread
    /// runs as (informational — the env is per thread, not per agent).
    /// `Err` when the environment cannot be created.
    fn runner_for(
        &self,
        thread_id: &str,
        agent: Option<&str>,
    ) -> Result<Arc<dyn Runner>, String>;
}
