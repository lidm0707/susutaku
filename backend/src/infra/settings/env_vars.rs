//! Named environment variables persisted in the repo-root `setting.json`
//! under the `env_vars` section, handed to agent sandbox runs. Secret
//! values are write-only from the API's point of view: stored to disk,
//! never echoed back — the API returns a fixed mask instead.

pub const ENV_VARS_SECTION: &str = "env_vars";
pub const FIELD_VARS: &str = "vars";
pub const FIELD_NAME: &str = "name";
pub const FIELD_VALUE: &str = "value";
pub const FIELD_SECRET: &str = "secret";

const MAX_NAME_LEN: usize = 64;
const MAX_VALUE_LEN: usize = 4096;

/// What the settings API returns instead of a secret value.
pub const SECRET_MASK: &str = "••••••";

#[derive(Debug, Clone, PartialEq)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
    pub secret: bool,
}

impl EnvVar {
    /// The value safe to show: secrets collapse to [`SECRET_MASK`].
    pub fn display_value(&self) -> &str {
        if self.secret {
            SECRET_MASK
        } else {
            &self.value
        }
    }
}

/// `NAME` must look like an env var: letters, digits, underscore, not
/// starting with a digit — so it can be handed to a sandbox as-is.
pub fn validate_name(raw: &str) -> Result<(), String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("env var name is empty".into());
    }
    if name.len() > MAX_NAME_LEN {
        return Err(format!("env var name longer than {MAX_NAME_LEN} bytes"));
    }
    let first = name.chars().next().unwrap_or_default();
    if first.is_ascii_digit() {
        return Err(format!("env var name {name:?} must not start with a digit"));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!(
            "env var name {name:?} must use only letters, digits and underscore"
        ));
    }
    Ok(())
}

pub fn validate_value(raw: &str) -> Result<(), String> {
    if raw.len() > MAX_VALUE_LEN {
        return Err(format!("env var value longer than {MAX_VALUE_LEN} bytes"));
    }
    if raw.chars().any(|c| c == '\0') {
        return Err("env var value must not contain NUL".into());
    }
    Ok(())
}

pub fn read(doc: &serde_json::Value) -> Vec<EnvVar> {
    doc.get(ENV_VARS_SECTION)
        .and_then(|s| s.get(FIELD_VARS))
        .and_then(|v| v.as_array())
        .map(|vars| {
            vars.iter()
                .filter_map(|v| {
                    Some(EnvVar {
                        name: v.get(FIELD_NAME)?.as_str()?.to_owned(),
                        value: v
                            .get(FIELD_VALUE)
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        secret: v
                            .get(FIELD_SECRET)
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn write(doc: &mut serde_json::Value, vars: &[EnvVar]) {
    let entries: Vec<serde_json::Value> = vars
        .iter()
        .map(|v| {
            serde_json::json!({
                FIELD_NAME: v.name,
                FIELD_VALUE: v.value,
                FIELD_SECRET: v.secret,
            })
        })
        .collect();
    doc[ENV_VARS_SECTION] = serde_json::json!({ FIELD_VARS: entries });
}
