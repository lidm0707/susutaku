//! API layer: HTTP transport only. Maps DTOs to the chat use case.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, FromRequestParts, Multipart, Path, Request, State},
    http::{self, StatusCode, request::Parts},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Serialize};
use utoipa::OpenApi;

use crate::app::kanban::{CardView, KanbanApp};
use crate::domain::SearchMode;
use crate::infra::claude_auth::{ClaudeAuth, LoginStatus as ClaudeLoginStatus};
use crate::infra::claude_chat;
use crate::infra::codex_auth;
use crate::infra::codex_auth::{CodexAuth, LoginStatus};
use crate::infra::codex_chat;
use crate::infra::sandbox::AgentSandbox;
use crate::infra::zai_settings::{SettingsState, ZaiSettings};
use crate::port::inbound::{ChatCmd, ChatHandling};
use crate::port::outbound::ModelSwitch;
use crate::port::outbound::{
    AgentConfigDraft, CardMove, CardPatch, NewCard, NewPipeline, NewProject, NewWorkspace,
};
use prompt_sys::{MAX_PROMPT_CHARS, PromptBuilder, Role as PromptRole};
use std::path::PathBuf;
use susutaku_mlx::tok::TokKind;

const DEFAULT_MAX_TOKENS: usize = 512;

pub fn router<T: ChatHandling + ModelSwitch + 'static>(
    use_case: Arc<T>,
    catalog: Arc<dyn crate::infra::model_client::ModelCatalog>,
    codex_workspace: PathBuf,
    kanban_store: std::sync::Arc<kanban_rs::Store>,
    manager: Arc<manager_rs::ManagerProcess>,
) -> Router {
    let kanban_store_for_sched = kanban_store.clone();
    let core = Router::new()
        .route("/api/health", get(health))
        .route("/api/models/select", post(select_model))
        .route("/api/chat", post(chat))
        .route("/api/prompts/render", post(render_prompt))
        .route("/api-docs/openapi.json", get(openapi_json))
        .fallback(not_found)
        .with_state(use_case.clone());
    let models_router = Router::new()
        .route("/api/models", get(models))
        .with_state(catalog);
    let core = core.merge(models_router);
    let manager_router = Router::new()
        .route(
            "/api/manager/agents",
            get(manager_list_agents).post(spawn_agent),
        )
        .route("/api/manager/agents/{agent}/run", post(run_agent_command))
        .route("/api/manager/agents/{agent}/finish", post(finish_agent))
        .with_state(manager);
    let core = core.merge(manager_router);
    let auth = Router::new()
        .route("/api/auth/codex/start", post(codex_start))
        .route("/api/auth/codex/status", get(codex_status))
        .route("/api/auth/codex/models", get(codex_models))
        .route("/api/chat/codex", post(chat_codex))
        .with_state(Arc::new(CodexAuth::new(codex_workspace.clone())));
    let claude = Router::new()
        .route("/api/auth/claude/start", post(claude_start))
        .route("/api/auth/claude/callback", post(claude_callback))
        .route("/api/auth/claude/status", get(claude_status))
        .route("/api/chat/claude", post(chat_claude))
        .with_state(Arc::new(ClaudeAuth::new(codex_workspace.clone())));
    let settings = Router::new()
        .route(
            "/api/settings/zai",
            get(get_zai_settings).post(set_zai_settings),
        )
        .route("/api/settings/zai/models", post(zai_model_action))
        .route(
            "/api/settings/client-env",
            get(get_client_env).post(set_client_env),
        )
        .route(
            "/api/settings/system-prompt",
            get(get_system_prompt).post(set_system_prompt),
        )
        .route("/api/chat/zai", post(chat_zai))
        .route("/api/sandbox", get(list_sandboxes))
        .route("/api/sandbox/purge", post(purge_sandbox))
        .route("/api/sandbox/sweep", post(sweep_sandboxes))
        .route("/install.sh", get(install_script))
        .with_state(Arc::new(SettingsState::load()));
    core.merge(auth)
        .merge(claude)
        .merge(settings)
        .merge(kanban_router(kanban_state(
            kanban_store,
            std::sync::Arc::new(crate::app::schedule_work::spawn(std::sync::Arc::new(
                crate::app::kanban::build(kanban_store_for_sched),
            ))),
        )))
}

fn kanban_router(state: KanbanStore) -> Router {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/bootstrap", get(bootstrap))
        .route("/api/auth/change-password", post(change_password))
        .route("/api/auth/users", get(list_users).post(create_user))
        .route(
            "/api/workspaces",
            get(list_workspaces).post(create_workspace),
        )
        .route("/api/workspaces/{id}", delete(delete_workspace))
        .route(
            "/api/workspaces/{id}/projects",
            get(list_projects).post(create_project),
        )
        .route("/api/projects/{id}", delete(delete_project))
        .route("/api/kanban/cards", get(list_cards).post(create_card))
        .route(
            "/api/kanban/cards/{id}",
            delete(remove_card).put(update_card),
        )
        .route("/api/kanban/cards/{id}/move", post(move_card))
        .route(
            "/api/kanban/cards/{id}/comments",
            get(list_comments).post(add_comment),
        )
        .route(
            "/api/kanban/cards/{id}/agent",
            get(get_agent).put(set_agent),
        )
        .route("/api/kanban/cards/{id}/pipeline", put(set_card_pipeline))
        .route("/api/kanban/cards/{id}/run", post(run_card))
        .route("/api/kanban/cards/{id}/schedule", put(set_card_schedule))
        .route("/api/cronjobs", get(list_cronjobs))
        .route(
            "/api/attachments",
            post(upload_attachment).layer(DefaultBodyLimit::max(ATTACHMENT_MAX_BYTES)),
        )
        .route("/api/pipelines", get(list_pipelines).post(create_pipeline))
        .route(
            "/api/pipelines/{id}",
            put(update_pipeline).delete(remove_pipeline),
        )
        .route("/api/agents", get(list_agents).post(create_agent))
        .route(
            "/api/agents/{id}",
            put(update_agent_cfg).delete(remove_agent_cfg),
        )
        .with_state(state)
}

/// Bearer-token auth extractor: resolves the session to a user.
struct AuthUser(kanban_rs::UserRow);

const BEARER_PREFIX: &str = "Bearer ";
const UNAUTHORIZED_MSG: &str = "unauthorized";
const FORBIDDEN_MSG: &str = "forbidden";
const PASSWORD_CHANGE_REQUIRED_MSG: &str = "password change required";
const CHANGE_PASSWORD_PATH: &str = "/api/auth/change-password";
const LOGOUT_PATH: &str = "/api/auth/logout";
const ATTACHMENTS_DIR: &str = "attachments";
const ATTACHMENT_UPLOAD_PREFIX: &str = "upload-";
const ATTACHMENT_FIELD: &str = "file";
const ATTACHMENT_DEFAULT_NAME: &str = "image";
const ATTACHMENT_MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(serde::Serialize, utoipa::ToSchema)]
struct UploadReply {
    path: String,
}

