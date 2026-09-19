//! Installer generation for remote runtime clients. Every machine installs
//! the same runtime (sandbox client + worker); inference comes from the
//! server's model endpoint, so there is no model-host role and no capability
//! probe beyond the metadata the hub requires.

use serde::Deserialize;

pub const SERVER_QUERY_FIELD: &str = "server";
pub const GIB: usize = 1024 * 1024 * 1024;

/// Per-OS programs the runtime client depends on: the sandbox shell
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
pub const DEFAULT_HTTP_PORT: u16 = 8992;
pub const DEFAULT_TCP_PORT: u16 = 8993;
pub const SERVER_URL_ENV: &str = "SUSUTAKU_LOCAL_MODEL_URL";
pub const HUB_ADDR_ENV: &str = "SUSUTAKU_HUB_ADDR";
/// Client registration metadata (see `proto_rs::ClientMeta`): the hub refuses
/// machines that register without this.
pub const CLIENT_RAM_ENV: &str = "SUSUTAKU_CLIENT_RAM_GIB";

pub fn render_install_script(server_url: &str) -> String {
    format!(
        r#"#!/bin/sh
# susutaku runtime installer — server={server_url}
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

# runtime dependency check: shell + probe tools per os
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
    echo "missing runtime dependencies:$missing" >&2
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

if [ "$os" = windows ]; then
    echo "windows runtime client is not supported yet; register manually later" >&2
    exit 1
fi

echo "machine probe: $os/$arch, ${{ram_gib}} GiB RAM"
echo "server: {server_url}"

# prepare: rust toolchain (the binaries are pure rust; no other runtime deps)
if ! command -v cargo >/dev/null 2>&1; then
    echo "installing rust toolchain…"
    curl --proto '=https' --tlsv1.2 -sSf {RUSTUP_URL} | sh -s -- -y --default-toolchain stable
    . "$HOME/.cargo/env"
fi

# prepare: source + release build
mkdir -p {INSTALL_ROOT}
if [ ! -d {INSTALL_ROOT}/src ]; then
    git clone --depth 1 {REPO_URL} {INSTALL_ROOT}/src
fi
cd {INSTALL_ROOT}/src
git pull --ff-only || echo "keeping existing checkout"
cargo build --release -p backend

# launch: the runtime client registers with the server's hub and runs
# dispatched agent commands in local per-agent sandboxes; inference uses the
# server's model endpoint.
server_host="$(printf '%s\n' '{server_url}' | sed -E 's#^https?://##; s#[:/].*$##')"
export {SERVER_URL_ENV}="http://$server_host:{DEFAULT_HTTP_PORT}"
export {HUB_ADDR_ENV}="$server_host:{DEFAULT_TCP_PORT}"
export {CLIENT_RAM_ENV}="$ram_gib"
nohup ./target/release/backend >{INSTALL_ROOT}/backend.log 2>&1 &
echo "runtime started (pid $!), registers with hub $server_host:{DEFAULT_TCP_PORT}"
echo "logs: {INSTALL_ROOT}/backend.log"
"#
    )
}

#[derive(Debug, Deserialize)]
pub struct InstallQuery {
    #[serde(default, rename = "server")]
    pub server: Option<String>,
}
