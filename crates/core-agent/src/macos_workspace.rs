use std::io::Error;
use std::process::Command;

pub const SHELL: &str = "/bin/zsh";
pub const RUN_FLAG: &str = "-c";
pub const MAX_OUTPUT_BYTES: usize = 1 << 20;

pub fn run(cmd: &str) -> Result<String, Error> {
    let out = Command::new(SHELL).arg(RUN_FLAG).arg(cmd).output()?;
    if !out.status.success() {
        return Err(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ));
    }
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.truncate(MAX_OUTPUT_BYTES);
    Ok(text)
}
