use serde_json::{Value, json};

use crate::client::{
    AUTH_HEADER, BEARER_PREFIX, CHOICES_FIELD, CODING_BASE_URL, CONTENT_FIELD, CONTENT_TYPE,
    JSON_CONTENT_TYPE, MESSAGE_FIELD, strip_bearer_scheme,
};

pub const QUOTA_URL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";
pub const HI_MODEL: &str = "glm-4.7";
pub const HI_MESSAGE: &str = "hi";
pub const HI_ROLE: &str = "user";
pub const HI_MAX_TOKENS: i64 = 16;
pub const MODEL_FIELD: &str = "model";
pub const MESSAGES_FIELD: &str = "messages";
pub const MAX_TOKENS_FIELD: &str = "max_tokens";
pub const STREAM_FIELD: &str = "stream";
pub const THINKING_FIELD: &str = "thinking";
pub const THINKING_TYPE_FIELD: &str = "type";
pub const THINKING_DISABLED: &str = "disabled";
pub const DATA_FIELD: &str = "data";
pub const LIMITS_FIELD: &str = "limits";
pub const LIMIT_TYPE_FIELD: &str = "type";
pub const PERCENTAGE_FIELD: &str = "percentage";
pub const NEXT_RESET_FIELD: &str = "nextResetTime";
pub const TOKENS_LIMIT: &str = "TOKENS_LIMIT";
pub const TIME_LIMIT: &str = "TIME_LIMIT";
pub const BODY_NOTE: &str = "<unreadable body>";

#[derive(Debug, PartialEq)]
pub struct Limit {
    pub limit_type: String,
    pub percentage: f32,
    pub next_reset_time: Option<i64>,
}

pub struct Quota {
    pub limits: Vec<Limit>,
}

pub fn parse_quota(value: &Value) -> Result<Quota, String> {
    let entries = value[DATA_FIELD][LIMITS_FIELD]
        .as_array()
        .ok_or_else(|| format!("missing {DATA_FIELD}.{LIMITS_FIELD}"))?;
    let limits = entries
        .iter()
        .map(parse_limit)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Quota { limits })
}

fn parse_limit(entry: &Value) -> Result<Limit, String> {
    let limit_type = entry[LIMIT_TYPE_FIELD]
        .as_str()
        .ok_or_else(|| format!("missing {LIMIT_TYPE_FIELD}"))?
        .to_string();
    let percentage = entry[PERCENTAGE_FIELD]
        .as_f64()
        .ok_or_else(|| format!("missing {PERCENTAGE_FIELD}"))? as f32;
    let next_reset_time = entry[NEXT_RESET_FIELD].as_i64();
    Ok(Limit {
        limit_type,
        percentage,
        next_reset_time,
    })
}

pub fn fetch_quota(token: &str) -> Result<Quota, String> {
    let raw = ureq::get(QUOTA_URL)
        .set(
            AUTH_HEADER,
            &format!("{BEARER_PREFIX}{}", strip_bearer_scheme(token)),
        )
        .call()
        .map_err(|e| http_message("quota", e))?
        .into_string()
        .map_err(|e| format!("read quota body: {e}"))?;
    let value: Value = serde_json::from_str(&raw).map_err(|e| format!("parse quota: {e}"))?;
    parse_quota(&value)
}

pub fn say_hi(token: &str) -> Result<String, String> {
    let body = json!({
        MODEL_FIELD: HI_MODEL,
        MESSAGES_FIELD: [{
            crate::parse::ROLE_FIELD: HI_ROLE,
            CONTENT_FIELD: HI_MESSAGE,
        }],
        THINKING_FIELD: { THINKING_TYPE_FIELD: THINKING_DISABLED },
        MAX_TOKENS_FIELD: HI_MAX_TOKENS,
        STREAM_FIELD: false,
    })
    .to_string();
    let raw = ureq::post(CODING_BASE_URL)
        .set(CONTENT_TYPE, JSON_CONTENT_TYPE)
        .set(
            AUTH_HEADER,
            &format!("{BEARER_PREFIX}{}", strip_bearer_scheme(token)),
        )
        .send_string(&body)
        .map_err(|e| http_message("hi", e))?
        .into_string()
        .map_err(|e| format!("read hi body: {e}"))?;
    let value: Value = serde_json::from_str(&raw).map_err(|e| format!("parse hi: {e}"))?;
    Ok(value[CHOICES_FIELD][0][MESSAGE_FIELD][CONTENT_FIELD]
        .as_str()
        .unwrap_or_default()
        .to_string())
}

fn http_message(what: &str, e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, response) => format!(
            "{what} http {code}: {}",
            response
                .into_string()
                .unwrap_or_else(|_| BODY_NOTE.to_string())
        ),
        other => format!("{what}: {other}"),
    }
}
