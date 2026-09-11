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

#[test]
fn slot_lookup_rejects_every_operation_for_unknown_agent() {
    let manager = ManagerProcess::new();
    assert!(manager.push_context("ghost", "hello").is_err());
    assert!(manager.logs("ghost").is_err());
    assert!(manager.snapshot().is_empty());
}

#[test]
fn sanitize_maps_special_characters_in_work_tree_name() {
    let manager = ManagerProcess::new();
    let work_tree = manager.spawn("bad name/x.y").expect("spawn");
    assert_eq!(
        work_tree,
        std::path::Path::new(AGENTS_ROOT).join("bad_name_x_y")
    );
    assert!(work_tree.exists());
    manager.finish("bad name/x.y").expect("finish");
    assert!(!work_tree.exists(), "work tree purged");
}

#[test]
fn logs_reports_runs_transcript_and_last_result() {
    let manager = ManagerProcess::new();
    manager.spawn("logger").expect("spawn");
    manager.push_context("logger", "task notes").expect("push");
    manager.run("logger", "echo hi").expect("run");

    let logs = manager.logs("logger").expect("logs");
    assert_eq!(logs.agent, "logger");
    assert_eq!(
        logs.work_tree,
        std::path::Path::new(AGENTS_ROOT).join("logger")
    );
    assert_eq!(logs.runs, 1);
    assert!(logs.transcript.len() >= 2);
    assert!(logs.transcript[0].contains("task notes"));
    assert!(logs.transcript[1].contains("echo hi"));
    assert_eq!(logs.last_result.as_deref(), Some("hi\n"));

    let agents = manager.snapshot();
    assert_eq!(agents[0].runs, 1);
    manager.finish("logger").expect("finish");
}

#[test]
fn finish_without_runs_returns_empty_transcript_and_no_result() {
    let manager = ManagerProcess::new();
    manager.spawn("quiet").expect("spawn");

    let outcome = manager.finish("quiet").expect("finish");
    assert_eq!(outcome.agent, "quiet");
    assert_eq!(outcome.result, None);
    assert!(outcome.state.history.is_empty());
    assert_eq!(outcome.state.cwd, ".");
    assert!(manager.logs("quiet").is_err());
}