fn sanitize_name(raw: &str) -> String {
    let clean: String = raw
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if clean.is_empty() {
        ATTACHMENT_DEFAULT_NAME.to_string()
    } else {
        clean
    }
}

#[utoipa::path(
    post,
    path = "/api/attachments",
    responses((status = 200, body = UploadReply), (status = 400, body = str), (status = 403, body = str))
)]
async fn upload_attachment(
    State(_state): State<KanbanStore>,
    user: AuthUser,
    mut form: Multipart,
) -> Result<Json<UploadReply>, ApiError> {
    require_edit(&user)?;
    while let Some(field) = form
        .next_field()
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
    {
        if field.name() != Some(ATTACHMENT_FIELD) {
            continue;
        }
        let name = sanitize_name(field.file_name().unwrap_or(ATTACHMENT_DEFAULT_NAME));
        let data = field.bytes().await.map_err(|e| ApiError::internal(e.to_string()))?;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir = format!("{ATTACHMENTS_DIR}/{ATTACHMENT_UPLOAD_PREFIX}{nanos}");
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
        let path = format!("{dir}/{name}");
        tokio::fs::write(&path, &data)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
        return Ok(Json(UploadReply { path }));
    }
    Err(ApiError::bad_request("missing upload field"))
}

impl FromRequestParts<KanbanStore> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        store: &KanbanStore,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix(BEARER_PREFIX))
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, UNAUTHORIZED_MSG).into_response())?;
        let user = store
            .store
            .auth(header)
            .await
            .map_err(|e| ApiError::internal(e.to_string()).into_response())?
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, UNAUTHORIZED_MSG).into_response())?;
        // Forced password change blocks everything except the change itself and logout.
        if user.must_change_password {
            let path = parts.uri.path();
            if path != CHANGE_PASSWORD_PATH && path != LOGOUT_PATH {
                return Err((StatusCode::FORBIDDEN, PASSWORD_CHANGE_REQUIRED_MSG).into_response());
            }
        }
        Ok(AuthUser(user))
    }
}

fn require_edit(user: &AuthUser) -> Result<(), ApiError> {
    let role = parse_role(&user.0.role)?;
    role.can_edit()
        .then_some(())
        .ok_or_else(|| ApiError(FORBIDDEN_MSG.to_string(), StatusCode::FORBIDDEN))
}

fn require_users(user: &AuthUser) -> Result<(), ApiError> {
    let role = parse_role(&user.0.role)?;
    role.can_manage_users()
        .then_some(())
        .ok_or_else(|| ApiError(FORBIDDEN_MSG.to_string(), StatusCode::FORBIDDEN))
}

fn parse_role(role: &str) -> Result<kanban_rs::Role, ApiError> {
    role.parse()
        .map_err(|e: kanban_rs::StoreError| ApiError::internal(e.to_string()))
}

#[utoipa::path(
    post,
    path = "/api/auth/login",
    request_body = LoginRequest,
    responses((status = 200, body = LoginReply), (status = 401, body = str))
)]
async fn login(
    State(state): State<KanbanStore>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginReply>, ApiError> {
    let token = state
        .store
        .login(&req.username, &req.password)
        .await
        .map_err(store_err)?;
    let user = match &token {
        Some(t) => state.store.auth(t).await.map_err(store_err)?,
        None => None,
    };
    let user = user.ok_or(ApiError(UNAUTHORIZED_MSG.to_string(), StatusCode::UNAUTHORIZED))?;
    Ok(Json(LoginReply {
        token: token.unwrap_or_default(),
        username: user.username,
        role: user.role,
        must_change_password: user.must_change_password,
    }))
}

#[utoipa::path(
    post,
    path = "/api/auth/change-password",
    request_body = ChangePasswordRequest,
    responses((status = 200, body = str), (status = 400, body = str), (status = 401, body = str))
)]
async fn change_password(
    State(state): State<KanbanStore>,
    user: AuthUser,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<&'static str, ApiError> {
    state
        .store
        .change_password(user.0.id, &req.old_password, &req.new_password)
        .await
        .map_err(|e| match e {
            kanban_rs::StoreError::PasswordTooShort | kanban_rs::StoreError::BadCredentials => {
                ApiError::bad_request(e.to_string())
            }
            other => ApiError::internal(other.to_string()),
        })?;
    Ok("ok")
}

#[utoipa::path(post, path = "/api/auth/logout", responses((status = 200, body = str)))]
async fn logout(
    State(state): State<KanbanStore>,
    parts: axum::extract::RawQuery,
    req: Request,
) -> &'static str {
    let _ = parts;
    let token = req
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix(BEARER_PREFIX));
    if let Some(token) = token {
        let _ = state.store.logout(token).await;
    }
    "ok"
}

#[utoipa::path(get, path = "/api/auth/bootstrap", responses((status = 200, body = BootstrapReply)))]
async fn bootstrap(
    State(state): State<KanbanStore>,
    _user: AuthUser,
) -> Result<Json<BootstrapReply>, ApiError> {
    let count = state.store.user_count().await.map_err(store_err)?;
    Ok(Json(BootstrapReply {
        needs_setup: count == 0,
    }))
}

#[utoipa::path(get, path = "/api/auth/users", responses((status = 200, body = [UserDto]), (status = 403, body = str)))]
async fn list_users(
    State(state): State<KanbanStore>,
    user: AuthUser,
) -> Result<Json<Vec<UserDto>>, ApiError> {
    require_users(&user)?;
    let users = state.store.list_users().await.map_err(store_err)?;
    Ok(Json(users.into_iter().map(UserDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/auth/users",
    request_body = CreateUserRequest,
    responses((status = 200, body = UserDto), (status = 400, body = str), (status = 403, body = str))
)]
async fn create_user(
    State(state): State<KanbanStore>,
    user: AuthUser,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<UserDto>, ApiError> {
    // Bootstrap: with zero users, an unauthenticated call creates the owner.
    let needs_setup = state.store.user_count().await.map_err(store_err)? == 0;
    if !needs_setup {
        require_users(&user)?;
    }
    let role = parse_role(&req.role)?;
    let role = if needs_setup {
        kanban_rs::Role::Owner
    } else {
        role
    };
    let row = state
        .store
        .create_user(&kanban_rs::NewUser {
            username: &req.username,
            password: &req.password,
            role,
        })
        .await
        .map_err(|e| match e {
            kanban_rs::StoreError::UsernameTaken | kanban_rs::StoreError::PasswordTooShort => {
                ApiError::bad_request(e.to_string())
            }
            other => ApiError::internal(other.to_string()),
        })?;
    Ok(Json(UserDto::from(row)))
}

#[utoipa::path(get, path = "/api/health", responses((status = 200, body = &str)))]
async fn health() -> &'static str {
    "ok"
}

#[utoipa::path(
    post,
    path = "/api/prompts/render",
    request_body = RenderPromptRequest,
    responses((status = 200, body = RenderPromptReply), (status = 400, body = str))
)]
async fn render_prompt(
    Json(req): Json<RenderPromptRequest>,
) -> Result<Json<RenderPromptReply>, ApiError> {
    let prompt = build_prompt(&req.sections)?;
    Ok(Json(RenderPromptReply {
        rendered: prompt.render(),
        chars: prompt.len(),
        max_chars: MAX_PROMPT_CHARS,
    }))
}

