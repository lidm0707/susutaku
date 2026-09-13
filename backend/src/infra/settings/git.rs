//! Per-project git repo settings persisted in the repo-root `setting.json`
//! under the `git` section. One project owns one repo; the secret (access
//! token) is write-only: stored to disk, used for clones, never echoed back.

pub const GIT_SECTION: &str = "git";
pub const FIELD_REPOS: &str = "repos";
pub const FIELD_PROJECT_ID: &str = "project_id";
pub const FIELD_URL: &str = "url";
pub const FIELD_SECRET: &str = "secret";

const MAX_URL_LEN: usize = 2048;
const MAX_SECRET_LEN: usize = 512;
pub const HTTPS_PREFIX: &str = "https://";
pub const HTTP_PREFIX: &str = "http://";

#[derive(Debug, Clone, PartialEq)]
pub struct GitRepo {
    pub project_id: i64,
    pub url: String,
    pub secret: Option<String>,
}

impl GitRepo {
    pub fn secret_set(&self) -> bool {
        self.secret.as_deref().is_some_and(|s| !s.is_empty())
    }
}

pub fn validate_url(raw: &str) -> Result<(), String> {
    if raw.is_empty() {
        return Err("repo url is empty".into());
    }
    if raw.chars().any(char::is_whitespace) {
        return Err("repo url must not contain whitespace".into());
    }
    if raw.len() > MAX_URL_LEN {
        return Err(format!("repo url longer than {MAX_URL_LEN} bytes"));
    }
    let is_http = raw.starts_with(HTTPS_PREFIX) || raw.starts_with(HTTP_PREFIX);
    if !is_http {
        return Err("repo url must start with https:// or http://".into());
    }
    if has_userinfo(raw) {
        return Err(
            "repo url must not embed credentials — put the token in the secret field".into(),
        );
    }
    Ok(())
}

/// `true` when the url carries `user:pass@` before the host — a token pasted
/// into the url would be echoed back by the settings API and stored on disk.
fn has_userinfo(url: &str) -> bool {
    url.find("://")
        .map(|scheme_end| rest_of(url, scheme_end))
        .is_some_and(|rest| {
            rest.find('/')
                .unwrap_or(rest.len())
                .checked_sub(1)
                .and_then(|end| rest.get(..end))
                .is_some_and(|authority| authority.contains('@'))
        })
}

fn rest_of(url: &str, scheme_end: usize) -> &str {
    url.get(scheme_end + 3..).unwrap_or("")
}

/// Strips any embedded userinfo — the settings API must never echo a url
/// that carries a credential.
pub fn redact_url(url: &str) -> String {
    url.find("://")
        .map(|scheme_end| {
            let rest = rest_of(url, scheme_end);
            match rest.find('@') {
                Some(at) if rest.find('/').is_none_or(|s| at < s) => {
                    format!("{}://{}", &url[..scheme_end], &rest[at + 1..])
                }
                _ => url.to_owned(),
            }
        })
        .unwrap_or_else(|| url.to_owned())
}

pub fn validate_secret(raw: &str) -> Result<(), String> {
    if raw.chars().any(char::is_whitespace) {
        return Err("repo secret must not contain whitespace or newlines".into());
    }
    if raw.len() > MAX_SECRET_LEN {
        return Err(format!("repo secret longer than {MAX_SECRET_LEN} bytes"));
    }
    Ok(())
}

pub fn read(doc: &serde_json::Value) -> Vec<GitRepo> {
    doc.get(GIT_SECTION)
        .and_then(|g| g.get(FIELD_REPOS))
        .and_then(|v| v.as_array())
        .map(|repos| {
            repos
                .iter()
                .filter_map(|r| {
                    Some(GitRepo {
                        project_id: r.get(FIELD_PROJECT_ID)?.as_i64()?,
                        url: r.get(FIELD_URL)?.as_str()?.to_owned(),
                        secret: r
                            .get(FIELD_SECRET)
                            .and_then(serde_json::Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `secret: None` keeps the stored secret; `Some("")` clears it.
pub fn write(doc: &mut serde_json::Value, repos: &[GitRepo]) {
    let entries: Vec<serde_json::Value> = repos
        .iter()
        .map(|r| {
            serde_json::json!({
                FIELD_PROJECT_ID: r.project_id,
                FIELD_URL: r.url,
                FIELD_SECRET: r.secret.as_deref().unwrap_or(""),
            })
        })
        .collect();
    doc[GIT_SECTION] = serde_json::json!({ FIELD_REPOS: entries });
}
