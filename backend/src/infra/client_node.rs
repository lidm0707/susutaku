//! Client node: registers with the local-model hub and runs commands in the
//! agent sandbox.

use std::sync::Arc;
use std::time::Duration;

use proto_rs::client;
use tokio::time::sleep;

use crate::infra::sandbox::AgentSandbox;
use crate::port::outbound::Runner;

const OS_NAME: &str = std::env::consts::OS;
const RECONNECT_SECS: u64 = 3;

pub struct ClientNode {
    hub_addr: String,
    sandbox: Arc<AgentSandbox>,
}

impl ClientNode {
    pub fn new(hub_addr: &str, sandbox: Arc<AgentSandbox>) -> Self {
        Self {
            hub_addr: hub_addr.to_string(),
            sandbox,
        }
    }

    /// Connects and serves hub commands forever, reconnecting on failure.
    pub async fn run(&self) -> Result<(), String> {
        loop {
            let sandbox = self.sandbox.clone();
            let result = client::connect(
                &self.hub_addr,
                hostname(),
                OS_NAME.to_string(),
                move |cmd| {
                    sandbox
                        .run(cmd)
                        .unwrap_or_else(|e| format!("sandbox error: {e}"))
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

fn hostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_string())
}
