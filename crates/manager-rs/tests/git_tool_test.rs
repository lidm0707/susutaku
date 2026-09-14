//! Git toolcall (clone/status/diff) against a plain workspace dir — pure
//! git-rs, no podman. The Manager spawn path is covered by sandbox tests.

use std::fs;

use git_rs::GitRepo;
use manager_rs::git_tool;
use proto_rs::GitTool;

fn tmp_dir(label: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("susutaku-git-tool-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

#[test]
fn git_tool_clone_status_diff() {
    // Source repo with one commit.
    let src = tmp_dir("src");
    let repo = GitRepo::open_or_init(&src).expect("init");
    fs::write(src.join("README.md"), "hello\n").expect("seed file");
    repo.commit_all("seed").expect("commit");

    // Fresh workspace still holds spawn layout entries (.git from an empty
    // init, snapshots dir) — clone must accept and replace that.
    let ws = tmp_dir("ws");
    fs::create_dir_all(ws.join("snapshots")).expect("layout");
    GitRepo::open_or_init(&ws).expect("empty init");

    let tool_clone = GitTool::Clone {
        url: src.display().to_string(),
        token: None,
    };
    let out = git_tool::apply(&ws, &tool_clone).expect("clone");
    assert!(out.contains("cloned"), "unexpected clone output: {out}");
    assert!(
        ws.join("README.md").exists(),
        "seed file missing after clone"
    );

    // Clone into a populated tree is refused.
    assert!(git_tool::apply(&ws, &tool_clone).is_err());

    let out = git_tool::apply(&ws, &GitTool::Status).expect("status");
    assert!(out.contains("clean"), "unexpected status: {out}");
    assert!(
        !out.contains("no commits"),
        "clone should carry the seed commit"
    );

    let out = git_tool::apply(&ws, &GitTool::Diff).expect("diff");
    assert_eq!(out, "(no changes)");

    let _ = fs::remove_dir_all(&src);
    let _ = fs::remove_dir_all(&ws);
}
