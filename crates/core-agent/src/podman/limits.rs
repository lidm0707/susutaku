//! Sandbox policy types: network choice and per-run resource limits.

use std::time::Duration;

pub(crate) const MAX_OUTPUT_BYTES: usize = 1 << 20;
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_MAX_PROCESSES: u64 = 64;

#[derive(Debug, Clone)]
pub enum NetworkPolicy {
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetworkPolicyChoice {
    #[default]
    Disabled,
    Enabled,
}

impl From<NetworkPolicyChoice> for NetworkPolicy {
    fn from(c: NetworkPolicyChoice) -> Self {
        match c {
            NetworkPolicyChoice::Disabled => NetworkPolicy::Disabled,
            NetworkPolicyChoice::Enabled => NetworkPolicy::Enabled,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SandboxLimits {
    pub timeout: Duration,
    pub max_output_bytes: usize,
    pub max_processes: u64,
    pub max_memory_bytes: Option<u64>,
    pub max_cpu_seconds: Option<u64>,
    pub max_file_size_bytes: Option<u64>,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            max_output_bytes: MAX_OUTPUT_BYTES,
            max_processes: DEFAULT_MAX_PROCESSES,
            max_memory_bytes: None,
            max_cpu_seconds: None,
            max_file_size_bytes: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub limits: SandboxLimits,
    pub network: NetworkPolicyChoice,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            limits: SandboxLimits::default(),
            network: NetworkPolicyChoice::Disabled,
        }
    }
}
