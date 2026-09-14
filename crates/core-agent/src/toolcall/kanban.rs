use serde_json::json;
use std::env;

use super::http;

pub const BASE_URL_ENV: &str = "SUSUTAKU_BACKEND_URL";
pub const TOKEN_ENV: &str = "SUSUTAKU_BACKEND_TOKEN";
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8991";
pub const DEFAULT_COLUMN_ID: &str = "todo";
const CARDS_PATH: &str = "/api/kanban/cards";
const JSON_CONTENT_TYPE: &str = "application/json";
const BEARER_PREFIX: &str = "Bearer ";
const SEND_ERROR: &str = "card create failed: ";
const READ_ERROR: &str = "card created but response unreadable: ";

/// Backend base url: env override, else the host-local backend port.
pub fn base_url() -> String {
    env::var(BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
}

/// Bearer token for the backend API, when configured.
pub fn token() -> Option<String> {
    env::var(TOKEN_ENV).ok().filter(|t| !t.is_empty())
}

#[derive(Debug, Clone)]
pub struct NewCard {
    pub project_id: i64,
    pub title: String,
    pub description: Option<String>,
}

/// The exact JSON body POSTed to `/api/kanban/cards` (pure; testable).
pub fn card_body(card: &NewCard) -> String {
    json!({
        "project_id": card.project_id,
        "column_id": DEFAULT_COLUMN_ID,
        "title": card.title,
        "description": card.description,
    })
    .to_string()
}

/// Creates a card via the backend REST API. Host-side only — the sandbox
/// has no network, so this must never run inside a jail.
pub fn create_card(base_url: &str, token: Option<&str>, card: &NewCard) -> Result<String, String> {
    let url = format!("{base_url}{CARDS_PATH}");
    let mut req = http().post(&url).set("Content-Type", JSON_CONTENT_TYPE);
    if let Some(token) = token {
        req = req.set("Authorization", &format!("{BEARER_PREFIX}{token}"));
    }
    let resp = req
        .send(card_body(card).as_bytes())
        .map_err(|e| format!("{SEND_ERROR}{e}"))?;
    resp.into_string().map_err(|e| format!("{READ_ERROR}{e}"))
}
