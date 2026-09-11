use std::fs;
use std::path::Path;

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

const COPY_DIR_COPY_ERR: &str = "snapshot copy failed";

/// Recursively copy `src` into `dst`, skipping `skip_name` entries at any
/// depth (used when dst lives inside src).
pub(crate) fn copy_dir_recursive_skip(
    src: &Path,
    dst: &Path,
    skip_name: &str,
) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy() == skip_name {
            continue;
        }
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive_skip(&entry.path(), &to, skip_name)?;
        } else {
            fs::copy(entry.path(), &to).map_err(|e| {
                std::io::Error::other(format!(
                    "{COPY_DIR_COPY_ERR}: {}: {e}",
                    entry.path().display()
                ))
            })?;
        }
    }
    Ok(())
}
