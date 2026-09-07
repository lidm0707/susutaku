#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub use linux::{Sandbox, SandboxDir, list_dirs, purge_dir, run};
#[cfg(target_os = "macos")]
pub use macos::{Sandbox, SandboxDir, list_dirs, purge_dir, run};
#[cfg(target_os = "windows")]
pub use windows::{Sandbox, SandboxDir, list_dirs, purge_dir, run};

pub fn run_in_sandbox(cmd: &str) -> Result<String, std::io::Error> {
    run(cmd)
}
