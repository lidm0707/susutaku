//! API layer: HTTP transport only. Maps DTOs to the chat use case.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use utoipa::OpenApi;

use crate::domain::SearchMode;
use crate::port::inbound::{ChatCmd, ChatHandling};
use crate::port::outbound::ModelSwitch;
use susutaku_mlx::tok::TokKind;

const DEFAULT_MAX_TOKENS: usize = 512;

pub fn router<T: ChatHandling + ModelSwitch + 'static>(use_case: Arc<T>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/models", get(models))
        .route("/api/models/select", post(select_model))
        .route("/api/chat", post(chat))
        .route("/api-docs/openapi.json", get(openapi_json))
        .fallback(not_found)
        .with_state(use_case)
}

#[utoipa::path(get, path = "/api/health", responses((status = 200, body = &str)))]
async fn health() -> &'static str {
    "ok"
}

async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[utoipa::path(get, path = "/api/models", responses((status = 200, body = [ModelInfo])))]
async fn models<T: ModelSwitch>(State(use_case): State<Arc<T>>) -> Json<Vec<ModelInfo>> {
    let selected = use_case.selected();
    Json(
        hf_loader::loadable_models(std::path::Path::new(crate::infra::engine::MODELS_ROOT))
            .iter()
            .map(|m| ModelInfo {
                name: m.name.clone(),
                loadable: m.is_loadable(),
                bytes: m.bytes,
                engine: m.engine(),
                selected: selected.as_deref() == Some(m.name.as_str()),
            })
            .collect(),
    )
}

#[utoipa::path(
    post,
    path = "/api/models/select",
    request_body = SelectRequest,
    responses((status = 200, body = SelectReply), (status = 400, body = str))
)]
async fn select_model<T: ModelSwitch>(
    State(use_case): State<Arc<T>>,
    Json(req): Json<SelectRequest>,
) -> Result<Json<SelectReply>, ApiError> {
    use_case.select(&req.name).map_err(ApiError::bad_request)?;
    Ok(Json(SelectReply { selected: req.name }))
}

#[utoipa::path(
    post,
    path = "/api/chat",
    request_body = ChatRequest,
    responses((status = 200, body = ChatReply), (status = 500, body = str))
)]
async fn chat<T: ChatHandling>(
    State(use_case): State<Arc<T>>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatReply>, ApiError> {
    let tok = TokKind::parse(req.tokenizer.as_deref());
    let outcome = use_case
        .execute(ChatCmd {
            message: req.message,
            mode: SearchMode::parse(req.search.as_deref()),
            max_tokens: req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            tokenizer: tok,
            think: req.think.unwrap_or(false),
        })
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(ChatReply {
        model: outcome.model,
        reply: outcome.text,
        searched: outcome.searched,
        tokenizer: tok.as_str(),
        prompt_tokens: outcome.stats.prompt_tokens,
        prompt_tps: outcome.stats.prompt_tps(),
        decode_tokens: outcome.stats.decode_tokens,
        decode_tps: outcome.stats.decode_tps(),
    }))
}

async fn not_found(_req: Request) -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

#[derive(OpenApi)]
#[openapi(
    info(title = "susutaku backend", license(name = "MIT", url = "https://opensource.org/licenses/MIT")),
    paths(
        health,
        models,
        select_model,
        chat,
    ),
    components(schemas(
        ModelInfo,
        SelectRequest,
        SelectReply,
        ChatRequest,
        ChatReply,
    ))
)]
struct ApiDoc;

#[derive(Deserialize, utoipa::ToSchema)]
struct ChatRequest {
    message: String,
    max_tokens: Option<usize>,
    /// "on" | "off" | "auto" (default: model decides, Zed-style).
    search: Option<String>,
    /// "normal" | "katgpt" (default: normal).
    tokenizer: Option<String>,
    /// Emit a reasoning block (default: off — closed thinking).
    think: Option<bool>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct ChatReply {
    model: Option<String>,
    reply: String,
    searched: bool,
    tokenizer: &'static str,
    prompt_tokens: usize,
    prompt_tps: f64,
    decode_tokens: usize,
    decode_tps: f64,
}

#[derive(Serialize, utoipa::ToSchema)]
struct ModelInfo {
    name: String,
    loadable: bool,
    bytes: u64,
    engine: &'static str,
    selected: bool,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct SelectRequest {
    name: String,
}

#[derive(Serialize, utoipa::ToSchema)]
struct SelectReply {
    selected: String,
}

struct ApiError(String);

impl ApiError {
    fn internal(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }

    fn bad_request(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self(msg) if msg.starts_with("no loadable model") => {
                (StatusCode::NOT_FOUND, msg).into_response()
            }
            Self(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response(),
        }
    }
}
