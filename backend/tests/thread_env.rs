use backend::infra::thread_env::ThreadEnvManager;
use backend::port::outbound::ThreadEnvs;

#[test]
fn same_thread_reuses_env_new_thread_gets_own() {
    let manager = ThreadEnvManager::new();
    let a1 = manager.runner_for("101", Some("zai")).expect("env");
    let a2 = manager.runner_for("101", Some("zai")).expect("env");
    let b = manager.runner_for("102", Some("zai")).expect("env");
    // same thread -> same work tree; different thread -> isolated work tree
    assert_eq!(a1.workspace_root(), a2.workspace_root());
    assert_ne!(a1.workspace_root(), b.workspace_root());
    // each env is rooted under work/thread-envs/<thread_id>/...
    assert!(a1.workspace_root().display().to_string().contains("101"));
    assert!(b.workspace_root().display().to_string().contains("102"));
}

#[test]
fn rejects_empty_thread_id() {
    let manager = ThreadEnvManager::new();
    assert!(manager.runner_for("///", Some("zai")).is_err());
}

#[test]
fn env_dir_survives_manager_recreation() {
    let root = {
        let manager = ThreadEnvManager::new();
        let runner = manager.runner_for("303", None).expect("env");
        runner.workspace_root()
    };
    assert!(root.exists(), "env directory should persist on disk");
    // a fresh manager reuses the same directory without wiping it
    let manager = ThreadEnvManager::new();
    let runner = manager.runner_for("303", None).expect("env");
    assert_eq!(runner.workspace_root(), root);
}