async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[utoipa::path(get, path = "/api/models", responses((status = 200, body = [ModelInfo])))]
async fn models(
    State(catalog): State<Arc<dyn crate::infra::model_client::ModelCatalog>>,
) -> Result<Json<Vec<ModelInfo>>, ApiError> {
    let models = catalog.list().map_err(ApiError::internal)?;
    Ok(Json(
        models
            .iter()
            .map(|m| ModelInfo {
                name: m.name.clone(),
                loadable: m.loadable,
                bytes: m.bytes,
                engine: m.engine.clone(),
                selected: m.selected,
            })
            .collect(),
    ))
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
    Json(zai_reply(&state.zai()))
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
    Ok(Json(zai_reply(&state.zai())))
}

fn zai_reply(zai: &ZaiSettings) -> ZaiSettingsReply {
    ZaiSettingsReply {
        api_key_set: zai.api_key.is_some(),
        model: zai.model.clone().unwrap_or_default(),
        models: zai
            .models
            .iter()
            .map(|m| ZaiModelReply {
                model: m.model.clone(),
                api_key_set: m.key().is_some(),
            })
            .collect(),
    }
}

#[utoipa::path(
    post,
    path = "/api/settings/zai/models",
    request_body = ZaiModelAction,
    responses((status = 200, body = ZaiSettingsReply), (status = 400, body = str))
)]
async fn zai_model_action(
    State(state): State<Arc<SettingsState>>,
    Json(action): Json<ZaiModelAction>,
) -> Result<Json<ZaiSettingsReply>, ApiError> {
    let res = match action {
        ZaiModelAction::Add { model, api_key } => state.zai_add_model(&model, &api_key),
        ZaiModelAction::SetKey { model, api_key } => state.zai_set_key(&model, &api_key),
        ZaiModelAction::Remove { model } => state.zai_remove_model(&model),
        ZaiModelAction::SetActive { model } => state.zai_set_active(&model),
    };
    res.map_err(ApiError::bad_request)?;
    Ok(Json(zai_reply(&state.zai())))
}

#[derive(Deserialize, utoipa::ToSchema)]
struct SystemPromptRequest {
    prompt: String,
}

#[derive(Serialize, utoipa::ToSchema)]
struct SystemPromptReply {
    prompt: String,
}

#[utoipa::path(
    get,
    path = "/api/settings/system-prompt",
    responses((status = 200, body = SystemPromptReply))
)]
async fn get_system_prompt(State(state): State<Arc<SettingsState>>) -> Json<SystemPromptReply> {
    Json(SystemPromptReply {
        prompt: state.system_prompt(),
    })
}

#[utoipa::path(
    post,
    path = "/api/settings/system-prompt",
    request_body = SystemPromptRequest,
    responses((status = 200, body = SystemPromptReply), (status = 500, body = str))
)]
async fn set_system_prompt(
    State(state): State<Arc<SettingsState>>,
    Json(req): Json<SystemPromptRequest>,
) -> Result<Json<SystemPromptReply>, ApiError> {
    state
        .set_system_prompt(&req.prompt)
        .map_err(ApiError::internal)?;
    Ok(Json(SystemPromptReply {
        prompt: state.system_prompt(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/settings/client-env",
    responses((status = 200, body = ClientEnvReply))
)]
async fn get_client_env(State(state): State<Arc<SettingsState>>) -> Json<ClientEnvReply> {
    let reported = state.client_env().is_some();
    let mut env = state.client_env().unwrap_or_default();
    stamp_host(&mut env);
    Json(ClientEnvReply {
        reported,
        user_agent: env.user_agent,
        platform: env.platform,
        language: env.language,
        timezone: env.timezone,
        screen: env.screen,
        workspace_path: env.workspace_path,
        hostname: env.hostname,
        os: env.os,
        arch: env.arch,
    })
}

#[utoipa::path(
    post,
    path = "/api/settings/client-env",
    request_body = ClientEnvRequest,
    responses((status = 200, body = ClientEnvReply), (status = 500, body = str))
)]
async fn set_client_env(
    State(state): State<Arc<SettingsState>>,
    Json(req): Json<ClientEnvRequest>,
) -> Result<Json<ClientEnvReply>, ApiError> {
    let mut env = crate::infra::client_env::ClientEnv {
        user_agent: req.user_agent,
        platform: req.platform,
        language: req.language,
        timezone: req.timezone,
        screen: req.screen,
        workspace_path: req.workspace_path,
        ..crate::infra::client_env::ClientEnv::default()
    };
    stamp_host(&mut env);
    state.set_client_env(&env).map_err(ApiError::internal)?;
    Ok(Json(ClientEnvReply {
        reported: true,
        user_agent: env.user_agent,
        platform: env.platform,
        language: env.language,
        timezone: env.timezone,
        screen: env.screen,
        workspace_path: env.workspace_path,
        hostname: env.hostname,
        os: env.os,
        arch: env.arch,
    }))
}

fn stamp_host(env: &mut crate::infra::client_env::ClientEnv) {
    let (hostname, os, arch) = crate::infra::client_env::host_fingerprint();
    env.hostname = hostname;
    env.os = os;
    env.arch = arch;
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
    let system = build_system_message(&req.system)?;
    let chat_model = req.model.clone().unwrap_or_default();
    let user = req.message;
    let reply = tokio::task::spawn_blocking(move || {
        ai_interface_layer::provider::ChatProvider::complete(
            &client,
            &ai_interface_layer::request::ChatRequest::new(
                chat_model,
                system
                    .into_iter()
                    .chain(std::iter::once(ai_interface_layer::message::Message::user(
                        user,
                    )))
                    .collect(),
            ),
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

#[derive(Deserialize, utoipa::ToSchema)]
struct ClaudeCodeRequest {
    code: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct ClaudeChatRequest {
    message: String,
}

#[utoipa::path(
    post,
    path = "/api/auth/claude/start",
    responses((status = 200, body = CodexStartReply), (status = 500, body = str))
)]
async fn claude_start(
    State(auth): State<Arc<ClaudeAuth>>,
) -> Result<Json<CodexStartReply>, ApiError> {
    let authorize_url = auth.start().map_err(ApiError::bad_request)?;
    Ok(Json(CodexStartReply { authorize_url }))
}

#[utoipa::path(
    post,
    path = "/api/auth/claude/callback",
    request_body = ClaudeCodeRequest,
    responses((status = 200, body = ()), (status = 400, body = str))
)]
async fn claude_callback(
    State(auth): State<Arc<ClaudeAuth>>,
    Json(req): Json<ClaudeCodeRequest>,
) -> Result<Json<()>, ApiError> {
    const MISSING_CODE: &str = "missing code";
    if req.code.trim().is_empty() {
        return Err(ApiError::bad_request(MISSING_CODE));
    }
    auth.callback(req.code.trim())
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(()))
}

#[utoipa::path(
    get,
    path = "/api/auth/claude/status",
    responses((status = 200, body = CodexStatusReply))
)]
async fn claude_status(State(auth): State<Arc<ClaudeAuth>>) -> Json<CodexStatusReply> {
    let (status, error) = match auth.status() {
        ClaudeLoginStatus::LoggedIn => (CodexTokenStatus::LoggedIn, None),
        ClaudeLoginStatus::Expired => (CodexTokenStatus::Expired, None),
        ClaudeLoginStatus::Missing => (CodexTokenStatus::Missing, None),
        ClaudeLoginStatus::AwaitingLogin => (CodexTokenStatus::AwaitingLogin, None),
        ClaudeLoginStatus::Failed(e) => (CodexTokenStatus::Failed, Some(e)),
    };
    let cli_available = claude_cli::check_available().is_ok();
    Json(CodexStatusReply {
        status,
        cli_available,
        error,
    })
}

