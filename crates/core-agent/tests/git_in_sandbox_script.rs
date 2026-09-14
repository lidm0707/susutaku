use core_agent::toolcall::git_in_sandbox::{PR_BASE_DEFAULT, repo_slug, script_for};
use proto_rs::GitTool;

#[test]
fn commit_script_sets_identity_and_quotes_message() {
    let (script, token) = script_for(&GitTool::Commit {
        message: "fix: it's a 'quoted' msg".into(),
    })
    .unwrap();
    assert!(token.is_none());
    assert!(script.contains("git add -A"));
    assert!(script.contains("user.name='susutaku-agent'"));
    // single quotes are shell-escaped, never breaking out of the quoting
    assert!(script.contains(r"it'\''s"));
}

#[test]
fn push_requires_token_and_never_embeds_it_in_the_script() {
    let err = script_for(&GitTool::Push {
        branch: "feat/x".into(),
        url: Some("https://github.com/o/r.git".into()),
        token: None,
    })
    .unwrap_err();
    assert!(err.contains("token"));

    let (script, token) = script_for(&GitTool::Push {
        branch: "feat/x".into(),
        url: Some("https://github.com/o/r.git".into()),
        token: Some("gh_secret".into()),
    })
    .unwrap();
    assert_eq!(token.as_deref(), Some("gh_secret"));
    assert!(!script.contains("gh_secret"), "token must stay in env");
    assert!(script.contains("HEAD:refs/heads/'feat/x'"));
    assert!(script.contains("GIT_ASKPASS"));
}

#[test]
fn pr_script_targets_repo_api_with_current_branch_head() {
    let (script, token) = script_for(&GitTool::PullRequest {
        title: "my pr".into(),
        head: String::new(),
        base: String::new(),
        url: Some("https://github.com/owner/repo.git".into()),
        token: Some("t".into()),
    })
    .unwrap();
    assert_eq!(token.as_deref(), Some("t"));
    assert!(script.contains("repos/owner/repo/pulls"));
    assert!(script.contains("$(git branch --show-current)"));
    assert!(script.contains(PR_BASE_DEFAULT));
    // auth header must expand the env var at runtime — never a literal
    assert!(script.contains("Bearer $GIT_TOKEN"));
    assert!(!script.contains("'$GIT_TOKEN'"));
}

#[test]
fn repo_slug_parses_github_urls_only() {
    assert_eq!(
        repo_slug("https://github.com/owner/repo.git").unwrap(),
        "owner/repo"
    );
    assert_eq!(
        repo_slug("https://github.com/owner/repo").unwrap(),
        "owner/repo"
    );
    assert!(repo_slug("https://gitlab.com/owner/repo").is_err());
    assert!(repo_slug("https://github.com/owner").is_err());
}

#[test]
fn host_side_ops_are_refused_in_sandbox() {
    assert!(script_for(&GitTool::Status).is_err());
    assert!(script_for(&GitTool::Diff).is_err());
    assert!(
        script_for(&GitTool::Clone {
            url: "https://github.com/o/r".into(),
            token: None,
        })
        .is_err()
    );
}
