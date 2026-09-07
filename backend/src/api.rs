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
use crate::infra::codex_auth;
use crate::infra::codex_auth::{CodexAuth, LoginStatus};
use crate::infra::codex_chat;
use crate::infra::zai_settings::SettingsState;
use crate::port::inbound::{ChatCmd, ChatHandling};
use crate::port::outbound::ModelSwitch;
use std::path::PathBuf;
use susutaku_mlx::tok::TokKind;

const DEFAULT_MAX_TOKENS: usize = 512;

pub fn router<T: ChatHandling + ModelSwitch + 'static>(
    use_case: Arc<T>,
    codex_workspace: PathBuf,
) -> Router {
    let core = Router::new()
        .route("/api/health", get(health))
        .route("/api/models", get(models))
        .route("/api/models/select", post(select_model))
        .route("/api/chat", post(chat))
        .route("/api-docs/openapi.json", get(openapi_json))
        .fallback(not_found)
        .with_state(use_case);
    let auth = Router::new()
        .route("/api/auth/codex/start", post(codex_start))
        .route("/api/auth/codex/status", get(codex_status))
        .route("/api/auth/codex/models", get(codex_models))
        .route("/api/chat/codex", post(chat_codex))
        .with_state(Arc::new(CodexAuth::new(codex_workspace)));
    let settings = Router::new()
        .route(
            "/api/settings/zai",
            get(get_zai_settings).post(set_zai_settings),
        )
        .route("/api/chat/zai", post(chat_zai))
        .with_state(Arc::new(SettingsState::load()));
    core.merge(auth).merge(settings)
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

#[utoipa::path(
    post,
    path = "/api/auth/codex/start",
    responses((status = 200, body = CodexStartReply), (status = 500, body = str))
)]
async fn codex_start(
    State(auth): State<Arc<CodexAuth>>,
) -> Result<Json<CodexStartReply>, ApiError> {
    let authorize_url = auth.start().map_err(ApiError::bad_request)?;
    Ok(Json(CodexStartReply { authorize_url }))
}

#[utoipa::path(
    get,
    path = "/api/auth/codex/status",
    responses((status = 200, body = CodexStatusReply))
)]
async fn codex_status(State(auth): State<Arc<CodexAuth>>) -> Json<CodexStatusReply> {
    let (status, error) = match auth.status() {
        LoginStatus::LoggedIn => (CodexTokenStatus::LoggedIn, None),
        LoginStatus::Expired => (CodexTokenStatus::Expired, None),
        LoginStatus::Missing => (CodexTokenStatus::Missing, None),
        LoginStatus::AwaitingLogin => (CodexTokenStatus::AwaitingLogin, None),
        LoginStatus::Failed(e) => (CodexTokenStatus::Failed, Some(e)),
    };
    let cli_available = codex_cli::check_available().is_ok();
    Json(CodexStatusReply {
        status,
        cli_available,
        error,
    })
}

#[utoipa::path(
    get,
    path = "/api/auth/codex/models",
    responses((status = 200, body = [CodexModelInfo]))
)]
async fn codex_models() -> Json<Vec<CodexModelInfo>> {
    Json(
        codex_cli::list_models(&codex_auth::codex_home())
            .iter()
            .map(|m| CodexModelInfo {
                id: m.slug.clone(),
                label: m.display_name.clone(),
            })
            .collect(),
    )
}

#[utoipa::path(
    post,
    path = "/api/chat/codex",
    request_body = CodexChatRequest,
    responses((status = 200, body = ChatReply), (status = 500, body = str))
)]
async fn chat_codex(
    State(auth): State<Arc<CodexAuth>>,
    Json(req): Json<CodexChatRequest>,
) -> Result<Json<ChatReply>, ApiError> {
    const TOKENIZER: &str = "codex";
    const ZERO_TPS: f64 = 0.0;
    let model = req.model.clone().filter(|m| !m.is_empty());
    let workspace = auth.workspace().to_path_buf();
    let chat_model = model.clone();
    let reply = tokio::task::spawn_blocking(move || {
        codex_chat::chat(&req.message, chat_model.as_deref(), &workspace)
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
    .map_err(ApiError::internal)?;
    Ok(Json(ChatReply {
        model,
        reply,
        searched: false,
        tokenizer: TOKENIZER,
        prompt_tokens: 0,
        prompt_tps: ZERO_TPS,
        decode_tokens: 0,
        decode_tps: ZERO_TPS,
    }))
}

#[utoipa::path(
    get,
    path = "/api/settings/zai",
    responses((status = 200, body = ZaiSettingsReply))
)]
async fn get_zai_settings(State(state): State<Arc<SettingsState>>) -> Json<ZaiSettingsReply> {
    let zai = state.zai();
    Json(ZaiSettingsReply {
        api_key_set: zai.api_key.is_some(),
        model: zai.model.unwrap_or_default(),
    })
}

