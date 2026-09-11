//! Installer generation for remote sandbox clients. The script probes the
//! target machine (arch/RAM) before installing and refuses `model` role on
//! machines that cannot hold a local MLX model.

use serde::Deserialize;

pub const ROLE_QUERY_FIELD: &str = "role";
pub const SERVER_QUERY_FIELD: &str = "server";
/// Minimum installed RAM (GiB) for a machine to register as a local-model
/// host: a ≥30 GiB q4 checkpoint needs headroom beyond its own size.
pub const MODEL_MIN_RAM_GIB: usize = 32;
pub const GIB: usize = 1024 * 1024 * 1024;

/// Per-OS programs the sandbox client depends on: the sandbox shell
/// (see `core-agent/src/sandbox/{macos,linux,windows}.rs`) and the
/// probe tools used by this installer.
pub const SANDBOX_SHELL_MACOS: &str = "/bin/zsh";
pub const SANDBOX_SHELL_LINUX: &str = "/bin/bash";
pub const SANDBOX_SHELL_WINDOWS: &str = "powershell";
pub const PROBE_TOOL_MACOS: &str = "sysctl";
pub const PROBE_TOOL_LINUX: &str = "awk";
pub const REPO_URL: &str = "https://github.com/lidm0707/susutaku.git";
pub const INSTALL_ROOT: &str = "$HOME/.susutaku";
pub const RUSTUP_URL: &str = "https://sh.rustup.rs";
pub const HTTP_PORT_ENV: &str = "LOCAL_MODEL_HTTP_PORT";
pub const TCP_PORT_ENV: &str = "LOCAL_MODEL_TCP_PORT";
pub const DEFAULT_HTTP_PORT: u16 = 8992;
pub const DEFAULT_TCP_PORT: u16 = 8993;
pub const SERVER_URL_ENV: &str = "SUSUTAKU_LOCAL_MODEL_URL";
pub const HUB_ADDR_ENV: &str = "SUSUTAKU_HUB_ADDR";
/// Client registration metadata (see `proto_rs::ClientMeta`): the hub refuses
/// machines that register without these.
pub const CLIENT_ROLE_ENV: &str = "SUSUTAKU_CLIENT_ROLE";
pub const CLIENT_RAM_ENV: &str = "SUSUTAKU_CLIENT_RAM_GIB";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Role {
    /// Probe decides: model+worker when capable, else worker-only.
    #[default]
    Auto,
    /// Force local-model host; install fails when the probe disagrees.
    Model,
    /// Provider jobs only; skip model checks.
    Worker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Model,
    WorkerOnly,
}

pub fn parse_role(raw: &str) -> Option<Role> {
    match raw {
        "auto" => Some(Role::Auto),
        "model" => Some(Role::Model),
        "worker" => Some(Role::Worker),
        _ => None,
    }
}

/// A machine can host a local model when it is Apple Silicon (MLX target)
/// with at least `MODEL_MIN_RAM_GIB` installed.
pub fn classify(os: &str, arch: &str, installed_bytes: usize) -> Capability {
    let apple_silicon = os == "macos" && arch == "aarch64";
    if apple_silicon && installed_bytes >= MODEL_MIN_RAM_GIB * GIB {
        Capability::Model
    } else {
        Capability::WorkerOnly
    }
}

pub fn render_install_script(role: Role, server_url: &str) -> String {
    let role = match role {
        Role::Auto => "auto",
        Role::Model => "model",
        Role::Worker => "worker",
    };
    format!(
        r#"#!/bin/sh
# susutaku sandbox client installer — role={role}, server={server_url}
set -eu

OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
    Darwin) os=macos ;;
    Linux) os=linux ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT) os=windows ;;
    *) echo "unsupported os: $OS" >&2; exit 1 ;;
esac
case "$ARCH" in
    arm64|aarch64) arch=aarch64 ;;
    x86_64|amd64) arch=x86_64 ;;
    *) echo "unsupported arch: $ARCH" >&2; exit 1 ;;
esac

