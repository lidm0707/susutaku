pub mod agent_state;
#[cfg(target_os = "linux")]
pub mod linux_workspace;
#[cfg(target_os = "macos")]
pub mod macos_workspace;
pub mod toolcall;

pub fn run_in_workspace(cmd: &str) -> Result<String, std::io::Error> {
    #[cfg(target_os = "macos")]
    {
        macos_workspace::run(cmd)
    }
    #[cfg(target_os = "linux")]
    {
        linux_workspace::run(cmd)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = cmd;
        unimplemented!()
    }
}
