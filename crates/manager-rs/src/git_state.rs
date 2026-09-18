//! Git toolcall: ALL git operations run host-side against the agent's work
//! tree — on the machine that owns the agent. The repo token is used here
//! only and never enters the agent's sandbox, so the agent cannot read it.

use std::fs;
use std::path::Path;

use git_rs::GitRepo;
use proto_rs::GitTool;

/// Spawn-time layout inside the agent workspace that is not workspace
/// content: a fresh init repo and the snapshots dir.
const SPAWN_LAYOUT_ENTRIES: [&str; 2] = [".git", "snapshots"];

const GIT_IDENTITY_NAME: &str = "susutaku-agent";
const GH_API_BASE: &str = "https://api.github.com";
/// Overrides the GitHub API base (e.g. a local fake for e2e tests).
const GH_API_BASE_ENV: &str = "SUSUTAKU_GH_API_BASE";
/// PR base used when the caller does not name one.
pub const PR_BASE_DEFAULT: &str = "main";
/// Timeout for the host-side PR API call.
const PR_TIMEOUT_SECS: u64 = 30;

/// Executes `tool` against `workspace` — the agent's mounted workspace
/// (the dir the agent sees as `/workspace`) — returning text output.
pub fn apply(work_tree: &Path, tool: &GitTool) -> Result<String, String> {
    match tool {
        GitTool::Clone { url, token } => clone(work_tree, url, token.as_deref()),
        GitTool::Status => status(work_tree),
        GitTool::Diff => diff(work_tree),
        GitTool::Branch { name } => branch(work_tree, name),
        GitTool::Commit { message } => commit(work_tree, message),
        GitTool::Push { branch, url, token } => {
            push(work_tree, branch, url.as_deref(), token.as_deref())
        }
        GitTool::PullRequest {
            title,
            head,
            base,
            url,
            token,
        } => pull_request(
            work_tree,
            title,
            head,
            base,
            url.as_deref(),
            token.as_deref(),
        ),
    }
}

/// Clones `url` into the work tree. Only a fresh workspace may be cloned
/// into: any content beyond the bare `.git` a spawn creates is refused.
fn clone(workspace: &Path, url: &str, token: Option<&str>) -> Result<String, String> {
    let stale = || {
        std::fs::read_dir(workspace)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|e| !SPAWN_LAYOUT_ENTRIES.contains(&e.file_name().to_str().unwrap_or("")))
            })
            .unwrap_or(false)
    };
    if stale() {
        return Err(format!(
            "workspace {} is not empty; clone needs a fresh workspace",
            workspace.display()
        ));
    }
    // `git clone` refuses non-empty targets — clear the spawn layout first.
    for entry in SPAWN_LAYOUT_ENTRIES {
        let _ = fs::remove_dir_all(workspace.join(entry));
    }
    let _ = fs::remove_file(workspace.join(".git"));
    GitRepo::clone_into(url, workspace, token)
        .map(|_| format!("cloned {url} into {}", workspace.display()))
        .map_err(|e| e.to_string())
}

fn status(workspace: &Path) -> Result<String, String> {
    let repo = GitRepo::open(workspace).map_err(|e| e.to_string())?;
    let head = repo
        .head_oid()
        .map(|oid| oid.to_string())
        .unwrap_or_else(|_| "no commits".to_string());
    let dirty = repo.is_dirty().map_err(|e| e.to_string())?;
    Ok(format!(
        "HEAD {head}\n{}",
        if dirty { "dirty" } else { "clean" }
    ))
}

/// Placeholder the GIT DIFF tool renders for an empty patch — consumers
/// must treat it as "clean", not as diff content.
pub const NO_CHANGES: &str = "(no changes)";

fn diff(workspace: &Path) -> Result<String, String> {
    let repo = GitRepo::open(workspace).map_err(|e| e.to_string())?;
    // unborn HEAD (fresh work tree, no commits): diff against the empty tree
    let patch = if repo.has_commits() {
        let patch = repo.patch_workdir().map_err(|e| e.to_string())?;
        // clean tree: fall back to committed-but-unpushed work vs base branch
        if patch.is_empty() {
            repo.patch_unpushed().map_err(|e| e.to_string())?
        } else {
            patch
        }
    } else {
        repo.patch_workdir_empty_base().map_err(|e| e.to_string())?
    };
    if patch.is_empty() {
        Ok(NO_CHANGES.to_string())
    } else {
        Ok(patch)
    }
}