#[utoipa::path(
    post,
    path = "/api/settings/zai",
    request_body = ZaiSettingsRequest,
    responses((status = 200, body = ZaiSettingsReply), (status = 500, body = str))
)]
async fn set_zai_settings(
    State(state): State<Arc<SettingsState>>,
    Json(req): Json<ZaiSettingsRequest>,
) -> Result<Json<ZaiSettingsReply>, ApiError> {
    state
        .set_zai(req.api_key, req.model)
        .map_err(ApiError::internal)?;
    let zai = state.zai();
    Ok(Json(ZaiSettingsReply {
        api_key_set: zai.api_key.is_some(),
        model: zai.model.unwrap_or_default(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/chat/zai",
    request_body = ZaiChatRequest,
    responses((status = 200, body = ChatReply), (status = 500, body = str))
)]
async fn chat_zai(
    State(state): State<Arc<SettingsState>>,
    Json(req): Json<ZaiChatRequest>,
) -> Result<Json<ChatReply>, ApiError> {
    const TOKENIZER: &str = "zai";
    const ZERO_TPS: f64 = 0.0;
    let client = state.zai_client().map_err(ApiError::bad_request)?;
    let chat_model = req.model.clone().unwrap_or_default();
    let reply = tokio::task::spawn_blocking(move || {
        ai_interface_layer::provider::ChatProvider::complete(
            &client,
            &ai_interface_layer::request::ChatRequest::prompt(chat_model, &req.message),
        )
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
    .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(ChatReply {
        model: Some(zai_api::client::DEFAULT_MODEL.to_string()),
        reply: reply.content,
        searched: false,
        tokenizer: TOKENIZER,
        prompt_tokens: 0,
        prompt_tps: ZERO_TPS,
        decode_tokens: 0,
        decode_tps: ZERO_TPS,
    }))
}

async fn not_found(_req: Request) -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "susutaku backend",
        license(name = "MIT", url = "https://opensource.org/licenses/MIT")
    ),
    paths(
        health,
        models,
        select_model,
        chat,
        codex_start,
        codex_status,
        codex_models,
        chat_codex,
        get_zai_settings,
        set_zai_settings,
        chat_zai,
    ),
    components(schemas(
        ModelInfo,
        SelectRequest,
        SelectReply,
        ChatRequest,
        ChatReply,
        CodexStartReply,
        CodexStatusReply,
        CodexModelInfo,
        CodexChatRequest,
        ZaiSettingsReply,
        ZaiSettingsRequest,
        ZaiChatRequest,
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

#[derive(Serialize, utoipa::ToSchema)]
struct CodexStartReply {
    authorize_url: String,
}

#[derive(Serialize, utoipa::ToSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CodexTokenStatus {
    LoggedIn,
    Expired,
    Missing,
    AwaitingLogin,
    Failed,
}

#[derive(Serialize, utoipa::ToSchema)]
struct CodexStatusReply {
    status: CodexTokenStatus,
    cli_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct CodexModelInfo {
    id: String,
    label: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct CodexChatRequest {
    message: String,
    /// "gpt-5-codex" | "gpt-5" | "gpt-5-mini" (default: gpt-5-codex).
    model: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct ZaiSettingsReply {
    api_key_set: bool,
    model: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct ZaiSettingsRequest {
    /// Omit or send empty to keep the saved key; the saved key is never returned.
    api_key: Option<String>,
    model: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct ZaiChatRequest {
    message: String,
    /// Optional override; otherwise the model from settings / `glm-4.6`.
    model: Option<String>,
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
