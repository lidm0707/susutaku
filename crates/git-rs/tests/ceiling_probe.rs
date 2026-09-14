use git_rs::GitRepo;
#[test]
fn open_does_not_climb_above_target() {
    // this test dir has no repo of its own but lives inside the outer
    // susutaku work tree; open() must refuse to discover it
    let probe = std::path::Path::new("target/ceiling-probe/no-repo-here");
    let _ = std::fs::remove_dir_all(probe);
    std::fs::create_dir_all(probe).unwrap();
    assert!(GitRepo::open(probe).is_err(), "open must not climb above the target dir");
    let repo = GitRepo::open_or_init(probe).unwrap();
    assert!(probe.join(".git").exists(), "init created the repo in place");
    drop(repo);
    std::fs::remove_dir_all(probe.parent().unwrap()).unwrap();
}
