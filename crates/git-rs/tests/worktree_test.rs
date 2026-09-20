#![allow(clippy::unwrap_used)] // tests: unwrap is the assertion tool

use std::fs;
use std::path::{Path, PathBuf};

use git_rs::GitRepo;

fn temp_tree(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "git-rs-wt-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Seeds a normal repo with one commit and returns it + its dir.
/// Per-test root with the seeded repo at `root/base`; the worktree lives
/// in `root/wt-*`. Cleanup is always `remove_dir_all(root)`.
fn seed_repo(tag: &str) -> (PathBuf, GitRepo) {
    let root = temp_tree(tag);
    let dir = root.join("base");
    fs::create_dir_all(&dir).unwrap();
    let repo = GitRepo::init(&dir).unwrap();
    fs::write(dir.join("seed.txt"), "seed\n").unwrap();
    repo.commit_all("seed").unwrap();
    (root, repo)
}

fn cleanup(root: &Path) {
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn worktree_add_same_basename_slots_do_not_collide() {
    let (base_dir, base) = seed_repo("collide");
    // Real slot layout: every agent work tree ends in sandbox/workspace,
    // so the admin name must not be the bare file name.
    let wt_a = base_dir.join("agents/a/sandbox/workspace");
    let wt_b = base_dir.join("agents/b/sandbox/workspace");

    base.worktree_add(&wt_a, Some("task-a")).unwrap();
    base.worktree_add(&wt_b, Some("task-b")).unwrap();

    // Both worktrees resolve independently — git status on each.
    let a = GitRepo::open(&wt_a).unwrap();
    let b = GitRepo::open(&wt_b).unwrap();
    assert_eq!(a.current_branch().unwrap(), "task-a");
    assert_eq!(b.current_branch().unwrap(), "task-b");
    assert!(!a.is_dirty().unwrap());
    assert!(!b.is_dirty().unwrap());
    // Distinct admin dirs, one per slot.
    let admin = base_dir.join("base/.git/worktrees");
    let names: Vec<String> = fs::read_dir(&admin)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "admin dirs: {names:?}");

    cleanup(&base_dir);
}

#[test]
fn worktree_add_creates_branch_and_checkout() {
    let (base_dir, base) = seed_repo("add");
    let wt_dir = base_dir.join("wt-slot");

    base.worktree_add(&wt_dir, Some("task-1-agent")).unwrap();

    let wt = GitRepo::open(&wt_dir).unwrap();
    assert!(wt.has_commits());
    assert_eq!(wt.current_branch().unwrap(), "task-1-agent");
    assert_eq!(
        fs::read_to_string(wt_dir.join("seed.txt")).unwrap(),
        "seed\n"
    );
    assert!(!wt.is_dirty().unwrap());

    // the base repo tracks the linked worktree
    assert!(
        base.worktree_names()
            .unwrap()
            .iter()
            .any(|n| n.contains("wt-slot")),
    );

    cleanup(&base_dir);
}

#[test]
fn worktree_add_detached_when_no_branch() {
    let (base_dir, base) = seed_repo("detached");
    let wt_dir = base_dir.join("wt-detached");

    base.worktree_add(&wt_dir, None).unwrap();

    let wt = GitRepo::open(&wt_dir).unwrap();
    assert_eq!(wt.head_oid().unwrap(), base.head_oid().unwrap());
    assert_eq!(
        wt.current_branch().unwrap(),
        "HEAD",
        "detached HEAD is not on a branch"
    );

    cleanup(&base_dir);
}

#[test]
fn worktree_add_recreates_stale_branch_at_current_head() {
    let (base_dir, base) = seed_repo("stale");
    // a branch from a previous (reclaimed) run pointing at the seed commit,
    // left dangling after its worktree dir was reclaimed without a prune
    let stale_wt = base_dir.join("wt-stale-old");
    base.worktree_add(&stale_wt, Some("task/2-agent")).unwrap();
    fs::remove_dir_all(&stale_wt).unwrap();

    // the task moved on: the stale branch must be recreated at fresh HEAD
    fs::write(base_dir.join("next.txt"), "next\n").unwrap();
    base.commit_all("advance").unwrap();

    let wt_dir = base_dir.join("wt-stale");
    base.worktree_prune().unwrap();
    base.worktree_add(&wt_dir, Some("task/2-agent")).unwrap();

    let wt = GitRepo::open(&wt_dir).unwrap();
    assert_eq!(wt.head_oid().unwrap(), base.head_oid().unwrap());

    cleanup(&base_dir);
}

#[test]
fn worktree_remove_refuses_dirty_then_forces() {
    let (base_dir, base) = seed_repo("dirty");
    let wt_dir = base_dir.join("wt-dirty");
    base.worktree_add(&wt_dir, Some("task/3-agent")).unwrap();

    fs::write(wt_dir.join("edit.txt"), "uncommitted\n").unwrap();

    // non-force refuses a dirty tree
    let err = base.worktree_remove(&wt_dir, false).unwrap_err();
    assert!(err.to_string().contains("dirty"), "got: {err}");
    assert!(wt_dir.exists());

    // force removes the dir and the admin metadata
    base.worktree_remove(&wt_dir, true).unwrap();
    assert!(!wt_dir.exists());
    assert!(
        !base
            .worktree_names()
            .unwrap()
            .iter()
            .any(|n| n.contains("wt-dirty"))
    );

    cleanup(&base_dir);
}

#[test]
fn worktree_prune_drops_metadata_of_deleted_dirs() {
    let (base_dir, base) = seed_repo("prune");
    let wt_dir = base_dir.join("wt-prune");
    base.worktree_add(&wt_dir, Some("task/4-agent")).unwrap();

    // the slot is reclaimed without removing worktree metadata first
    fs::remove_dir_all(&wt_dir).unwrap();
    assert!(
        base.worktree_names()
            .unwrap()
            .iter()
            .any(|n| n.contains("wt-prune")),
        "metadata still there before prune"
    );

    base.worktree_prune().unwrap();
    assert!(
        !base
            .worktree_names()
            .unwrap()
            .iter()
            .any(|n| n.contains("wt-prune"))
    );

    cleanup(&base_dir);
}

#[test]
fn fetch_updates_bare_cache_and_worktree_starts_from_head() {
    // seed + bare remote
    let root = temp_tree("fetch");
    let seed_dir = root.join("seed");
    let origin = root.join("origin.git");
    let cache_dir = root.join("cache.git");
    let wt_dir = root.join("wt-slot");

    let seed = GitRepo::init(&seed_dir).unwrap();
    fs::write(seed_dir.join("f.txt"), "one\n").unwrap();
    seed.commit_all("one").unwrap();
    GitRepo::init_bare(&origin).unwrap();
    seed.push_branch(&origin.display().to_string(), "main", None)
        .unwrap();

    // first cache generation: init bare + fetch
    let cache = GitRepo::init_bare(&cache_dir).unwrap();
    cache.fetch(&origin.display().to_string(), None).unwrap();
    if !cache.head_resolves() {
        let name = cache.first_local_branch().unwrap();
        cache.set_head_to_branch(&name).unwrap();
    }
    assert!(cache.has_commits());

    // remote advances; refresh the cache via fetch only
    fs::write(seed_dir.join("f.txt"), "two\n").unwrap();
    seed.commit_all("two").unwrap();
    seed.push_branch(&origin.display().to_string(), "main", None)
        .unwrap();
    cache.fetch(&origin.display().to_string(), None).unwrap();
    let head = GitRepo::open(&cache_dir).unwrap().head_oid().unwrap();
    assert_eq!(
        head,
        seed.head_oid().unwrap(),
        "fetch updated the cache head"
    );

    // a worktree spawned from the cache carries the latest content
    cache.worktree_add(&wt_dir, Some("task/5-agent")).unwrap();
    let wt = GitRepo::open(&wt_dir).unwrap();
    assert_eq!(
        fs::read_to_string(wt_dir.join("f.txt")).unwrap(),
        "two\n",
        "worktree checks out the fresh head"
    );
    assert_eq!(wt.current_branch().unwrap(), "task/5-agent");

    cleanup(&root);
}

#[test]
fn worktree_commits_flow_back_to_shared_objects() {
    let (base_dir, base) = seed_repo("share");
    let wt_dir = base_dir.join("wt-share");
    base.worktree_add(&wt_dir, Some("task/6-agent")).unwrap();

    // the agent commits in its worktree; the object lives in the shared store
    fs::write(wt_dir.join("work.txt"), "agent work\n").unwrap();
    let wt = GitRepo::open(&wt_dir).unwrap();
    let oid = wt.commit_all("task work").unwrap();

    // shared object store: the base repo sees the commit (patch across the
    // boundary resolves both sides without any object copy)
    let patch = base
        .patch_range(base.head_oid().unwrap(), oid)
        .expect("shared objects: base reads the worktree commit");
    assert!(patch.contains("agent work"), "patch: {patch}");

    cleanup(&base_dir);
}