#[utoipa::path(
    post,
    path = "/api/chat/claude",
    request_body = ClaudeChatRequest,
    responses((status = 200, body = ChatReply), (status = 500, body = str))
)]
async fn chat_claude(
    State(_auth): State<Arc<ClaudeAuth>>,
    Json(req): Json<ClaudeChatRequest>,
) -> Result<Json<ChatReply>, ApiError> {
    const TOKENIZER: &str = "claude";
    const ZERO_TPS: f64 = 0.0;
    const MODEL: &str = "claude";
    let reply = tokio::task::spawn_blocking(move || claude_chat::chat(&req.message))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(ApiError::internal)?;
    Ok(Json(ChatReply {
        model: Some(MODEL.to_string()),
        reply,
        searched: false,
        tokenizer: TOKENIZER,
        prompt_tokens: 0,
        prompt_tps: ZERO_TPS,
        decode_tokens: 0,
        decode_tps: ZERO_TPS,
    }))
}

fn build_prompt(sections: &[PromptSectionDto]) -> Result<prompt_sys::Prompt, ApiError> {
    const UNKNOWN_ROLE: &str = "unknown prompt role: ";
    let mut builder = PromptBuilder::new();
    for section in sections {
        let role = PromptRole::parse(&section.role)
            .ok_or_else(|| ApiError::bad_request(format!("{UNKNOWN_ROLE}{}", section.role)))?;
        builder = builder.section(role, section.body.clone());
    }
    builder
        .build()
        .map_err(|e| ApiError::bad_request(e.to_string()))
}

fn build_system_message(
    sections: &Option<Vec<PromptSectionDto>>,
) -> Result<Option<ai_interface_layer::message::Message>, ApiError> {
    let Some(sections) = sections else {
        return Ok(None);
    };
    if sections.is_empty() {
        return Ok(None);
    }
    build_prompt(sections).map(|p| Some(ai_interface_layer::message::Message::system(p.render())))
}

/// Shared handler state: the composed kanban app plus the raw store for auth.
struct KanbanState {
    app: KanbanApp,
    store: std::sync::Arc<kanban_rs::Store>,
    sched: std::sync::Arc<crate::app::schedule_work::ScheduleHandle>,
}

type KanbanStore = std::sync::Arc<KanbanState>;

fn kanban_state(
    store: std::sync::Arc<kanban_rs::Store>,
    sched: std::sync::Arc<crate::app::schedule_work::ScheduleHandle>,
) -> KanbanStore {
    std::sync::Arc::new(KanbanState {
        app: crate::app::kanban::build(store.clone()),
        store,
        sched,
    })
}

