use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub id: u64,
    pub kind: Kind,
}

/// Identity a client must present to register with a hub. Every field is
/// mandatory — the hub refuses registrations with missing metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMeta {
    pub hostname: String,
    pub os: String,
    pub arch: String,
    /// Role decided by the installer probe: `"model"` or `"worker"`.
    pub role: String,
    /// Installed RAM in GiB as measured by the installer probe.
    pub ram_gib: u64,
}

/// One agent a client machine holds: name plus what it is doing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBrief {
    pub name: String,
    pub runs: u64,
    /// Last command the agent ran, if any.
    pub last_cmd: Option<String>,
}

/// Role of a machine that hosts the local model server.
pub const ROLE_MODEL: &str = "model";
/// Role of a machine that only runs provider/sandbox jobs.
pub const ROLE_WORKER: &str = "worker";

impl ClientMeta {
    /// Registration is refused unless every field is present and the role is
    /// one of the installer-decided roles.
    pub fn is_valid(&self) -> bool {
        !self.hostname.is_empty()
            && !self.os.is_empty()
            && !self.arch.is_empty()
            && (self.role == ROLE_MODEL || self.role == ROLE_WORKER)
            && self.ram_gib > 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Kind {
    Register {
        hostname: String,
        os: String,
        arch: String,
        role: String,
        ram_gib: u64,
    },
    /// Run `cmd` in the client sandbox. Non-empty `agent` addresses the
    /// client's per-agent manager (own work tree + transcript); empty means
    /// the process-global sandbox.
    Command {
        cmd: String,
        agent: String,
    },
    Result {
        output: String,
    },
    /// Hub asks a client which agents it currently holds.
    AgentNames,
    /// Client reply: per-agent name/runs/last-command from its manager
    /// snapshot.
    AgentNamesResult {
        agents: Vec<AgentBrief>,
    },
    Heartbeat,
}
