pub mod agent_state;
pub mod sandbox;
pub mod sandbox_abstract_layer;
pub mod toolcall;

pub use sandbox_abstract_layer::{Guarantee, SandboxLayer};

pub fn run_in_workspace(cmd: &str) -> Result<String, std::io::Error> {
    sandbox::run_in_sandbox(cmd)
}