#[utoipa::path(get, path = "/api/workspaces", responses((status = 200, body = [WorkspaceDto])))]
async fn list_workspaces(
    State(state): State<KanbanStore>,
    _user: AuthUser,
) -> Result<Json<Vec<WorkspaceDto>>, ApiError> {
    let rows = state.app.workspaces.list().await.map_err(store_err)?;
    Ok(Json(rows.into_iter().map(WorkspaceDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/workspaces",
    request_body = CreateWorkspaceRequest,
    responses((status = 200, body = WorkspaceDto), (status = 400, body = str))
)]
async fn create_workspace(
    State(state): State<KanbanStore>,
    user: AuthUser,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<Json<WorkspaceDto>, ApiError> {
    require_edit(&user)?;
    let row = state
        .app
        .workspaces
        .create(NewWorkspace {
            name: req.name.trim().to_string(),
        })
        .await
        .map_err(workspace_err)?;
    Ok(Json(WorkspaceDto::from(row)))
}

#[utoipa::path(delete, path = "/api/workspaces/{id}", responses((status = 200, body = str), (status = 404, body = str)))]
async fn delete_workspace(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state.app.workspaces.remove(id).await.map_err(store_err)?;
    Ok("ok")
}

#[utoipa::path(get, path = "/api/workspaces/{id}/projects", responses((status = 200, body = [ProjectDto])))]
async fn list_projects(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    _user: AuthUser,
) -> Result<Json<Vec<ProjectDto>>, ApiError> {
    let rows = state.app.projects.list(id).await.map_err(store_err)?;
    Ok(Json(rows.into_iter().map(ProjectDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/workspaces/{id}/projects",
    request_body = CreateProjectRequest,
    responses((status = 200, body = ProjectDto), (status = 400, body = str))
)]
async fn create_project(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
    Json(req): Json<CreateProjectRequest>,
) -> Result<Json<ProjectDto>, ApiError> {
    require_edit(&user)?;
    let row = state
        .app
        .projects
        .create(NewProject {
            workspace_id: id,
            name: req.name.trim().to_string(),
        })
        .await
        .map_err(workspace_err)?;
    Ok(Json(ProjectDto::from(row)))
}

#[utoipa::path(delete, path = "/api/projects/{id}", responses((status = 200, body = str), (status = 404, body = str)))]
async fn delete_project(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state.app.projects.remove(id).await.map_err(store_err)?;
    Ok("ok")
}

#[utoipa::path(get, path = "/api/kanban/cards", responses((status = 200, body = [CardDto])))]
async fn list_cards(
    State(state): State<KanbanStore>,
    axum::extract::Query(query): axum::extract::Query<ListCardsQuery>,
    _user: AuthUser,
) -> Result<Json<Vec<CardDto>>, ApiError> {
    let views = state
        .app
        .card_views(query.project_id)
        .await
        .map_err(kanban_err)?;
    Ok(Json(views.into_iter().map(CardDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/kanban/cards",
    request_body = CreateCardRequest,
    responses((status = 200, body = CardDto), (status = 400, body = str))
)]
async fn create_card(
    State(state): State<KanbanStore>,
    user: AuthUser,
    Json(req): Json<CreateCardRequest>,
) -> Result<Json<CardDto>, ApiError> {
    require_edit(&user)?;
    let priority = req
        .priority
        .unwrap_or_else(|| kanban_rs::PRIORITY_NORMAL.to_string());
    let row = state
        .app
        .cards
        .create(NewCard {
            project_id: req.project_id,
            column_id: req.column_id,
            title: req.title,
            description: req.description.unwrap_or_default(),
            priority,
        })
        .await
        .map_err(kanban_err)?;
    Ok(Json(CardDto::from(CardView {
        card: row,
        pipeline_name: None,
    })))
}

#[utoipa::path(
    post,
    path = "/api/kanban/cards/{id}/move",
    request_body = MoveCardRequest,
    responses((status = 200, body = str), (status = 404, body = str))
)]
async fn move_card(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
    Json(req): Json<MoveCardRequest>,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state
        .app
        .cards
        .move_card(CardMove {
            id,
            column_id: req.column_id,
            position: req.position,
        })
        .await
        .map_err(kanban_err)?;
    Ok("ok")
}

#[utoipa::path(delete, path = "/api/kanban/cards/{id}", responses((status = 200, body = str), (status = 404, body = str)))]
async fn remove_card(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state.app.cards.remove(id).await.map_err(kanban_err)?;
    Ok("ok")
}

#[utoipa::path(get, path = "/api/kanban/cards/{id}/agent", responses((status = 200, body = AgentDto), (status = 404, body = str)))]
async fn get_agent(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    _user: AuthUser,
) -> Result<Json<AgentDto>, ApiError> {
    let agent = state
        .app
        .cards
        .agent(id)
        .await
        .map_err(kanban_err)?
        .ok_or(ApiError(ApiError::NOT_FOUND_MSG.to_string(), StatusCode::NOT_FOUND))?;
    Ok(Json(AgentDto::from(agent)))
}

#[utoipa::path(
    put,
    path = "/api/kanban/cards/{id}/agent",
    request_body = SetAgentRequest,
    responses((status = 200, body = str), (status = 404, body = str))
)]
async fn set_agent(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
    Json(req): Json<SetAgentRequest>,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state
        .app
        .cards
        .set_agent(
            id,
            &kanban_rs::AgentState {
                name: req.name,
                state: req.state.unwrap_or(serde_json::Value::Null),
            },
        )
        .await
        .map_err(kanban_err)?;
    Ok("ok")
}

#[utoipa::path(
    put,
    path = "/api/kanban/cards/{id}/pipeline",
    request_body = SetCardPipelineRequest,
    responses((status = 200, body = str), (status = 404, body = str))
)]
async fn set_card_pipeline(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
    Json(req): Json<SetCardPipelineRequest>,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state
        .app
        .cards
        .set_pipeline(id, req.pipeline_id)
        .await
        .map_err(kanban_err)?;
    Ok("ok")
}

#[utoipa::path(
    post,
    path = "/api/kanban/cards/{id}/run",
    responses((status = 200, body = crate::app::pipeline_run::RunRecord), (status = 404, body = str), (status = 400, body = str))
)]
async fn run_card(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
) -> Result<Json<crate::app::pipeline_run::RunRecord>, ApiError> {
    require_edit(&user)?;
    let record = crate::app::pipeline_run::run_card_pipeline(&state.app, id)
        .await
        .map_err(kanban_err)?;
    Ok(Json(record))
}

#[utoipa::path(
    put,
    path = "/api/kanban/cards/{id}/schedule",
    request_body = SetScheduleRequest,
    responses((status = 200, body = str), (status = 404, body = str), (status = 400, body = str))
)]
async fn set_card_schedule(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
    Json(req): Json<SetScheduleRequest>,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    if let Some(expr) = &req.cron {
        work::services::cron::Cron::parse(expr)
            .map_err(|e| ApiError::bad_request(e.to_string()))?;
    }
    state
        .app
        .cards
        .set_cron(id, req.cron.clone())
        .await
        .map_err(kanban_err)?;
    Ok("ok")
}

#[derive(Serialize, utoipa::ToSchema)]
struct CronJobDto {
    card_id: i64,
    title: String,
    cron: String,
    pipeline_name: Option<String>,
    next_run: u64,
}

