use std::fs;

use manager_rs::{AGENTS_ROOT, ManagerProcess};

#[test]
fn stale_work_tree_is_reclaimed_on_spawn() {
    let stale = std::path::Path::new(AGENTS_ROOT).join("stale-agent");
    fs::create_dir_all(stale.join("leftover")).expect("create stale tree");
    fs::write(stale.join("leftover").join("junk.txt"), "old run").expect("junk");

    let manager = ManagerProcess::new();
    manager.spawn("stale-agent").expect("spawn over stale tree");

    assert!(!stale.join("leftover").exists(), "stale content wiped");
    manager.finish("stale-agent").expect("finish");
}

#[test]
fn spawn_run_finish_round_trip() {
    let manager = ManagerProcess::new();
    let work_tree = manager.spawn("tester-1").expect("spawn");
    assert!(work_tree.starts_with(AGENTS_ROOT));

    let out = manager.run("tester-1", "echo hello").expect("run");
    assert_eq!(out.trim(), "hello");

    let agents = manager.snapshot();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].agent, "tester-1");
    assert_eq!(agents[0].runs, 1);

    let outcome = manager.finish("tester-1").expect("finish");
    assert_eq!(outcome.result.as_deref(), Some("hello\n"));
    assert!(!outcome.state.history.is_empty(), "transcript returned");
    assert!(!work_tree.join("sandbox").exists(), "sandbox purged");

    assert!(manager.snapshot().is_empty(), "agent removed after finish");
    assert!(manager.run("tester-1", "echo x").is_err());
}

#[test]
fn spawn_is_idempotent_per_agent() {
    let manager = ManagerProcess::new();
    let a = manager.spawn("dup").expect("spawn");
    let b = manager.spawn("dup").expect("spawn again");
    assert_eq!(a, b, "same work tree reused");
    assert_eq!(manager.snapshot().len(), 1);
    manager.finish("dup").expect("finish");
}

#[test]
fn run_unknown_agent_fails() {
    let manager = ManagerProcess::new();
    assert!(manager.run("ghost", "echo x").is_err());
    assert!(manager.finish("ghost").is_err());
}