# sandbox dependency check: shell + probe tools per os
missing=""
case "$os" in
    macos)
        [ -x "{SANDBOX_SHELL_MACOS}" ] || missing="$missing {SANDBOX_SHELL_MACOS}"
        command -v {PROBE_TOOL_MACOS} >/dev/null 2>&1 || missing="$missing {PROBE_TOOL_MACOS}"
        ;;
    linux)
        [ -x "{SANDBOX_SHELL_LINUX}" ] || missing="$missing {SANDBOX_SHELL_LINUX}"
        command -v {PROBE_TOOL_LINUX} >/dev/null 2>&1 || missing="$missing {PROBE_TOOL_LINUX}"
        ;;
    windows)
        command -v {SANDBOX_SHELL_WINDOWS} >/dev/null 2>&1 || missing="$missing {SANDBOX_SHELL_WINDOWS}"
        ;;
esac
if [ -n "$missing" ]; then
    echo "missing sandbox dependencies:$missing" >&2
    exit 1
fi

ram_gib=0
if [ "$os" = macos ]; then
    ram_bytes="$(sysctl -n hw.memsize 2>/dev/null || echo 0)"
elif [ "$os" = linux ]; then
    ram_kb="$(awk '/MemTotal/ {{print $2}}' /proc/meminfo 2>/dev/null || echo 0)"
    ram_bytes=$((ram_kb * 1024))
fi
ram_gib=$((ram_bytes / {GIB}))

capable=no
if [ "$os" = macos ] && [ "$arch" = aarch64 ] && [ "$ram_gib" -ge {MODEL_MIN_RAM_GIB} ]; then
    capable=yes
fi

role="{role}"
if [ "$role" = auto ]; then
    if [ "$capable" = yes ]; then role=model; else role=worker; fi
fi
if [ "$role" = model ] && [ "$capable" != yes ]; then
    echo "this machine cannot host a local model" \
         "(needs macos + aarch64 + >= {MODEL_MIN_RAM_GIB} GiB RAM;" \
         "found $os/$arch, ${{ram_gib}} GiB)" >&2
    exit 1
fi
if [ "$os" = windows ]; then
    echo "windows sandbox client is not supported yet; register manually later" >&2
    exit 1
fi

echo "machine probe: $os/$arch, ${{ram_gib}} GiB RAM"
echo "role: $role ($([ "$role" = model ] && echo model+worker || echo provider jobs only))"
echo "server: {server_url}"

# prepare: rust toolchain (the binaries are pure rust; no other runtime deps)
if ! command -v cargo >/dev/null 2>&1; then
    echo "installing rust toolchain…"
    curl --proto '=https' --tlsv1.2 -sSf {RUSTUP_URL} | sh -s -- -y --default-toolchain stable
    . "$HOME/.cargo/env"
fi

# prepare: source + release build (one checkout builds both binaries)
mkdir -p {INSTALL_ROOT}
if [ ! -d {INSTALL_ROOT}/src ]; then
    git clone --depth 1 {REPO_URL} {INSTALL_ROOT}/src
fi
cd {INSTALL_ROOT}/src
git pull --ff-only || echo "keeping existing checkout"
cargo build --release -p local-model -p backend

# launch: model hosts run the model server (inference + command hub);
# workers run the backend client node, which registers with the hub.
server_host="$(printf '%s\n' '{server_url}' | sed -E 's#^https?://##; s#[:/].*$##')"
if [ "$role" = model ]; then
    export {HTTP_PORT_ENV}={DEFAULT_HTTP_PORT} {TCP_PORT_ENV}={DEFAULT_TCP_PORT}
    echo "model host: fetch a checkpoint first if none is under models/ (see Makefile download-model)"
    nohup ./target/release/local-model >{INSTALL_ROOT}/local-model.log 2>&1 &
    echo "local-model started (pid $!), logs: {INSTALL_ROOT}/local-model.log"
else
    export {SERVER_URL_ENV}="http://$server_host:{DEFAULT_HTTP_PORT}"
    export {HUB_ADDR_ENV}="$server_host:{DEFAULT_TCP_PORT}"
    export {CLIENT_ROLE_ENV}="$role"
    export {CLIENT_RAM_ENV}="$ram_gib"
    nohup ./target/release/backend >{INSTALL_ROOT}/backend.log 2>&1 &
    echo "backend started (pid $!), registers with hub $server_host:{DEFAULT_TCP_PORT}"
    echo "logs: {INSTALL_ROOT}/backend.log"
fi
"#
    )
}

#[derive(Debug, Deserialize)]
pub struct InstallQuery {
    #[serde(default, rename = "role")]
    pub role: Option<String>,
    #[serde(default, rename = "server")]
    pub server: Option<String>,
}
