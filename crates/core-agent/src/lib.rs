pub mod agent_state;
pub mod sandbox_abstract_layer;
pub mod sandbox_jail;
pub mod toolcall;

pub use sandbox_abstract_layer::{Guarantee, SandboxLayer};

pub fn run_in_workspace(cmd: &str) -> Result<String, std::io::Error> {
    sandbox_jail::run_in_sandbox(cmd)
}