/// Creates `name` at HEAD (kept when it exists) and checks it out.
fn branch(workspace: &Path, name: &str) -> Result<String, String> {
    let repo = GitRepo::open(workspace).map_err(|e| e.to_string())?;
    repo.create_checkout_branch(name)
        .map_err(|e| e.to_string())?;
    Ok(format!("on branch {name}"))
}

/// Commits all pending work; a clean tree is a reported no-op, matching the
/// old in-sandbox script's tolerance for empty commits.
fn commit(workspace: &Path, message: &str) -> Result<String, String> {
    let repo = GitRepo::open(workspace).map_err(|e| e.to_string())?;
    if !repo.is_dirty().map_err(|e| e.to_string())? {
        return Ok("nothing to commit".to_string());
    }
    let oid = repo.commit_all(message).map_err(|e| e.to_string())?;
    Ok(format!("committed {oid}"))
}

/// Pushes the local `branch` to `url` (default `origin`); the token is used
/// for https auth here on the host and never reaches the sandbox.
pub fn push(
    workspace: &Path,
    branch: &str,
    url: Option<&str>,
    token: Option<&str>,
) -> Result<String, String> {
    let token = require_token(token)?;
    let repo = GitRepo::open(workspace).map_err(|e| e.to_string())?;
    let url = url.ok_or("push needs the repo url (no origin remote in agent trees)")?;
    repo.push_branch(url, branch, Some(&token))
        .map_err(|e| e.to_string())?;
    Ok(format!("pushed HEAD:refs/heads/{branch}"))
}

/// Opens a PR against `base` (default [`PR_BASE_DEFAULT`]) via the GitHub
/// API, from the host. `head` defaults to the work tree's current branch.
fn pull_request(
    workspace: &Path,
    title: &str,
    head: &str,
    base: &str,
    url: Option<&str>,
    token: Option<&str>,
) -> Result<String, String> {
    let token = require_token(token)?;
    let repo_url = url.ok_or("pr needs the repo url")?;
    let slug = repo_slug(repo_url)?;
    let head = if head.is_empty() {
        let repo = GitRepo::open(workspace).map_err(|e| e.to_string())?;
        repo.current_branch().map_err(|e| e.to_string())?
    } else {
        head.to_string()
    };
    let base = if base.is_empty() {
        PR_BASE_DEFAULT
    } else {
        base
    };
    let body = serde_json::json!({ "title": title, "head": head, "base": base }).to_string();
    let api = std::env::var(GH_API_BASE_ENV).unwrap_or_else(|_| GH_API_BASE.to_owned());
    let api_url = format!("{api}/repos/{slug}/pulls");
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(PR_TIMEOUT_SECS))
        .build();
    let resp = agent
        .post(&api_url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", GIT_IDENTITY_NAME)
        .set("Authorization", &format!("Bearer {token}"))
        .send_string(&body)
        .map_err(|e| match e {
            ureq::Error::Status(code, resp) => {
                let body = resp.into_string().unwrap_or_default();
                format!("pr api answered {code}: {body}")
            }
            other => other.to_string(),
        })?;
    let status = resp.status();
    let body = resp.into_string().map_err(|e| e.to_string())?;
    Ok(format!("HTTP {status}\n{body}"))
}

fn require_token(token: Option<&str>) -> Result<String, String> {
    token
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "needs a repo token (bind one in settings → git repos)".to_string())
}

/// `owner/repo` out of a github remote url; errors for non-github hosts.
pub fn repo_slug(url: &str) -> Result<String, String> {
    const GH_HOST: &str = "github.com/";
    let path = url
        .split_once(GH_HOST)
        .map(|(_, p)| p)
        .ok_or_else(|| format!("only {GH_HOST} remotes are supported for pr, got {url}"))?
        .trim_end_matches('/')
        .trim_end_matches(".git");
    if path.matches('/').count() != 1 || path.is_empty() {
        return Err(format!("{url} is not an owner/repo github url"));
    }
    Ok(path.to_string())
}
