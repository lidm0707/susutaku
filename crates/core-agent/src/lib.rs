pub mod agent_state;
pub mod sandbox;
pub mod toolcall;

pub fn run_in_workspace(cmd: &str) -> Result<String, std::io::Error> {
    sandbox::run_in_sandbox(cmd)
}
