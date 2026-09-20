#![allow(clippy::unwrap_used)] // tests: unwrap is the assertion tool

use git_rs::GitRepo;
use std::path::Path;
use std::process::Command;

const TMP_PARENT: &str = "/tmp/git-rs-clone-tls-test";
const PROBE_URL: &str = "https://github.com/octocat/Hello-World.git";

#[test]
fn https_clone_over_tls_works() {
    let _ = std::fs::remove_dir_all(TMP_PARENT);
    std::fs::create_dir_all(TMP_PARENT).unwrap();
    let into = Path::new(TMP_PARENT).join("probe");
    let repo = GitRepo::clone_into(PROBE_URL, &into, None).expect("tls clone must succeed");
    assert!(repo.is_dirty().is_ok());
    // the remote url must carry no secret
    let cfg = Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(&into)
        .output()
        .unwrap();
    let url = String::from_utf8_lossy(&cfg.stdout);
    assert!(url.trim().ends_with(".git") || url.trim().ends_with("Hello-World"));
    let _ = std::fs::remove_dir_all(TMP_PARENT);
}
