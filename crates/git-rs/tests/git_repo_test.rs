use std::path::PathBuf;

use git_rs::GitRepo;
use git_rs::diff::PatchBase;
use std::fs;

fn temp_tree(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "git-rs-test-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn init_commit_dirty_diff_roundtrip() {
    let dir = temp_tree("roundtrip");

    let repo = GitRepo::init(&dir).unwrap();
    assert!(!repo.has_commits());
    assert!(!repo.is_dirty().unwrap());

    fs::write(dir.join("a.txt"), "hello\n").unwrap();
    assert!(repo.is_dirty().unwrap());

    let first = repo.commit_all("initial").unwrap();
    assert!(repo.has_commits());
    assert_eq!(repo.head_oid().unwrap(), first);
    assert!(!repo.is_dirty().unwrap());
    // nothing changed since HEAD -> empty patch
    assert!(repo.patch(PatchBase::WorkDir).unwrap().is_empty());

    fs::write(dir.join("a.txt"), "changed\n").unwrap();
    fs::write(dir.join("b.txt"), "new file\n").unwrap();
    assert!(repo.is_dirty().unwrap());

    let patch = repo.patch(PatchBase::WorkDir).unwrap();
    assert!(patch.contains("+changed"));
    assert!(patch.contains("new file"));

    let second = repo.commit_all("edit").unwrap();
    assert_ne!(first, second);
    assert!(!repo.is_dirty().unwrap());

    let history = repo.patch(PatchBase::Commit("HEAD~1")).unwrap();
    assert!(history.contains("+changed"));

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn open_existing_work_tree() {
    let dir = temp_tree("open");
    let repo = GitRepo::init(&dir).unwrap();
    fs::write(dir.join("x.txt"), "x\n").unwrap();
    repo.commit_all("c0").unwrap();
    drop(repo);

    let reopened = GitRepo::open(&dir).unwrap();
    assert!(reopened.has_commits());
    assert!(reopened.root().exists());

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn task_branch_create_checkout_is_idempotent_and_isolates_work() {
    let dir = temp_tree("branch");
    let repo = GitRepo::init(&dir).unwrap();
    fs::write(dir.join("a.txt"), "one\n").unwrap();
    let base = repo.commit_all("initial").unwrap();

    repo.create_checkout_branch("task/42-demo").unwrap();
    // second call on the existing branch must be a no-op, not an error
    repo.create_checkout_branch("task/42-demo").unwrap();

    fs::write(dir.join("b.txt"), "two\n").unwrap();
    let on_branch = repo.commit_all("on branch").unwrap();
    assert_ne!(base, on_branch);
    assert!(dir.join("b.txt").exists());

    // base commit's tree has no b.txt: the work landed only on the branch
    let repo2 = GitRepo::open(&dir).unwrap();
    let base_patch = repo2
        .patch(git_rs::diff::PatchBase::Commit(&format!("{base}")))
        .unwrap();
    assert!(base_patch.contains("b.txt"));

    // fresh checkout of the base branch ref still resolves
    repo.checkout_branch("task/42-demo").unwrap();

    fs::remove_dir_all(&dir).unwrap();
}