#[utoipa::path(
    get,
    path = "/api/cronjobs",
    responses((status = 200, body = [CronJobDto]))
)]
async fn list_cronjobs(
    State(state): State<KanbanStore>,
    _user: AuthUser,
) -> Result<Json<Vec<CronJobDto>>, ApiError> {
    let next: std::collections::HashMap<i64, u64> = state
        .sched
        .entries()
        .into_iter()
        .map(|e| (e.card_id, e.next_run))
        .collect();
    let jobs = state
        .app
        .card_views(None)
        .await
        .map_err(kanban_err)?
        .into_iter()
        .filter_map(|v| {
            let cron = v.card.cron.clone()?;
            Some(CronJobDto {
                card_id: v.card.id,
                title: v.card.title,
                cron,
                pipeline_name: v.pipeline_name,
                next_run: next.get(&v.card.id).copied().unwrap_or(0),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(jobs))
}

#[utoipa::path(
    put,
    path = "/api/kanban/cards/{id}",
    request_body = UpdateCardRequest,
    responses((status = 200, body = str), (status = 404, body = str))
)]
async fn update_card(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
    Json(req): Json<UpdateCardRequest>,
) -> Result<Json<CardDto>, ApiError> {
    require_edit(&user)?;
    state
        .app
        .cards
        .update(CardPatch {
            id,
            title: req.title,
            description: req.description,
            assignee: req.assignee,
        })
        .await
        .map_err(kanban_err)?;
    let view = state
        .app
        .card_view(id)
        .await
        .map_err(kanban_err)?
        .ok_or(ApiError(ApiError::NOT_FOUND_MSG.to_string(), StatusCode::NOT_FOUND))?;
    Ok(Json(CardDto::from(view)))
}

#[utoipa::path(
    get,
    path = "/api/kanban/cards/{id}/comments",
    responses((status = 200, body = [CommentDto]), (status = 404, body = str))
)]
async fn list_comments(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    _user: AuthUser,
) -> Result<Json<Vec<CommentDto>>, ApiError> {
    let rows = state.app.comments.list(id).await.map_err(kanban_err)?;
    Ok(Json(rows.into_iter().map(CommentDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/kanban/cards/{id}/comments",
    request_body = AddCommentRequest,
    responses((status = 200, body = CommentDto), (status = 404, body = str))
)]
async fn add_comment(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
    Json(req): Json<AddCommentRequest>,
) -> Result<Json<CommentDto>, ApiError> {
    require_edit(&user)?;
    let row = state
        .app
        .comments
        .add(id, user.0.username, req.body.trim().to_string())
        .await
        .map_err(kanban_err)?;
    Ok(Json(CommentDto::from(row)))
}

#[utoipa::path(get, path = "/api/pipelines", responses((status = 200, body = [PipelineDto])))]
async fn list_pipelines(
    State(state): State<KanbanStore>,
    _user: AuthUser,
) -> Result<Json<Vec<PipelineDto>>, ApiError> {
    let rows = state.app.pipelines.list().await.map_err(cfg_err)?;
    Ok(Json(rows.into_iter().map(PipelineDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/pipelines",
    request_body = SavePipelineRequest,
    responses((status = 200, body = PipelineDto), (status = 400, body = str))
)]
async fn create_pipeline(
    State(state): State<KanbanStore>,
    user: AuthUser,
    Json(req): Json<SavePipelineRequest>,
) -> Result<Json<PipelineDto>, ApiError> {
    require_edit(&user)?;
    let row = state
        .app
        .pipelines
        .create(NewPipeline {
            name: req.name.trim().to_string(),
            spec: serde_json::to_string(&req.spec)
                .map_err(|e| ApiError::internal(e.to_string()))?,
        })
        .await
        .map_err(cfg_err)?;
    let spec = serde_json::from_str(&row.spec).unwrap_or(serde_json::Value::Null);
    Ok(Json(PipelineDto {
        id: row.id,
        name: row.name,
        spec,
    }))
}

#[utoipa::path(
    put,
    path = "/api/pipelines/{id}",
    request_body = SavePipelineRequest,
    responses((status = 200, body = str), (status = 404, body = str), (status = 400, body = str))
)]
async fn update_pipeline(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
    Json(req): Json<SavePipelineRequest>,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    let spec = serde_json::to_string(&req.spec).map_err(|e| ApiError::internal(e.to_string()))?;
    state
        .app
        .pipelines
        .update(
            id,
            NewPipeline {
                name: req.name.trim().to_string(),
                spec,
            },
        )
        .await
        .map_err(cfg_err)?;
    Ok("ok")
}

#[utoipa::path(delete, path = "/api/pipelines/{id}", responses((status = 200, body = str), (status = 404, body = str)))]
async fn remove_pipeline(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state.app.pipelines.remove(id).await.map_err(cfg_err)?;
    Ok("ok")
}

#[utoipa::path(get, path = "/api/agents", responses((status = 200, body = [AgentConfigDto])))]
async fn list_agents(
    State(state): State<KanbanStore>,
    _user: AuthUser,
) -> Result<Json<Vec<AgentConfigDto>>, ApiError> {
    let rows = state.app.agents.list().await.map_err(cfg_err)?;
    Ok(Json(rows.into_iter().map(AgentConfigDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/agents",
    request_body = AgentConfigRequest,
    responses((status = 200, body = AgentConfigDto), (status = 400, body = str))
)]
async fn create_agent(
    State(state): State<KanbanStore>,
    user: AuthUser,
    Json(req): Json<AgentConfigRequest>,
) -> Result<Json<AgentConfigDto>, ApiError> {
    require_edit(&user)?;
    let row = state
        .app
        .agents
        .create(AgentConfigDraft {
            name: req.name.trim().to_string(),
            model: req.model.unwrap_or_default(),
            persona: req.persona.unwrap_or_default(),
            prompt: req.prompt.unwrap_or_default(),
            output: req.output.unwrap_or_default(),
        })
        .await
        .map_err(cfg_err)?;
    Ok(Json(AgentConfigDto::from(row)))
}

#[utoipa::path(
    put,
    path = "/api/agents/{id}",
    request_body = AgentConfigRequest,
    responses((status = 200, body = str), (status = 404, body = str), (status = 400, body = str))
)]
async fn update_agent_cfg(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
    Json(req): Json<AgentConfigRequest>,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state
        .app
        .agents
        .update(
            id,
            AgentConfigDraft {
                name: req.name.trim().to_string(),
                model: req.model.unwrap_or_default(),
                persona: req.persona.unwrap_or_default(),
                prompt: req.prompt.unwrap_or_default(),
                output: req.output.unwrap_or_default(),
            },
        )
        .await
        .map_err(cfg_err)?;
    Ok("ok")
}

#[utoipa::path(delete, path = "/api/agents/{id}", responses((status = 200, body = str), (status = 404, body = str)))]
async fn remove_agent_cfg(
    State(state): State<KanbanStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state.app.agents.remove(id).await.map_err(cfg_err)?;
    Ok("ok")
}

fn store_err(e: kanban_rs::StoreError) -> ApiError {
    ApiError::internal(e.to_string())
}

fn workspace_err(e: kanban_rs::StoreError) -> ApiError {
    match e {
        kanban_rs::StoreError::WorkspaceTaken => {
            ApiError::bad_request("workspace name already taken")
        }
        kanban_rs::StoreError::ProjectTaken => ApiError::bad_request("project name already taken"),
        other => ApiError::internal(other.to_string()),
    }
}

fn cfg_err(e: kanban_rs::StoreError) -> ApiError {
    match e {
        kanban_rs::StoreError::NoSuchCard
        | kanban_rs::StoreError::NoSuchPipeline
        | kanban_rs::StoreError::NoSuchAgent => ApiError(ApiError::NOT_FOUND_MSG.to_string(), StatusCode::NOT_FOUND),
        kanban_rs::StoreError::PipelineTaken => {
            ApiError::bad_request("pipeline name already taken")
        }
        kanban_rs::StoreError::AgentTaken => ApiError::bad_request("agent name already taken"),
        kanban_rs::StoreError::BadSpec(msg) => ApiError::bad_request(msg),
        other => ApiError::internal(other.to_string()),
    }
}

fn kanban_err(e: kanban_rs::StoreError) -> ApiError {
    match e {
        kanban_rs::StoreError::NoSuchCard | kanban_rs::StoreError::NoSuchColumn => {
            ApiError(ApiError::NOT_FOUND_MSG.to_string(), StatusCode::NOT_FOUND)
        }
        other => ApiError::internal(other.to_string()),
    }
}

async fn not_found(_req: Request) -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

#[derive(Serialize, utoipa::ToSchema)]
struct SandboxDirInfo {
    pid: u32,
    alive: bool,
    path: String,
}

#[utoipa::path(
    get,
    path = "/api/sandbox",
    responses((status = 200, body = [SandboxDirInfo]))
)]
async fn list_sandboxes() -> Json<Vec<SandboxDirInfo>> {
    Json(
        AgentSandbox::dirs()
            .iter()
            .map(|d| SandboxDirInfo {
                pid: d.pid,
                alive: d.alive,
                path: d.path.display().to_string(),
            })
            .collect(),
    )
}

#[utoipa::path(
    post,
    path = "/api/sandbox/purge",
    request_body = SandboxPurgeRequest,
    responses((status = 200, body = SandboxSweepReply), (status = 400, body = str))
)]
async fn purge_sandbox(
    Json(req): Json<SandboxPurgeRequest>,
) -> Result<Json<SandboxSweepReply>, ApiError> {
    let removed = AgentSandbox::purge_dir(req.pid).map_err(ApiError::bad_request)?;
    Ok(Json(SandboxSweepReply {
        removed: removed as usize,
    }))
}

