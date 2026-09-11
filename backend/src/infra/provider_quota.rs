//! Unified per-platform quota view (Z.AI, Codex/ChatGPT, Claude).
//! Everything is read from locally stored credentials; only Z.AI quota
//! requires an HTTP call.

use std::path::Path;

use base64::Engine;
use serde::Deserialize;
use serde_json::Value;
use zai_api::quota;

use super::claude_auth::claude_home;
use super::codex_auth::codex_home;
use super::zai_settings::SettingsState;

pub const PLATFORM_ZAI: &str = "zai";
pub const PLATFORM_CODEX: &str = "codex";
pub const PLATFORM_CLAUDE: &str = "claude";

const REASON_NO_TOKEN: &str = "not logged in";
const REASON_TOKEN_EXPIRED: &str = "token expired";
const REASON_NO_USAGE_DATA: &str = "no usage data";
const REASON_NO_ZAI_KEY: &str = "no Z.ai key configured";

const CLAIM_RATE_LIMITS: &str = "chatgpt_rate_limits";
const MS_PER_SEC: i64 = 1000;

#[derive(Debug, Clone, PartialEq, serde::Serialize, utoipa::ToSchema)]
pub struct PlatformQuota {
    pub platform: String,
    pub available: bool,
    pub reason: Option<String>,
    pub tokens_used_pct: Option<f32>,
    pub window_reset_ms: Option<i64>,
    pub window_used_pct: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, utoipa::ToSchema)]
pub struct QuotaBoard {
    pub platforms: Vec<PlatformQuota>,
}

pub fn board(state: &SettingsState, codex_usage: Option<codex_usage_rs::Row>) -> QuotaBoard {
    QuotaBoard {
        platforms: vec![
            zai(state),
            codex(&codex_home(), codex_usage),
            claude(&claude_home()),
        ],
    }
}

pub fn zai(state: &SettingsState) -> PlatformQuota {
    let token = match state.zai_token() {
        Ok(token) => token,
        Err(_) => return unavailable(PLATFORM_ZAI, REASON_NO_ZAI_KEY),
    };
    match quota::fetch_quota(&token) {
        Ok(found) => from_zai_limits(&found.limits),
        Err(e) => unavailable(PLATFORM_ZAI, &e),
    }
}

fn from_zai_limits(limits: &[quota::Limit]) -> PlatformQuota {
    let now = now_ms();
    let tokens = limits.iter().find(|l| l.limit_type == quota::TOKENS_LIMIT);
    let time = limits
        .iter()
        .filter(|l| l.limit_type == quota::TIME_LIMIT)
        .filter(|l| l.next_reset_time.is_some_and(|t| t > now))
        .min_by_key(|l| l.next_reset_time.unwrap_or(i64::MAX));
    PlatformQuota {
        platform: PLATFORM_ZAI.to_string(),
        available: true,
        reason: None,
        tokens_used_pct: tokens.map(|l| l.percentage),
        window_reset_ms: time.and_then(|l| l.next_reset_time),
        window_used_pct: time.map(|l| l.percentage),
    }
}

pub fn codex(home: &Path, usage: Option<codex_usage_rs::Row>) -> PlatformQuota {
    if let Some(row) = usage {
        return from_codex_usage_row(&row);
    }
    if let Some(platform) =
        codex_claims(home).and_then(|claims| platform_from_codex_claims(&claims))
    {
        return platform;
    }
    codex_from_usage(&codex_cli::usage::status(home))
}

/// Freshest source: the scheduler's DB snapshot (rollout `rate_limits`).
/// Mapping mirrors the z.ai row: primary window is the headline % + bar;
/// weekly (secondary), when present, shows as the small "win %".
fn from_codex_usage_row(row: &codex_usage_rs::Row) -> PlatformQuota {
    PlatformQuota {
        platform: PLATFORM_CODEX.to_string(),
        available: true,
        reason: None,
        tokens_used_pct: row.primary_used_percent.map(|p| p as f32),
        window_reset_ms: row.primary_resets_at.map(|t| t.timestamp_millis()),
        window_used_pct: row.secondary_used_percent.map(|p| p as f32),
    }
}

fn codex_from_usage(status: &codex_cli::UsageStatus) -> PlatformQuota {
    match status {
        codex_cli::UsageStatus::Ok(usage) => PlatformQuota {
            platform: PLATFORM_CODEX.to_string(),
            available: true,
            reason: None,
            tokens_used_pct: usage.secondary.as_ref().map(|w| w.used_percent as f32),
            window_reset_ms: usage.primary.as_ref().and_then(|w| w.resets_at).map(to_ms),
            window_used_pct: usage.primary.as_ref().map(|w| w.used_percent as f32),
        },
        codex_cli::UsageStatus::NotLoggedIn => unavailable(PLATFORM_CODEX, REASON_NO_TOKEN),
        codex_cli::UsageStatus::Unavailable(e) => {
            unavailable(PLATFORM_CODEX, &format!("{REASON_NO_USAGE_DATA}: {e}"))
        }
    }
}

fn codex_claims(home: &Path) -> Option<Value> {
    let bytes = std::fs::read(codex_cli::auth::auth_path(home)).ok()?;
    let doc: Value = serde_json::from_slice(&bytes).ok()?;
    let token = doc.pointer("/tokens/access_token")?.as_str()?;
    jwt_payload_claims(token)
}

/// Codex access tokens carry ChatGPT rate-limit windows as JWT claims:
/// `primary` is the 5h window, `secondary` the weekly/short window.
pub fn platform_from_codex_claims(claims: &Value) -> Option<PlatformQuota> {
    let limits: ChatGptRateLimits =
        serde_json::from_value(claims.get(CLAIM_RATE_LIMITS)?.clone()).ok()?;
    Some(PlatformQuota {
        platform: PLATFORM_CODEX.to_string(),
        available: true,
        reason: None,
        tokens_used_pct: limits.secondary.as_ref().and_then(|w| w.used_percent),
        window_reset_ms: limits.primary.as_ref().and_then(|w| w.resets_at).map(to_ms),
        window_used_pct: limits.primary.as_ref().and_then(|w| w.used_percent),
    })
}

pub fn claude(home: &Path) -> PlatformQuota {
    match claude_cli::auth::check(home) {
        claude_cli::auth::AuthStatus::LoggedIn => PlatformQuota {
            platform: PLATFORM_CLAUDE.to_string(),
            available: true,
            reason: None,
            tokens_used_pct: None,
            window_reset_ms: None,
            window_used_pct: None,
        },
        claude_cli::auth::AuthStatus::Expired => unavailable(PLATFORM_CLAUDE, REASON_TOKEN_EXPIRED),
        claude_cli::auth::AuthStatus::Missing => unavailable(PLATFORM_CLAUDE, REASON_NO_TOKEN),
    }
}

fn unavailable(platform: &str, reason: &str) -> PlatformQuota {
    PlatformQuota {
        platform: platform.to_string(),
        available: false,
        reason: Some(reason.to_string()),
        tokens_used_pct: None,
        window_reset_ms: None,
        window_used_pct: None,
    }
}

fn jwt_payload_claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn to_ms(unix_sec: i64) -> i64 {
    unix_sec.saturating_mul(MS_PER_SEC)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Deserialize)]
struct ChatGptRateLimits {
    primary: Option<RateWindow>,
    secondary: Option<RateWindow>,
}

#[derive(Deserialize)]
struct RateWindow {
    used_percent: Option<f32>,
    resets_at: Option<i64>,
}
