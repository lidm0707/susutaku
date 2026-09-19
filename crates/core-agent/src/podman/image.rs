//! Sandbox image selection and the per-agent image cache.
//!
//! Each spawned agent runs from an image (enum [`AgentImage`]; coding agents
//! get the toolchain image built by `make sandbox-image`). After every run
//! the container is committed to a per-agent cache tag on the host podman
//! storage, so toolchains/packages installed by the agent survive its
//! teardown and the next spawn of the same agent starts from that image.

use std::io::Error;
use std::process::{Command, Stdio};

const PODMAN_BIN: &str = "podman";
const CODING_IMAGE: &str = "localhost/susutaku-sandbox:latest";
const CODING_IMAGE_ENV: &str = "SUSUTAKU_SANDBOX_IMAGE";
const CACHE_REPO: &str = "localhost/susutaku-agent-cache";
const COMMIT_MSG: &str = "susutaku agent image cache";

/// Image an agent sandbox runs from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentImage {
    /// Default coding image (toolchain baked in via `make sandbox-image`).
    Coding,
    /// Explicit image reference.
    Custom(String),
}

impl AgentImage {
    pub fn reference(&self) -> String {
        match self {
            Self::Coding => {
                std::env::var(CODING_IMAGE_ENV).unwrap_or_else(|_| CODING_IMAGE.to_owned())
            }
            Self::Custom(image) => image.clone(),
        }
    }
}

/// Host-side cache tag for `agent` (sanitized into a valid tag).
pub fn cached_tag(agent: &str) -> String {
    let tag: String = agent
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{CACHE_REPO}:{tag}")
}

/// Whether an image reference already exists in host podman storage.
pub fn exists(reference: &str) -> bool {
    Command::new(PODMAN_BIN)
        .args(["image", "exists", reference])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Pull `reference` from its registry when it is not in host podman storage.
pub fn ensure(reference: &str) -> Result<(), String> {
    if exists(reference) {
        return Ok(());
    }
    let status = Command::new(PODMAN_BIN)
        .args(["pull", reference])
        .status()
        .map_err(|e| format!("podman pull {reference}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("podman pull {reference} failed"))
    }
}

/// The image a fresh sandbox for `agent` should run from: the cached image
/// when present, otherwise the profile default.
pub fn resolve(agent: &str) -> String {
    let cached = cached_tag(agent);
    if exists(&cached) {
        return cached;
    }
    AgentImage::Coding.reference()
}

/// Commit container `name` into `tag`, then remove it. Best-effort removal:
/// a failed commit is reported, a failed cleanup is not.
pub(crate) fn commit_and_remove(name: &str, tag: &str) -> Result<(), Error> {
    let status = Command::new(PODMAN_BIN)
        .args(["commit", "-m", COMMIT_MSG, name, tag])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(Error::other(format!(
            "podman commit {name} -> {tag} failed"
        )));
    }
    let _ = Command::new(PODMAN_BIN)
        .args(["rm", "-f", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}