#[utoipa::path(
    post,
    path = "/api/sandbox/sweep",
    responses((status = 200, body = SandboxSweepReply))
)]
async fn sweep_sandboxes() -> Json<SandboxSweepReply> {
    Json(SandboxSweepReply {
        removed: AgentSandbox::sweep(),
    })
}

#[derive(Serialize)]
pub struct AgentListReply {
    agents: Vec<serde_json::Value>,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct AgentSpawnRequest {
    agent: String,
}

#[derive(Serialize, utoipa::ToSchema)]
struct AgentSpawnReply {
    agent: String,
    work_tree: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct AgentRunRequest {
    cmd: String,
}

#[derive(Serialize, utoipa::ToSchema)]
struct AgentRunReply {
    agent: String,
    output: String,
}

async fn manager_list_agents(
    State(manager): State<Arc<manager_rs::ManagerProcess>>,
) -> Json<AgentListReply> {
    Json(AgentListReply {
        agents: manager
            .snapshot()
            .iter()
            .map(|a| serde_json::to_value(a).unwrap_or_default())
            .collect(),
    })
}

async fn spawn_agent(
    State(manager): State<Arc<manager_rs::ManagerProcess>>,
    Json(req): Json<AgentSpawnRequest>,
) -> Result<Json<AgentSpawnReply>, ApiError> {
    let work_tree = manager.spawn(&req.agent).map_err(ApiError::bad_request)?;
    Ok(Json(AgentSpawnReply {
        agent: req.agent,
        work_tree: work_tree.display().to_string(),
    }))
}

async fn run_agent_command(
    State(manager): State<Arc<manager_rs::ManagerProcess>>,
    Path(agent): Path<String>,
    Json(req): Json<AgentRunRequest>,
) -> Result<Json<AgentRunReply>, ApiError> {
    let manager = Arc::clone(&manager);
    let cmd_agent = agent.clone();
    let output = tokio::task::spawn_blocking(move || manager.run(&cmd_agent, &req.cmd))
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
        .map_err(ApiError::bad_request)?;
    Ok(Json(AgentRunReply { agent, output }))
}

async fn finish_agent(
    State(manager): State<Arc<manager_rs::ManagerProcess>>,
    Path(agent): Path<String>,
) -> Result<Json<manager_rs::TaskOutcome>, ApiError> {
    let outcome = tokio::task::spawn_blocking(move || manager.finish(&agent))
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
        .map_err(ApiError::bad_request)?;
    Ok(Json(outcome))
}

/// Installer for remote sandbox clients; `role` selects auto/model/worker.
/// Unauthenticated by design — it only probes the downloading machine.
async fn install_script(
    axum::extract::Query(q): axum::extract::Query<crate::infra::install::InstallQuery>,
) -> Response {
    use crate::infra::install;
    let role = match q.role.as_deref().map(install::parse_role) {
        None => install::Role::default(),
        Some(Some(r)) => r,
        Some(None) => {
            return (
                StatusCode::BAD_REQUEST,
                "invalid role: use auto | model | worker",
            )
                .into_response();
        }
    };
    let server = q
        .server
        .unwrap_or_else(|| "http://localhost:3334".to_owned());
    (
        [
            (
                http::header::CONTENT_TYPE,
                "text/x-shellscript; charset=utf-8",
            ),
            (
                http::header::HeaderName::from_static("content-disposition"),
                "attachment; filename=\"install.sh\"",
            ),
        ],
        install::render_install_script(role, &server),
    )
        .into_response()
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
        claude_start,
        claude_callback,
        claude_status,
        chat_claude,
        get_zai_settings,
        set_zai_settings,
        get_system_prompt,
        set_system_prompt,
        get_client_env,
        set_client_env,
        chat_zai,
        render_prompt,
        list_sandboxes,
        purge_sandbox,
        sweep_sandboxes,
        list_cards,
        create_card,
        update_card,
        move_card,
        remove_card,
        get_agent,
        set_agent,
        run_card,
        set_card_schedule,
        list_cronjobs,
        list_comments,
        add_comment,
        login,
        logout,
        bootstrap,
        list_users,
        create_user,
        list_workspaces,
        create_workspace,
        delete_workspace,
        list_projects,
        create_project,
        delete_project,
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
        ClaudeCodeRequest,
        ClaudeChatRequest,
        ZaiSettingsReply,
        ZaiSettingsRequest,
        ZaiModelReply,
        ZaiModelAction,
        SystemPromptReply,
        SystemPromptRequest,
        ClientEnvReply,
        ClientEnvRequest,
        ZaiChatRequest,
        PromptSectionDto,
        RenderPromptRequest,
        RenderPromptReply,
        SandboxDirInfo,
        SandboxPurgeRequest,
        SandboxSweepReply,
        CardDto,
        AgentDto,
        crate::app::pipeline_run::RunRecord,
        crate::app::pipeline_run::RunStage,
        crate::app::pipeline_run::StageStatus,
        CreateCardRequest,
        ListCardsQuery,
        MoveCardRequest,
        SetAgentRequest,
        SetScheduleRequest,
        CronJobDto,
        crate::app::schedule_work::CronJobState,
        UpdateCardRequest,
        CommentDto,
        AddCommentRequest,
        LoginRequest,
        LoginReply,
        ChangePasswordRequest,
        BootstrapReply,
        CreateUserRequest,
        UserDto,
        WorkspaceDto,
        ProjectDto,
        CreateWorkspaceRequest,
        CreateProjectRequest,
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
    engine: String,
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
    models: Vec<ZaiModelReply>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct ZaiModelReply {
    model: String,
    api_key_set: bool,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ZaiModelAction {
    /// Create a model entry; it must carry its own api key.
    Add {
        model: String,
        api_key: String,
    },
    /// Replace the stored key of an existing model.
    SetKey {
        model: String,
        api_key: String,
    },
    Remove {
        model: String,
    },
    SetActive {
        model: String,
    },
}

#[derive(Deserialize, utoipa::ToSchema)]
struct ZaiSettingsRequest {
    /// Omit or send empty to keep the saved key; the saved key is never returned.
    api_key: Option<String>,
    model: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct ClientEnvReply {
    reported: bool,
    user_agent: String,
    platform: String,
    language: String,
    timezone: String,
    screen: String,
    workspace_path: String,
    hostname: String,
    os: String,
    arch: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct ClientEnvRequest {
    user_agent: String,
    platform: String,
    language: String,
    timezone: String,
    screen: String,
    workspace_path: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct ZaiChatRequest {
    message: String,
    model: Option<String>,
    system: Option<Vec<PromptSectionDto>>,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct PromptSectionDto {
    role: String,
    body: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct RenderPromptRequest {
    sections: Vec<PromptSectionDto>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct RenderPromptReply {
    rendered: String,
    chars: usize,
    max_chars: usize,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct SandboxPurgeRequest {
    pid: u32,
}

#[derive(Serialize, utoipa::ToSchema)]
struct SandboxSweepReply {
    removed: usize,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct CreateCardRequest {
    column_id: String,
    title: String,
    description: Option<String>,
    /// "low" | "normal" | "high" | "critical" (default: normal).
    priority: Option<String>,
    /// Owning project — a task lives in exactly one project.
    project_id: Option<i64>,
}

#[derive(Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
struct ListCardsQuery {
    /// Only cards of this project; omit for all cards.
    project_id: Option<i64>,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct MoveCardRequest {
    column_id: String,
    position: i32,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct SetAgentRequest {
    name: String,
    /// Arbitrary JSON state saved with the agent on the card.
    state: Option<serde_json::Value>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct CardDto {
    id: i64,
    column_id: String,
    project_id: Option<i64>,
    title: String,
    description: String,
    priority: String,
    position: i32,
    agent_name: Option<String>,
    /// JSON-encoded agent state, if an agent is attached.
    agent_state: Option<serde_json::Value>,
    assignee: Option<String>,
    pipeline_id: Option<i64>,
    pipeline_name: Option<String>,
    cron: Option<String>,
}

impl From<kanban_rs::CardRow> for CardDto {
    fn from(r: kanban_rs::CardRow) -> Self {
        Self {
            id: r.id,
            column_id: r.column_id,
            project_id: r.project_id,
            title: r.title,
            description: r.description,
            priority: r.priority,
            position: r.position,
            agent_name: r.agent_name,
            agent_state: r.agent_state.and_then(|s| serde_json::from_str(&s).ok()),
            assignee: r.assignee,
            pipeline_id: r.pipeline_id,
            pipeline_name: None,
            cron: r.cron,
        }
    }
}

impl From<CardView> for CardDto {
    fn from(v: CardView) -> Self {
        let mut dto = CardDto::from(v.card);
        dto.pipeline_name = v.pipeline_name;
        dto
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
struct UpdateCardRequest {
    title: String,
    description: String,
    /// Person assigned to this card; null to unassign.
    assignee: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct CommentDto {
    id: i64,
    card_id: i64,
    author: String,
    body: String,
    created_at: String,
}

impl From<kanban_rs::CommentRow> for CommentDto {
    fn from(c: kanban_rs::CommentRow) -> Self {
        Self {
            id: c.id,
            card_id: c.card_id,
            author: c.author,
            body: c.body,
            created_at: c.created_at.to_rfc3339(),
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
struct AddCommentRequest {
    body: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct SetCardPipelineRequest {
    /// Pipeline to run on this card; null to unassign.
    pipeline_id: Option<i64>,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct SetScheduleRequest {
    /// 5-field cron expression (UTC), or null to unschedule the card.
    cron: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct SavePipelineRequest {
    name: String,
    /// piplines::graph::PipelineSpec: {nodes: [{id, stage, params}], links: [{from, to}]}
    spec: serde_json::Value,
}

#[derive(Serialize, utoipa::ToSchema)]
struct PipelineDto {
    id: i64,
    name: String,
    spec: serde_json::Value,
}

impl From<kanban_rs::PipelineRow> for PipelineDto {
    fn from(r: kanban_rs::PipelineRow) -> Self {
        let spec = serde_json::from_str(&r.spec).unwrap_or(serde_json::Value::Null);
        Self {
            id: r.id,
            name: r.name,
            spec,
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
struct AgentConfigRequest {
    name: String,
    model: Option<String>,
    persona: Option<String>,
    prompt: Option<String>,
    output: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct AgentConfigDto {
    id: i64,
    name: String,
    model: String,
    persona: String,
    prompt: String,
    output: String,
}

impl From<kanban_rs::AgentConfigRow> for AgentConfigDto {
    fn from(r: kanban_rs::AgentConfigRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            model: r.model,
            persona: r.persona,
            prompt: r.prompt,
            output: r.output,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
struct AgentDto {
    name: String,
    state: serde_json::Value,
}

impl From<kanban_rs::AgentState> for AgentDto {
    fn from(a: kanban_rs::AgentState) -> Self {
        Self {
            name: a.name,
            state: a.state,
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize, utoipa::ToSchema)]
struct LoginReply {
    token: String,
    username: String,
    /// "owner" | "super_admin" | "admin" | "editor" | "viewer"
    role: String,
    /// True when the account must change its password before any other call.
    must_change_password: bool,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct ChangePasswordRequest {
    old_password: String,
    new_password: String,
}

#[derive(Serialize, utoipa::ToSchema)]
struct BootstrapReply {
    needs_setup: bool,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct CreateUserRequest {
    username: String,
    /// Minimum 8 chars — hashed with argon2, never stored in plain text.
    password: String,
    /// "owner" | "super_admin" | "admin" | "editor" | "viewer"
    role: String,
}

#[derive(Serialize, utoipa::ToSchema)]
struct UserDto {
    id: i64,
    username: String,
    role: String,
    must_change_password: bool,
}

impl From<kanban_rs::UserRow> for UserDto {
    fn from(u: kanban_rs::UserRow) -> Self {
        Self {
            id: u.id,
            username: u.username,
            role: u.role,
            must_change_password: u.must_change_password,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
struct WorkspaceDto {
    id: i64,
    name: String,
}

impl From<kanban_rs::WorkspaceRow> for WorkspaceDto {
    fn from(w: kanban_rs::WorkspaceRow) -> Self {
        Self {
            id: w.id,
            name: w.name,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
struct ProjectDto {
    id: i64,
    workspace_id: i64,
    name: String,
}

impl From<kanban_rs::ProjectRow> for ProjectDto {
    fn from(p: kanban_rs::ProjectRow) -> Self {
        Self {
            id: p.id,
            workspace_id: p.workspace_id,
            name: p.name,
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
struct CreateWorkspaceRequest {
    name: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct CreateProjectRequest {
    name: String,
}

struct ApiError(String, StatusCode);

impl ApiError {
    const NOT_FOUND_MSG: &'static str = "not found";

    fn internal(msg: impl Into<String>) -> Self {
        Self(msg.into(), StatusCode::INTERNAL_SERVER_ERROR)
    }

    fn bad_request(msg: impl Into<String>) -> Self {
        Self(msg.into(), StatusCode::BAD_REQUEST)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let Self(msg, status) = self;
        let status = match &*msg {
            m if m.starts_with("no loadable model") => StatusCode::NOT_FOUND,
            m if m == Self::NOT_FOUND_MSG => StatusCode::NOT_FOUND,
            m if m == UNAUTHORIZED_MSG => StatusCode::UNAUTHORIZED,
            m if m == FORBIDDEN_MSG => StatusCode::FORBIDDEN,
            _ => status,
        };
        (status, msg).into_response()
    }
}
