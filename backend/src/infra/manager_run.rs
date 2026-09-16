//! Manager-backed agent-run adapter: routes a chat toolcall to the named
//! agent's own podman sandbox via the manager slot.

use crate::port::outbound::AgentRun;
use manager_rs::manager::Manager;
use std::sync::Arc;

pub struct ManagerRun {
    manager: Arc<Manager>,
}

impl ManagerRun {
    pub fn new(manager: Arc<Manager>) -> Self {
        Self { manager }
    }
}

impl AgentRun for ManagerRun {
    fn run_for(&self, agent: &str, cmd: &str) -> Result<String, String> {
        self.manager.run(agent, cmd)
    }
}
