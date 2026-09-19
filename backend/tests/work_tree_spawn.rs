//! Live work-tree spawn test (needs podman): a card run must tear down a
//! stale slot seeded without a repo and re-spawn from the bound repo, so the
//! task branch exists and publish is possible.

use backend::app::card_run::{WorkTree, slot_key};
use manager_rs::manager::{Manager, RemoteRepo};
use std::path::PathBuf;
use std::process::Command;

const AGENT: &str = "wt-spawn-test-agent";
const TASK: i64 = 9001;
const BRANCH_PREFIX: &str = "task/";

fn git(dir: &std::path::Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A seed repo with one commit (branch creation needs commits).
fn seed_repo(tmp: &std::path::Path) -> PathBuf {
    let seed = tmp.join("seed");
    std::fs::create_dir_all(&seed).unwrap();
    git(&seed, &["init", "-q", "-b", "main"]);
    std::fs::write(seed.join("README.md"), "seed\n").unwrap();
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-q", "-m", "init"]);
    seed
}

#[tokio::test]
async fn stale_slot_without_branch_is_reseeded_from_repo() {
    let tmp = std::env::temp_dir().join(format!("wt-spawn-test-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let seed = seed_repo(&tmp);
    let repo = RemoteRepo {
        url: seed.display().to_string(),
        token: None,
    };
    let slot = slot_key(AGENT, TASK);

    // The bug: slot first spawned without a repo — empty init tree, no
    // commits, therefore no task branch, and publish would fail forever.
    let manager = Manager::new();
    manager.spawn_task(AGENT, &TASK.to_string()).unwrap();
    assert!(manager.snapshot().iter().any(|a| a.agent == slot));

    let wt = WorkTree::new(manager.clone());
    let tree = wt
        .spawn_task(AGENT, &slot, &TASK.to_string(), repo, None)
        .await
        .unwrap();
    assert!(tree.exists());

    // The stale tree is gone; the fresh one is on the task branch.
    let branch = wt
        .run(&slot, "git rev-parse --abbrev-ref HEAD")
        .await
        .unwrap();
    let branch = branch.trim();
    assert!(
        branch.starts_with(BRANCH_PREFIX),
        "expected task branch, on {branch:?}"
    );

    wt.finish(&slot).await.unwrap();
    let _ = std::fs::remove_dir_all(&tmp);
}
