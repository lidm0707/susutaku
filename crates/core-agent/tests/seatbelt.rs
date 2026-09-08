//! Seatbelt (sandbox-exec) integration tests for the macOS sandbox.

#![cfg(target_os = "macos")]

use core_agent::sandbox::macos::{
    ExecutionMode, Sandbox, execution_mode, seatbelt_command, seatbelt_profile,
};
use core_agent::sandbox_abstract_layer::{Guarantee, SandboxLayer};
use std::path::Path;

const SB_ROOT: &str = "/tmp/susutaku-seatbelt-test-root";
const ECHO_CMD: &str = "echo hi";
const WRITE_INSIDE_CMD: &str = "echo ok > inside.txt && cat inside.txt";
const WRITE_OUTSIDE_CMD: &str =
    "echo blocked > /tmp/susutaku-seatbelt-probe && cat /tmp/susutaku-seatbelt-probe";

#[test]
fn profile_restricts_writes_to_root() {
    let profile = seatbelt_profile(Path::new(SB_ROOT));
    assert!(profile.contains("(deny file-write*)"));
    assert!(profile.contains(&format!("(allow file-write* (subpath \"{SB_ROOT}\"))")));
}

#[test]
fn seatbelt_command_shape() {
    let cmd = seatbelt_command(
        ExecutionMode::Seatbelt,
        Path::new(SB_ROOT),
        Path::new(SB_ROOT),
        ECHO_CMD,
    );
    assert_eq!(cmd.get_program().to_string_lossy(), "/usr/bin/sandbox-exec");
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args[0], "-p");
    assert!(args.windows(3).any(|w| w[1] == "/bin/zsh" && w[2] == "-c"));
}

#[test]
fn run_inside_seatbelt_sandbox() {
    let sandbox = Sandbox::new().unwrap();
    assert_eq!(sandbox.run(ECHO_CMD).unwrap().trim(), "hi");

    // Writes inside the sandbox root succeed.
    assert_eq!(sandbox.run(WRITE_INSIDE_CMD).unwrap().trim(), "ok");

    // Writes outside the root are denied by seatbelt (kernel guarantee only).
    if execution_mode() == ExecutionMode::Seatbelt {
        assert_eq!(sandbox.guarantee(), Guarantee::Kernel);
        assert!(sandbox.run(WRITE_OUTSIDE_CMD).is_err());
    } else {
        assert_eq!(sandbox.guarantee(), Guarantee::BestEffort);
    }
    sandbox.purge();
}
