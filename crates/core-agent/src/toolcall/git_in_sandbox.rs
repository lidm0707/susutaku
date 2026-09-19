//! In-sandbox git work: branch/commit/push/pr run INSIDE the agent's own
//! podman container. Each run gets outbound network plus the repo token as
//! a run-scoped env var — the token never lands in the workspace, the
//! remote config, or the transcript.

use std::time::Duration;

use crate::podman::{NetworkPolicyChoice, Sandbox, SandboxLimits};
use proto_rs::GitTool;

pub const GIT_RUN_TIMEOUT_SECS: u64 = 300;
const GIT_IDENTITY_NAME: &str = "susutaku-agent";
const GIT_IDENTITY_EMAIL: &str = "agent@susutaku.local";
const GIT_TOKEN_ENV: &str = "GIT_TOKEN";
const ASKPASS_PATH: &str = "/tmp/.git-askpass.sh";
const PR_PAYLOAD_PATH: &str = "/tmp/.git-pr-payload.json";
const GH_API_BASE: &str = "https://api.github.com";
/// Overrides the GitHub API base (e.g. a local fake for e2e tests).
const GH_API_BASE_ENV: &str = "SUSUTAKU_GH_API_BASE";
const GH_API_HEADER: &str = "Accept: application/vnd.github+json";
const GH_API_UA: &str = "susutaku-agent";
/// PR base used when the caller does not name one.
pub const PR_BASE_DEFAULT: &str = "main";
/// head branch is resolved at runtime in the sandbox — the payload is
/// single-quoted, so an embedded `$(...)` would never execute.
const HEAD_PLACEHOLDER: &str = "__SUSUTAKU_HEAD__";

/// Executes `tool` inside `sandbox` (the agent's own container) with
/// outbound network enabled, returning the command output.
pub fn apply(sandbox: &Sandbox, tool: &GitTool) -> Result<String, String> {
    let (script, token) = script_for(tool)?;
    let limits = SandboxLimits {
        timeout: Duration::from_secs(GIT_RUN_TIMEOUT_SECS),
        ..SandboxLimits::default()
    };
    let env: Vec<(String, String)> = token
        .map(|t| vec![(GIT_TOKEN_ENV.to_string(), t)])
        .unwrap_or_default();
    sandbox
        .run_with_env(&script, &limits, NetworkPolicyChoice::Enabled, &env)
        .map_err(|e| e.to_string())
}

/// Builds the shell script for one git op plus the optional run-scoped
/// token. Pure, so it is testable without podman.
pub fn script_for(tool: &GitTool) -> Result<(String, Option<String>), String> {
    match tool {
        GitTool::Branch { name } => {
            let q = shq(name);
            Ok((
                work(&format!(
                    "git checkout {q} 2>/dev/null || git checkout -b {q}"
                )),
                None,
            ))
        }
        GitTool::Commit { message } => Ok((
            work(&format!(
                "git add -A && {{ git -c user.name={n} -c user.email={e} commit -m {m} || if git diff --quiet --cached; then echo nothing to commit; else echo commit failed; exit 1; fi; }}",
                n = shq(GIT_IDENTITY_NAME),
                e = shq(GIT_IDENTITY_EMAIL),
                m = shq(message),
            )),
            None,
        )),
        GitTool::Push { branch, url, token } => {
            let token = require_token(token)?;
            let target = url.as_deref().map(shq).unwrap_or_else(|| "origin".into());
            let b = shq(branch);
            Ok((
                work(&format!(
                    "{askpass} && GIT_ASKPASS={p} GIT_TERMINAL_PROMPT=0 git push {target} HEAD:refs/heads/{b}",
                    askpass = askpass_setup(),
                    p = shq(ASKPASS_PATH),
                )),
                Some(token),
            ))
        }
        GitTool::PullRequest {
            title,
            head,
            base,
            url,
            token,
        } => {
            let token = require_token(token)?;
            let repo = url.as_deref().ok_or("pr needs the repo url")?;
            let slug = repo_slug(repo)?;
            let base = if base.is_empty() {
                PR_BASE_DEFAULT
            } else {
                base
            };
            let payload = format!(
                "{{\"title\":{},\"head\":{},\"base\":{}}}",
                json_str(title),
                json_str(&head_current(head)),
                json_str(base),
            );
            let script = format!(
                "{} && payload=$(printf '%s' {}) && \
                 head=$(git branch --show-current) && \
                 payload=${{payload//{HEAD_PLACEHOLDER}/${{head//\\\"/}}}} && \
                 printf '%s' \"$payload\" > {PR_PAYLOAD_PATH} && \
                 curl -s -w '\\nHTTP %{{http_code}}' -X POST \
                 -H \"Authorization: Bearer ${GIT_TOKEN_ENV}\" -H {accept} -H {ua} -H 'Content-Type: application/json' \
                 -d @{PR_PAYLOAD_PATH} {api_base}/repos/{slug}/pulls",
                work(&askpass_setup()),
                shq(&payload),
                accept = shq(GH_API_HEADER),
                ua = shq(&format!("User-Agent: {GH_API_UA}")),
                api_base =
                    shq(&std::env::var(GH_API_BASE_ENV).unwrap_or_else(|_| GH_API_BASE.to_owned())),
            );
            Ok((script, Some(token)))
        }
        GitTool::Clone { .. } | GitTool::Status | GitTool::Diff | GitTool::TaskBranch { .. } => {
            Err("clone/status/diff/task-branch run host-side, not in the sandbox".to_string())
        }
    }
}

fn work(body: &str) -> String {
    format!("set -e; cd \"$HOME\"; {body}")
}

fn require_token(token: &Option<String>) -> Result<String, String> {
    token
        .clone()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| "needs a repo token (bind one in settings → git repos)".to_string())
}

fn head_current(head: &str) -> String {
    if head.is_empty() {
        HEAD_PLACEHOLDER.to_string()
    } else {
        head.to_string()
    }
}

/// Writes a GIT_ASKPASS helper that answers prompts from `$GIT_TOKEN`.
fn askpass_setup() -> String {
    format!(
        "printf '%s\\n' '#!/bin/bash' 'case \"$1\" in Username*) echo x-access-token;; *) echo \"$GIT_TOKEN\";; esac' > {ASKPASS_PATH} && chmod +x {ASKPASS_PATH}"
    )
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

/// Single-quote a string for safe shell embedding.
fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// JSON string literal with minimal escaping.
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
