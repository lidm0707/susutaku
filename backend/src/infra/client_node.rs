//! Client node: registers with the local-model hub and runs dispatched
//! commands in the agent sandbox. Commands carrying a non-empty `agent` are
//! executed in that agent's own work tree (spawned on demand), so a shared
//! machine runs per-agent work exactly like the backend's manager does.

use std::sync::Arc;
use std::time::Duration;

use manager_rs::ManagerProcess;
use proto_rs::client;
use proto_rs::{AgentBrief, ClientMeta, ROLE_MODEL, ROLE_WORKER};
use tokio::time::sleep;

use crate::infra::sandbox_jail::AgentSandbox;
use crate::port::outbound::Runner;

const OS_NAME: &str = std::env::consts::OS;
const ARCH_NAME: &str = std::env::consts::ARCH;
const RECONNECT_SECS: u64 = 3;
/// Role the installer probe decided for this machine (exported by install.sh).
pub const CLIENT_ROLE_ENV: &str = "SUSUTAKU_CLIENT_ROLE";
/// Installed RAM in GiB as measured by the installer probe.
pub const CLIENT_RAM_ENV: &str = "SUSUTAKU_CLIENT_RAM_GIB";

pub struct ClientNode {
    hub_addr: String,
    sandbox: Arc<AgentSandbox>,
    manager: Arc<ManagerProcess>,
}

impl ClientNode {
    pub fn new(hub_addr: &str, sandbox: Arc<AgentSandbox>) -> Self {
        Self {
            hub_addr: hub_addr.to_string(),
            sandbox,
            manager: ManagerProcess::new(),
        }
    }

    /// Connects and serves hub commands forever, reconnecting on failure.
    /// Refuses to run without the installer-provided metadata — the hub
    /// rejects registrations that carry no machine metadata.
    pub async fn run(&self) -> Result<(), String> {
        let meta = client_meta()?;
        loop {
            let node = NodeHandlers {
                sandbox: self.sandbox.clone(),
                manager: self.manager.clone(),
            };
            let agents = NodeHandlers {
                sandbox: self.sandbox.clone(),
                manager: self.manager.clone(),
            };
            let result = client::connect(
                &self.hub_addr,
                meta.clone(),
                move |agent, cmd| node.handle(agent, cmd),
                move || {
                    agents
                        .manager
                        .snapshot()
                        .into_iter()
                        .map(|a| AgentBrief {
                            name: a.agent,
                            runs: a.runs,
                            last_cmd: a.last_cmd,
                        })
                        .collect()
                },
            )
            .await;
            match result {
                Ok(()) => return Ok(()),
                Err(err) => {
                    eprintln!("client node {err}; retrying in {RECONNECT_SECS}s");
                    sleep(Duration::from_secs(RECONNECT_SECS)).await;
                }
            }
        }
    }
}

/// Command handlers shared by the client node loop.
#[derive(Clone)]
struct NodeHandlers {
    sandbox: Arc<AgentSandbox>,
    manager: Arc<ManagerProcess>,
}

impl NodeHandlers {
    /// Empty `agent` → process-global sandbox; otherwise the agent's own
    /// work tree (spawn is idempotent).
    fn handle(&self, agent: &str, cmd: &str) -> String {
        if agent.is_empty() {
            return self
                .sandbox
                .run(cmd)
                .unwrap_or_else(|e| format!("sandbox error: {e}"));
        }
        self.manager
            .spawn(agent)
            .and_then(|_| self.manager.run(agent, cmd))
            .unwrap_or_else(|e| format!("agent error: {e}"))
    }
}

fn hostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Machine metadata from the environment the installer sets up. Missing or
/// malformed values are a hard error: an un-describable machine must not
/// register.
fn client_meta() -> Result<ClientMeta, String> {
    let role = std::env::var(CLIENT_ROLE_ENV).unwrap_or_default();
    if role != ROLE_MODEL && role != ROLE_WORKER {
        return Err(format!(
            "missing or invalid {CLIENT_ROLE_ENV} (set it to model or worker, as decided by the installer probe)"
        ));
    }
    let ram_gib: u64 = std::env::var(CLIENT_RAM_ENV)
        .unwrap_or_default()
        .parse()
        .map_err(|_| format!("missing or invalid {CLIENT_RAM_ENV} (installed RAM in GiB, measured by the installer)"))?;
    if ram_gib == 0 {
        return Err(format!("invalid {CLIENT_RAM_ENV}: must be > 0"));
    }
    Ok(ClientMeta {
        hostname: hostname(),
        os: OS_NAME.to_string(),
        arch: ARCH_NAME.to_string(),
        role,
        ram_gib,
    })
}
