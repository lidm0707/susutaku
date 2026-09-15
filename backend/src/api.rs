//! API layer: HTTP transport only. Maps DTOs to the chat use case.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, Extension, FromRequestParts, Multipart, Path, Query, Request, State,
        ws::{Message, WebSocketUpgrade},
    },
    http::{self, StatusCode, request::Parts},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::wrappers::ReceiverStream;
use utoipa::OpenApi;

use crate::app::task::TaskApp;
use crate::app::{BoardService, ChatUseCase};
use crate::domain::{
    AgentConfigDraft, CancelFlag, CardMove, CardPatch, ChatCmd, NewCard, NewProject, NewWorkspace,
    ResourceService, SearchMode, TOOL_KIND_NAMES, ToolEvent, ToolSet, ToolUse,
};
use crate::infra::claude::auth::{ClaudeAuth, LoginStatus as ClaudeLoginStatus};
use crate::infra::claude::chat as claude_chat;
use crate::infra::client::host_spec::{self, HostSpec};
use crate::infra::codex::auth::{CodexAuth, LoginStatus};
use crate::infra::codex::chat as codex_chat;
use crate::infra::podman::AgentSandbox;
use crate::infra::provider_quota::QuotaBoard;
use crate::infra::search::{DuckDuckGo, PageFetcher};

use crate::infra::zai::chat::{STREAM_EVENT_CAPACITY, StreamEvent, ZaiEngine};
use crate::infra::zai::settings::{SettingsState, ZaiSettings};
use crate::port::inbound::ChatHandling;
use crate::port::outbound::{
    AgentConfigRepo, AgentGit, AgentRun, BoardOps, ChatMemory, Fetcher, Inference, ModelEndpoint,
    ModelSwitch, ProjectGit, Runner, Searcher, ThreadEnvs,
};
use prompt_sys::{MAX_PROMPT_CHARS, PromptBuilder, Role as PromptRole};
use proto_rs::AgentBrief;
use proto_rs::GitTool;
use std::path::PathBuf;
use susutaku_mlx::tok::TokKind;

const DEFAULT_MAX_TOKENS: usize = 512;

/// Shared deps for the Z.ai cloud chat path: same tool loop as the local chat,
/// engine built per request (the UI picks the model per request).
struct ZaiChatDeps {
    searcher: Arc<dyn Searcher>,
    fetcher: Arc<dyn Fetcher>,
    runner: Arc<dyn Runner>,
    models: Arc<dyn ModelSwitch>,
    memory: Option<Arc<dyn ChatMemory>>,
    board: Arc<dyn BoardOps>,
    agents: Arc<dyn AgentConfigRepo>,
    settings: Arc<SettingsState>,
    store: std::sync::Arc<task_rs::Store>,
    agent_git: Arc<dyn AgentGit>,
    agent_run: Arc<dyn AgentRun>,
    thread_envs: Arc<dyn ThreadEnvs>,
    project_git: Arc<dyn ProjectGit>,
    /// Persists tool artifacts onto the turn's target card.
    resources: Arc<ResourceService>,
    /// Live chat runs keyed by client run id, for cancellation.
    cancels: CancelRegistry,
}

/// Registry of in-flight chat runs: run id → cancel flag.
type CancelRegistry = Arc<std::sync::RwLock<std::collections::HashMap<String, CancelFlag>>>;

/// Name of the auto-seeded project skill embedded into agent prompts.
pub const PROJECT_SKILL_NAME: &str = "susutaku-project";
const PROJECT_SKILL_MD: &str = include_str!("../../.agents/skills/susutaku-project/SKILL.md");

/// Seeds (and refreshes) the project skill, then attaches it to every agent
/// that does not have it yet, so every chat embeds the project skill sheet.
pub async fn seed_project_skill(store: &task_rs::Store) {
    let body = strip_frontmatter(PROJECT_SKILL_MD);
    let skill_id = match store.list_skills().await {
        Ok(rows) => match rows.iter().find(|s| s.name == PROJECT_SKILL_NAME) {
            Some(row) => {
                if row.body != body {
                    let _ = store.update_skill(row.id, &body).await;
                }
                row.id
            }
            None => match store
                .create_skill(task_rs::NewSkill {
                    name: PROJECT_SKILL_NAME,
                    body: &body,
                })
                .await
            {
                Ok(row) => row.id,
                Err(e) => {
                    tracing::warn!(error = %e, "project skill seed failed");
                    return;
                }
            },
        },
        Err(e) => {
            tracing::warn!(error = %e, "project skill seed failed");
            return;
        }
    };
    for agent in store.list_agents().await.into_iter().flatten() {
        let has = store
            .list_agent_skills(agent.id)
            .await
            .map(|rows| rows.iter().any(|s| s.id == skill_id))
            .unwrap_or(true);
        if !has {
            let _ = store.attach_agent_skill(agent.id, skill_id).await;
        }
    }
}

/// Drops a leading YAML frontmatter block (`---\n...\n---`).
fn strip_frontmatter(md: &str) -> String {
    match md.strip_prefix("---\n") {
        Some(rest) => match rest.split_once("\n---") {
            Some((_, after)) => after.trim_start_matches('\n').trim_start().to_owned(),
            None => md.to_owned(),
        },
        None => md.to_owned(),
    }
}

fn zai_chat_deps<T: ChatHandling + ModelSwitch + 'static>(
    models: Arc<T>,
    runner: Arc<dyn Runner>,
    task_store: std::sync::Arc<task_rs::Store>,
    settings: Arc<SettingsState>,
    manager: Arc<manager_rs::manager::Manager>,
) -> Arc<ZaiChatDeps> {
    let task_store_for_resources = task_store.clone();
    let engine = models.inference();
    Arc::new(ZaiChatDeps {
        searcher: Arc::new(DuckDuckGo),
        fetcher: Arc::new(PageFetcher),
        runner,
        models: models as Arc<dyn ModelSwitch>,
        memory: crate::infra::chat_memory::from_env().map(|m| Arc::new(m) as Arc<dyn ChatMemory>),
        board: Arc::new(BoardService::new(task_store.clone(), engine)),
        agents: Arc::new(crate::infra::postgres::task::PgTask::new(
            task_store.clone(),
        )),
        settings,
        project_git: Arc::new(crate::infra::project_git::SettingsProjectGit::new(
            task_store.clone(),
        )),
        store: task_store,
        agent_git: Arc::new(crate::infra::manager_git::ManagerGit::new(manager.clone())),
        agent_run: Arc::new(crate::infra::manager_run::ManagerRun::new(manager)),
        thread_envs: Arc::new(crate::infra::thread_env::ThreadEnvManager::new()),
        resources: Arc::new(ResourceService::new(Arc::new(
            crate::infra::postgres::task::PgTask::new(task_store_for_resources),
        ))),
        cancels: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn router<T: ChatHandling + ModelSwitch + 'static>(
    use_case: Arc<T>,
    catalog: Arc<dyn crate::infra::model_client::ModelCatalog>,
    codex_workspace: PathBuf,
    runner: Arc<dyn Runner>,
    task_store: std::sync::Arc<task_rs::Store>,
    usage_store: std::sync::Arc<codex_usage_rs::Store>,
    manager: Arc<manager_rs::manager::Manager>,
    model_cfg: Arc<dyn ModelEndpoint>,
) -> Router {
    let task_store_for_sched = task_store.clone();
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
    let settings_state = Arc::new(SettingsState::load());
    let manager_router = Router::new()
        .route(
            "/api/manager/agents",
            get(manager_list_agents).post(spawn_agent),
        )
        .route("/api/manager/agents/{agent}/run", post(run_agent_command))
        .route("/api/manager/agents/{agent}/git", post(run_agent_git))
        .route("/api/manager/agents/{agent}/logs", get(agent_logs))
        .route("/api/manager/agents/{agent}/finish", post(finish_agent))
        .route(
            "/api/manager/agents/{agent}/token",
            post(issue_agent_run_token),
        )
        .with_state(ManagerState {
            manager: manager.clone(),
            store: task_store.clone(),
            settings: settings_state.clone(),
        });
    let core = core.merge(manager_router);
    let machines_router = Router::new()
        .route("/api/machines", get(machines))
        .route("/api/machines/{hostname}/kick", post(kick_machine))
        .route(
            "/api/machines/{hostname}/agents/{agent}/run",
            post(run_machine_agent),
        )
        .route(
            "/api/machines/{hostname}/agents/{agent}/git",
            post(run_machine_git),
        )
        .route("/api/agents/whereis/{agent}", get(agent_whereis))
        .with_state(MachinesState {
            manager: manager.clone(),
        });
    let core = core.merge(machines_router);
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
    crate::app::say_hi::spawn(settings_state.clone());
    let zai_deps = zai_chat_deps(
        use_case.clone(),
        runner,
        task_store.clone(),
        settings_state.clone(),
        manager.clone(),
    );
    let settings = Router::new()
        .route(
            "/api/settings/zai",
            get(get_zai_settings).post(set_zai_settings),
        )
        .route("/api/settings/zai/models", post(zai_model_action))
        .route("/api/settings/zai/quota", get(zai_quota))
        .route("/api/settings/zai/hi", post(zai_hi))
        .route(
            "/api/settings/client-env",
            get(get_client_env).post(set_client_env),
        )
        .route(
            "/api/settings/alerts",
            get(get_alert_settings).post(set_alert_settings),
        )
        .route("/api/settings/git/repos", get(list_git_repos))
        .route(
            "/api/settings/git/repos/{project_id}",
            put(set_git_repo).delete(remove_git_repo),
        )
        .route(
            "/api/settings/system-prompt",
            get(get_system_prompt).post(set_system_prompt),
        )
        .route(
            "/api/chat/zai",
            post(chat_zai).layer(DefaultBodyLimit::max(ATTACHMENT_MAX_BYTES)),
        )
        .route(
            "/api/chat/zai/stream",
            post(chat_zai_stream).layer(DefaultBodyLimit::max(ATTACHMENT_MAX_BYTES)),
        )
        .route("/api/chat/zai/cancel", post(chat_zai_cancel))
        .route(
            "/api/chat/threads",
            get(list_chat_threads).post(create_chat_thread),
        )
        .route(
            "/api/chat/threads/{id}",
            get(list_chat_messages).delete(delete_chat_thread),
        )
        .route("/api/sandbox", get(list_sandboxes))
        .route("/api/sandbox/logs", get(sandbox_logs))
        .route("/api/host", get(host_spec_handler))
        .route("/api/sandbox/purge", post(purge_sandbox))
        .route("/api/sandbox/sweep", post(sweep_sandboxes))
        .route("/install.sh", get(install_script))
        .with_state(settings_state.clone())
        .layer(Extension(zai_deps));
    let local_settings_router = Router::new()
        .route(
            "/api/settings/local",
            get(get_local_endpoint).put(set_local_endpoint),
        )
        .with_state(Arc::new(LocalSettingsState {
            settings: Arc::new(SettingsState::load()),
            model: model_cfg,
        }));
    let usage = Router::new()
        .route("/api/codex/usage/latest", get(codex_usage_latest))
        .route("/api/codex/usage/history", get(codex_usage_history))
        .with_state(usage_store.clone());
    let quota = Router::new()
        .route("/api/quota", get(quota_board))
        .with_state((settings_state.clone(), usage_store));
    let sched = std::sync::Arc::new(crate::app::schedule_work::spawn(
        std::sync::Arc::new(crate::app::task::build(task_store_for_sched)),
        use_case.inference(),
    ));
    let engine = use_case.inference();
    core.merge(auth)
        .merge(claude)
        .merge(settings)
        .merge(local_settings_router)
        .merge(usage)
        .merge(quota)
        .merge(task_router(task_state(task_store, sched, engine)))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(Extension(manager))
}

fn task_router(state: TaskStore) -> Router {
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
        .route("/api/projects/{id}/agents", get(project_agents))
        .route("/api/agent-results", post(agent_results))
        .route("/api/task/cards", get(list_cards).post(create_card))
        .route(
            "/api/task/cards/{id}",
            delete(remove_card).put(update_card),
        )
        .route("/api/task/cards/{id}/move", post(move_card))
        .route(
            "/api/task/cards/{id}/comments",
            get(list_comments).post(add_comment),
        )
        .route(
            "/api/task/cards/{id}/agent",
            get(get_agent).put(set_agent),
        )
        .route("/api/task/cards/{id}/runs", get(list_card_runs))
        .route("/api/task/cards/{id}/resources", get(list_card_resources))
        .route("/api/task/cards/{id}/image", put(set_card_image))
        .route("/api/task/cards/{id}/run", post(run_card))
        .route("/api/task/cards/{id}/schedule", put(set_card_schedule))
        .route("/api/cronjobs", get(list_cronjobs))
        .route("/api/activity", get(list_activity))
        .route("/api/events", get(events_ws))
        .route(
            "/api/attachments",
            get(list_attachments)
                .post(upload_attachment)
                .delete(delete_attachment)
                .layer(DefaultBodyLimit::max(ATTACHMENT_MAX_BYTES)),
        )
        .route("/api/attachments/file", get(serve_attachment))
        .route("/api/agents", get(list_agents).post(create_agent))
        .route(
            "/api/agents/{id}",
            put(update_agent_cfg).delete(remove_agent_cfg),
        )
        .route("/api/skills", get(list_skills).post(create_skill))
        .route("/api/skills/{id}", put(update_skill).delete(remove_skill))
        .route(
            "/api/agents/{id}/skills",
            get(list_agent_skills).post(attach_agent_skill),
        )
        .route(
            "/api/agents/{id}/skills/{skill_id}",
            delete(detach_agent_skill),
        )
        .route("/api/agent-outputs", get(list_agent_outputs_handler))
        .route("/api/agent-outputs/{id}", get(get_agent_output_handler))
        .route(
            "/api/agent-outputs/{id}/status",
            post(set_agent_output_status_handler),
        )
        .with_state(state)
}

/// Bearer-token auth extractor: resolves the session to a user.
struct AuthUser(task_rs::UserRow);

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
const ATTACHMENT_PATH_QUERY: &str = "path";

#[derive(serde::Serialize, utoipa::ToSchema)]
struct AttachmentCardRef {
    id: i64,
    title: String,
    project_id: Option<i64>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
struct AttachmentInfo {
    path: String,
    name: String,
    size: i64,
    modified: Option<String>,
    cards: Vec<AttachmentCardRef>,
}

const ATTACHMENT_CHAT_PREFIX: &str = "chat-";
const DATA_URL_PREFIX: &str = "data:";

/// Persist a chat image (data URL) into the attachments tree so it shows up
/// on the attachments page. Best-effort: a failure is logged and ignored.
fn store_chat_image(data_url: &str) {
    const B64_SEP: &str = ";base64,";
    let Some((head, data)) = data_url.split_once(B64_SEP) else {
        return;
    };
    let mime = head.strip_prefix(DATA_URL_PREFIX).unwrap_or("image/png");
    use base64::Engine as _;
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
        return;
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let ext = match mime {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png",
    };
    let dir = format!("{ATTACHMENTS_DIR}/{ATTACHMENT_UPLOAD_PREFIX}{nanos}");
    let path = format!("{dir}/{ATTACHMENT_CHAT_PREFIX}{nanos}.{ext}");
    tokio::spawn(async move {
        if tokio::fs::create_dir_all(&dir).await.is_err() {
            return;
        }
        if tokio::fs::write(&path, &bytes).await.is_ok() {
            crate::app::events::publish(crate::app::events::EventKind::Attachment);
        } else {
            tracing::warn!("failed to store chat image {path}");
        }
    });
}

/// A card "holds" an attachment when the attachment path appears in the
/// card title, description, or agent state JSON.
fn cards_holding(path: &str, cards: &[task_rs::CardRow]) -> Vec<AttachmentCardRef> {
    cards
        .iter()
        .filter(|c| {
            c.title.contains(path)
                || c.description.contains(path)
                || c.agent_state.as_deref().is_some_and(|s| s.contains(path))
        })
        .map(|c| AttachmentCardRef {
            id: c.id,
            title: c.title.clone(),
            project_id: c.project_id,
        })
        .collect()
}

/// Reject paths outside the uploads tree or containing traversal segments.
fn safe_attachment_path(raw: &str) -> Option<String> {
    let prefix = format!("{ATTACHMENTS_DIR}/{ATTACHMENT_UPLOAD_PREFIX}");
    if !raw.starts_with(&prefix) || raw.split('/').any(|seg| seg == ".." || seg.is_empty()) {
        return None;
    }
    Some(raw.to_string())
}

fn attachment_mime(name: &str) -> &'static str {
    const MIME_PNG: &str = "image/png";
    const MIME_JPEG: &str = "image/jpeg";
    const MIME_GIF: &str = "image/gif";
    const MIME_WEBP: &str = "image/webp";
    const MIME_PDF: &str = "application/pdf";
    const MIME_TEXT: &str = "text/plain; charset=utf-8";
    const MIME_OCTET: &str = "application/octet-stream";
    let ext = name.rsplit('.').next().unwrap_or("");
    match ext {
        "png" => MIME_PNG,
        "jpg" | "jpeg" => MIME_JPEG,
        "gif" => MIME_GIF,
        "webp" => MIME_WEBP,
        "pdf" => MIME_PDF,
        "txt" | "md" | "csv" | "json" | "log" => MIME_TEXT,
        _ => MIME_OCTET,
    }
}

#[utoipa::path(
    get,
    path = "/api/attachments",
    responses((status = 200, body = [AttachmentInfo]), (status = 403, body = str))
)]
async fn list_attachments(
    State(state): State<TaskStore>,
    user: AuthUser,
) -> Result<Json<Vec<AttachmentInfo>>, ApiError> {
    require_edit(&user)?;
    let cards = state
        .store
        .list(None)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let mut out = Vec::new();
    let root = std::path::Path::new(ATTACHMENTS_DIR);
    if !root.exists() {
        return Ok(Json(out));
    }
    let mut dirs = tokio::fs::read_dir(root)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    while let Some(dir) = dirs
        .next_entry()
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
    {
        if !dir.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let mut files = tokio::fs::read_dir(dir.path())
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
        while let Some(file) = files
            .next_entry()
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?
        {
            if !file.file_type().await.map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let path = file
                .path()
                .to_string_lossy()
                .trim_start_matches("./")
                .to_string();
            let meta = file
                .metadata()
                .await
                .map_err(|e| ApiError::internal(e.to_string()))?;
            let modified = meta.modified().ok().and_then(|t| {
                let secs = t
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or_default();
                chrono::DateTime::from_timestamp_secs(secs as i64).map(|dt| dt.to_rfc3339())
            });
            out.push(AttachmentInfo {
                cards: cards_holding(&path, &cards),
                name: file.file_name().to_string_lossy().to_string(),
                path,
                size: meta.len() as i64,
                modified,
            });
        }
    }
    out.sort_by(|a, b| b.path.cmp(&a.path));
    Ok(Json(out))
}

#[utoipa::path(
    delete,
    path = "/api/attachments",
    responses((status = 200), (status = 400, body = str), (status = 403, body = str), (status = 404, body = str))
)]
async fn delete_attachment(
    State(_state): State<TaskStore>,
    user: AuthUser,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<StatusCode, ApiError> {
    require_edit(&user)?;
    let raw = params
        .get(ATTACHMENT_PATH_QUERY)
        .ok_or_else(|| ApiError::bad_request("missing path"))?
        .clone();
    let path = safe_attachment_path(&raw)
        .ok_or_else(|| ApiError::bad_request("invalid attachment path"))?;
    tokio::fs::remove_file(&path)
        .await
        .map_err(|_| ApiError::not_found(ApiError::NOT_FOUND_MSG))?;
    if let Some(parent) = std::path::Path::new(&path).parent() {
        let _ = tokio::fs::remove_dir(parent).await;
    }
    crate::app::events::publish(crate::app::events::EventKind::Attachment);
    Ok(StatusCode::OK)
}

#[utoipa::path(
    get,
    path = "/api/attachments/file",
    params(("path" = String, Query)),
    responses((status = 200, body = Vec<u8>), (status = 400, body = str), (status = 403, body = str), (status = 404, body = str))
)]
async fn serve_attachment(
    _user: AuthUser,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let raw = params
        .get(ATTACHMENT_PATH_QUERY)
        .ok_or_else(|| ApiError::bad_request("missing path"))?
        .clone();
    let path = safe_attachment_path(&raw)
        .ok_or_else(|| ApiError::bad_request("invalid attachment path"))?;
    let data = tokio::fs::read(&path)
        .await
        .map_err(|_| ApiError::not_found(ApiError::NOT_FOUND_MSG))?;
    let name = std::path::Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| ATTACHMENT_DEFAULT_NAME.to_string());
    Ok((
        [
            (http::header::CONTENT_TYPE, attachment_mime(&name)),
            (http::header::CONTENT_DISPOSITION, "inline"),
        ],
        data,
    )
        .into_response())
}

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
    State(_state): State<TaskStore>,
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
        let data = field
            .bytes()
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
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
        crate::app::events::publish(crate::app::events::EventKind::Attachment);
        return Ok(Json(UploadReply { path }));
    }
    Err(ApiError::bad_request("missing upload field"))
}

impl FromRequestParts<TaskStore> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        store: &TaskStore,
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

/// Bearer-token auth extractor for agents: resolves a run ticket.
struct AuthAgent(task_rs::AgentRunTokenRow);

impl FromRequestParts<TaskStore> for AuthAgent {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        store: &TaskStore,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix(BEARER_PREFIX))
            .filter(|t| t.starts_with(task_rs::TOKEN_PREFIX))
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, UNAUTHORIZED_MSG).into_response())?;
        let row = store
            .store
            .resolve_agent_token(token)
            .await
            .map_err(|e| ApiError::internal(e.to_string()).into_response())?
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, UNAUTHORIZED_MSG).into_response())?;
        Ok(AuthAgent(row))
    }
}

/// Validates the per-agent tool allow-list (search|fetch|shell|board).
fn valid_tools(names: &[String]) -> Result<Vec<String>, ApiError> {
    ToolSet::from_names(names)
        .map(|_| names.to_vec())
        .map_err(ApiError::bad_request)
}

async fn auth_from_headers(
    state: &TaskStore,
    headers: &http::HeaderMap,
) -> Result<AuthUser, ApiError> {
    let token = headers
        .get(http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix(BEARER_PREFIX))
        .ok_or_else(|| ApiError(UNAUTHORIZED_MSG.to_string(), StatusCode::UNAUTHORIZED))?;
    state
        .store
        .auth(token)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map(AuthUser)
        .ok_or_else(|| ApiError(UNAUTHORIZED_MSG.to_string(), StatusCode::UNAUTHORIZED))
}

fn require_users(user: &AuthUser) -> Result<(), ApiError> {
    let role = parse_role(&user.0.role)?;
    role.can_manage_users()
        .then_some(())
        .ok_or_else(|| ApiError(FORBIDDEN_MSG.to_string(), StatusCode::FORBIDDEN))
}

fn parse_role(role: &str) -> Result<task_rs::Role, ApiError> {
    role.parse()
        .map_err(|e: task_rs::StoreError| ApiError::internal(e.to_string()))
}

#[utoipa::path(
    post,
    path = "/api/auth/login",
    request_body = LoginRequest,
    responses((status = 200, body = LoginReply), (status = 401, body = str))
)]
async fn login(
    State(state): State<TaskStore>,
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
    let user = user.ok_or(ApiError(
        UNAUTHORIZED_MSG.to_string(),
        StatusCode::UNAUTHORIZED,
    ))?;
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
    State(state): State<TaskStore>,
    user: AuthUser,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<&'static str, ApiError> {
    state
        .store
        .change_password(user.0.id, &req.old_password, &req.new_password)
        .await
        .map_err(|e| match e {
            task_rs::StoreError::PasswordTooShort | task_rs::StoreError::BadCredentials => {
                ApiError::bad_request(e.to_string())
            }
            other => ApiError::internal(other.to_string()),
        })?;
    Ok("ok")
}

#[utoipa::path(post, path = "/api/auth/logout", responses((status = 200, body = str)))]
async fn logout(
    State(state): State<TaskStore>,
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
    State(state): State<TaskStore>,
    _user: AuthUser,
) -> Result<Json<BootstrapReply>, ApiError> {
    let count = state.store.user_count().await.map_err(store_err)?;
    Ok(Json(BootstrapReply {
        needs_setup: count == 0,
    }))
}

#[utoipa::path(get, path = "/api/auth/users", responses((status = 200, body = [UserDto]), (status = 403, body = str)))]
async fn list_users(
    State(state): State<TaskStore>,
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
    State(state): State<TaskStore>,
    headers: http::HeaderMap,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<UserDto>, ApiError> {
    // Bootstrap: with zero users, an unauthenticated call creates the owner.
    // The header is parsed manually (not via AuthUser) because the extractor
    // would 401 before the zero-user check could ever run.
    let needs_setup = state.store.user_count().await.map_err(store_err)? == 0;
    if !needs_setup {
        let user = auth_from_headers(&state, &headers).await?;
        require_users(&user)?;
    }
    let role = parse_role(&req.role)?;
    let role = if needs_setup {
        task_rs::Role::Owner
    } else {
        role
    };
    let row = state
        .store
        .create_user(&task_rs::NewUser {
            username: &req.username,
            password: &req.password,
            role,
        })
        .await
        .map_err(|e| match e {
            task_rs::StoreError::UsernameTaken | task_rs::StoreError::PasswordTooShort => {
                ApiError::bad_request(e.to_string())
            }
            other => ApiError::internal(other.to_string()),
        })?;
    record_activity(
        &state.store,
        "user",
        format!("created user {}", row.username),
    )
    .await;
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
    Extension(manager): Extension<Arc<manager_rs::manager::Manager>>,
    State(use_case): State<Arc<T>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatReply>, ApiError> {
    let tok = TokKind::parse(req.tokenizer.as_deref());
    let board_token = headers
        .get(http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix(BEARER_PREFIX))
        .map(str::to_string);
    note_chat_agent(manager.clone(), req.agent.clone(), &req.message).await;
    let outcome = use_case
        .execute(ChatCmd {
            message: req.message,
            mode: SearchMode::parse(req.search.as_deref()),
            max_tokens: req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            tokenizer: tok,
            think: req.think.unwrap_or(false),
            board_token,
            agent: req.agent.clone(),
            image: None,
            thread_id: req.thread_id.map(|id| id.to_string()),
            card_id: req.card_id,
        })
        .await
        .map_err(ApiError::internal)?;
    note_chat_reply(manager, req.agent, &outcome.text).await;
    let tools: Vec<ToolUseDto> = outcome.tools.iter().map(ToolUseDto::from_use).collect();
    Ok(Json(ChatReply {
        model: outcome.model,
        reply: outcome.text,
        searched: outcome.searched,
        tools,
        memories: outcome.memories,
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
    let authorize_url = auth.start().await.map_err(ApiError::bad_request)?;
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
        codex_cli::list_models(&crate::infra::codex::auth::codex_home())
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
        tools: Vec::new(),
        memories: Vec::new(),
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
    if req.say_hi_time.is_some() || req.say_hi_interval_mins.is_some() || req.timezone.is_some() {
        state
            .set_zai_schedule(
                req.say_hi_time.flatten(),
                req.say_hi_interval_mins.flatten(),
                req.timezone.flatten(),
            )
            .map_err(ApiError::bad_request)?;
    }
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
        say_hi_time: zai.say_hi_time.clone(),
        say_hi_interval_mins: zai.say_hi_interval_mins,
        timezone: zai.timezone.clone(),
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

#[derive(Serialize, utoipa::ToSchema)]
struct ZaiQuotaReply {
    tokens_used_pct: f32,
    time_limit_reset_ms: Option<i64>,
    time_limit_pct: Option<f32>,
}

#[utoipa::path(
    get,
    path = "/api/settings/zai/quota",
    responses((status = 200, body = ZaiQuotaReply), (status = 500, body = str))
)]
async fn zai_quota(
    State(state): State<Arc<SettingsState>>,
) -> Result<Json<ZaiQuotaReply>, ApiError> {
    let token = state.zai_token().map_err(ApiError::bad_request)?;
    let quota = tokio::task::spawn_blocking(move || zai_api::quota::fetch_quota(&token))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(ApiError::bad_request)?;
    Ok(Json(quota_reply(&quota)))
}

fn quota_reply(quota: &zai_api::quota::Quota) -> ZaiQuotaReply {
    let now = now_ms();
    let tokens = quota
        .limits
        .iter()
        .find(|l| l.limit_type == zai_api::quota::TOKENS_LIMIT);
    let time = quota
        .limits
        .iter()
        .filter(|l| l.limit_type == zai_api::quota::TIME_LIMIT)
        .filter(|l| l.next_reset_time.is_some_and(|t| t > now))
        .min_by_key(|l| l.next_reset_time.unwrap_or(i64::MAX));
    ZaiQuotaReply {
        tokens_used_pct: tokens.map(|l| l.percentage).unwrap_or(0.0),
        time_limit_reset_ms: time.and_then(|l| l.next_reset_time),
        time_limit_pct: time.map(|l| l.percentage),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[utoipa::path(
    get,
    path = "/api/quota",
    responses((status = 200, body = QuotaBoard))
)]
async fn quota_board(
    State((settings, usage)): State<(Arc<SettingsState>, Arc<codex_usage_rs::Store>)>,
) -> Json<QuotaBoard> {
    let latest = usage.latest().await.ok().flatten();
    let board =
        tokio::task::spawn_blocking(move || crate::infra::provider_quota::board(&settings, latest))
            .await
            .unwrap_or(QuotaBoard {
                platforms: Vec::new(),
            });
    Json(board)
}

const CODEX_USAGE_HISTORY_DEFAULT_LIMIT: i64 = 50;
const CODEX_USAGE_HISTORY_MAX_LIMIT: i64 = 500;

#[derive(Deserialize)]
struct UsageHistoryQuery {
    limit: Option<i64>,
}

async fn codex_usage_latest(
    State(store): State<Arc<codex_usage_rs::Store>>,
) -> Json<Option<codex_usage_rs::Row>> {
    Json(store.latest().await.ok().flatten())
}

async fn codex_usage_history(
    State(store): State<Arc<codex_usage_rs::Store>>,
    Query(query): Query<UsageHistoryQuery>,
) -> Json<Vec<codex_usage_rs::Row>> {
    let limit = query
        .limit
        .unwrap_or(CODEX_USAGE_HISTORY_DEFAULT_LIMIT)
        .clamp(1, CODEX_USAGE_HISTORY_MAX_LIMIT);
    Json(store.history(limit).await.unwrap_or_default())
}

#[derive(Serialize, utoipa::ToSchema)]
struct ZaiHiReply {
    ok: bool,
    reply: String,
}

#[utoipa::path(
    post,
    path = "/api/settings/zai/hi",
    responses((status = 200, body = ZaiHiReply), (status = 500, body = str))
)]
async fn zai_hi(State(state): State<Arc<SettingsState>>) -> Result<Json<ZaiHiReply>, ApiError> {
    let token = state.zai_token().map_err(ApiError::bad_request)?;
    let reply = tokio::task::spawn_blocking(move || zai_api::quota::say_hi(&token))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(ApiError::bad_request)?;
    Ok(Json(ZaiHiReply { ok: true, reply }))
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

#[derive(Serialize, utoipa::ToSchema)]
struct LocalEndpointReply {
    endpoint: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct LocalEndpointRequest {
    endpoint: String,
}

struct LocalSettingsState {
    settings: Arc<SettingsState>,
    model: Arc<dyn ModelEndpoint>,
}

#[utoipa::path(
    get,
    path = "/api/settings/local",
    responses((status = 200, body = LocalEndpointReply))
)]
async fn get_local_endpoint(
    State(state): State<Arc<LocalSettingsState>>,
) -> Json<LocalEndpointReply> {
    Json(LocalEndpointReply {
        endpoint: state.model.base_url(),
    })
}

#[utoipa::path(
    put,
    path = "/api/settings/local",
    request_body = LocalEndpointRequest,
    responses((status = 200, body = LocalEndpointReply), (status = 500, body = str))
)]
async fn set_local_endpoint(
    State(state): State<Arc<LocalSettingsState>>,
    Json(req): Json<LocalEndpointRequest>,
) -> Result<Json<LocalEndpointReply>, ApiError> {
    state
        .settings
        .set_local_endpoint(req.endpoint.trim())
        .map_err(ApiError::internal)?;
    state.model.set_base_url(req.endpoint.trim());
    Ok(Json(LocalEndpointReply {
        endpoint: state.model.base_url(),
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
    let mut env = crate::infra::client::env::ClientEnv {
        user_agent: req.user_agent,
        platform: req.platform,
        language: req.language,
        timezone: req.timezone,
        screen: req.screen,
        workspace_path: req.workspace_path,
        ..crate::infra::client::env::ClientEnv::default()
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

fn stamp_host(env: &mut crate::infra::client::env::ClientEnv) {
    let (hostname, os, arch) = crate::infra::client::env::host_fingerprint();
    env.hostname = hostname;
    env.os = os;
    env.arch = arch;
}

#[derive(serde::Serialize, utoipa::ToSchema)]
struct AlertSettingsReply {
    webhook_set: bool,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
struct AlertSettingsRequest {
    webhook_url: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/settings/alerts",
    responses((status = 200, body = AlertSettingsReply))
)]
async fn get_alert_settings(State(state): State<Arc<SettingsState>>) -> Json<AlertSettingsReply> {
    Json(AlertSettingsReply {
        webhook_set: state.alert_webhook().is_some(),
    })
}

#[utoipa::path(
    post,
    path = "/api/settings/alerts",
    request_body = AlertSettingsRequest,
    responses((status = 200, body = AlertSettingsReply), (status = 500, body = str))
)]
async fn set_alert_settings(
    State(state): State<Arc<SettingsState>>,
    Json(req): Json<AlertSettingsRequest>,
) -> Result<Json<AlertSettingsReply>, ApiError> {
    state
        .set_alert_webhook(req.webhook_url.as_deref())
        .map_err(ApiError::internal)?;
    Ok(Json(AlertSettingsReply {
        webhook_set: state.alert_webhook().is_some(),
    }))
}

#[derive(serde::Serialize, utoipa::ToSchema)]
struct GitRepoReply {
    project_id: i64,
    url: String,
    secret_set: bool,
}

fn git_repo_reply(repo: &crate::infra::settings::git::GitRepo) -> GitRepoReply {
    GitRepoReply {
        project_id: repo.project_id,
        url: crate::infra::settings::git::redact_url(&repo.url),
        secret_set: repo.secret_set(),
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
struct GitRepoRequest {
    url: String,
    /// Omit to keep the stored secret; empty string clears it.
    secret: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
struct GitReposReply {
    repos: Vec<GitRepoReply>,
}

#[utoipa::path(
    get,
    path = "/api/settings/git/repos",
    responses((status = 200, body = GitReposReply))
)]
async fn list_git_repos(State(state): State<Arc<SettingsState>>) -> Json<GitReposReply> {
    Json(GitReposReply {
        repos: state.git_repos().iter().map(git_repo_reply).collect(),
    })
}

#[utoipa::path(
    put,
    path = "/api/settings/git/repos/{project_id}",
    request_body = GitRepoRequest,
    responses((status = 200, body = GitRepoReply), (status = 400, body = str), (status = 500, body = str))
)]
async fn set_git_repo(
    State(state): State<Arc<SettingsState>>,
    Path(project_id): Path<i64>,
    Json(req): Json<GitRepoRequest>,
) -> Result<Json<GitRepoReply>, ApiError> {
    state
        .set_git_repo(project_id, &req.url, req.secret.as_deref())
        .map_err(ApiError::bad_request)?;
    let repo = state
        .git_repo(project_id)
        .ok_or_else(|| ApiError::internal("repo not found after save"))?;
    Ok(Json(git_repo_reply(&repo)))
}

#[utoipa::path(
    delete,
    path = "/api/settings/git/repos/{project_id}",
    responses((status = 200, body = GitReposReply), (status = 500, body = str))
)]
async fn remove_git_repo(
    State(state): State<Arc<SettingsState>>,
    Path(project_id): Path<i64>,
) -> Result<Json<GitReposReply>, ApiError> {
    state
        .remove_git_repo(project_id)
        .map_err(ApiError::internal)?;
    Ok(Json(GitReposReply {
        repos: state.git_repos().iter().map(git_repo_reply).collect(),
    }))
}

/// Registers a chat agent with the manager (spawn on demand) and records the
/// user message in its transcript, so the agent shows in machines/inspect.
fn note_chat_agent(
    manager: Arc<manager_rs::manager::Manager>,
    agent: Option<String>,
    message: &str,
) -> impl std::future::Future<Output = ()> + Send {
    let message = message.to_string();
    async move {
        tokio::task::spawn_blocking(move || {
            let Some(agent) = agent.as_deref().map(str::trim).filter(|a| !a.is_empty()) else {
                return;
            };
            if let Err(e) = manager.spawn(agent) {
                tracing::warn!("chat agent {agent} spawn: {e}");
                return;
            }
            if let Err(e) = manager.push_context(agent, &message) {
                tracing::warn!("chat agent {agent} context: {e}");
            }
        })
        .await
        .ok();
    }
}

/// Appends the assistant reply to the chat agent's transcript.
fn note_chat_reply(
    manager: Arc<manager_rs::manager::Manager>,
    agent: Option<String>,
    reply: &str,
) -> impl std::future::Future<Output = ()> + Send {
    let reply = format!("assistant: {reply}");
    async move {
        tokio::task::spawn_blocking(move || {
            let Some(agent) = agent.as_deref().map(str::trim).filter(|a| !a.is_empty()) else {
                return;
            };
            if let Err(e) = manager.push_context(agent, &reply) {
                tracing::warn!("chat agent {agent} context: {e}");
            }
        })
        .await
        .ok();
    }
}

const IMAGE_DENIED_MSG: &str = "this agent does not accept images";

/// Enforce the per-agent `receive_images` flag: images are rejected with 403
/// for agents that must not receive them, otherwise passed to the model as a
/// multimodal content part.
async fn gate_image(
    agents: Arc<dyn AgentConfigRepo>,
    agent: Option<&str>,
    image: Option<String>,
) -> Result<Option<String>, ApiError> {
    let Some(image) = image else {
        return Ok(None);
    };
    let Some(name) = agent else {
        return Err(ApiError::bad_request("image requires an agent"));
    };
    let cfg = agents
        .by_name(name)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("no such agent"))?;
    if !cfg.receive_images {
        return Err(ApiError::forbidden(IMAGE_DENIED_MSG));
    }
    Ok(Some(image))
}

#[utoipa::path(
    post,
    path = "/api/chat/zai",
    request_body = ZaiChatRequest,
    responses((status = 200, body = ChatReply), (status = 500, body = str))
)]
async fn chat_zai(
    Extension(manager): Extension<Arc<manager_rs::manager::Manager>>,
    Extension(deps): Extension<Arc<ZaiChatDeps>>,
    State(_state): State<Arc<SettingsState>>,
    headers: http::HeaderMap,
    Json(req): Json<ZaiChatRequest>,
) -> Result<Json<ChatReply>, ApiError> {
    const TOKENIZER: &str = "zai";
    let board_token = headers
        .get(http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix(BEARER_PREFIX))
        .map(str::to_string);
    note_chat_agent(manager.clone(), req.agent.clone(), &req.message).await;
    let engine = ZaiEngine::new(deps.settings.clone(), req.model.clone());
    let zai_deps_ = deps.clone();
    let zai = ChatUseCase::new(
        deps.searcher.clone(),
        deps.fetcher.clone(),
        deps.runner.clone(),
        Arc::new(engine),
        deps.models.clone(),
        deps.memory.clone(),
        deps.board.clone(),
        deps.agents.clone(),
    )
    .with_agent_git(zai_deps_.agent_git.clone())
    .with_agent_run(zai_deps_.agent_run.clone())
    .with_thread_envs(zai_deps_.thread_envs.clone())
    .with_project_git(zai_deps_.project_git.clone())
    .with_resources(deps.resources.clone());
    let message = match build_system_message(&req.system)? {
        Some(sys) => format!("{}\n\n{}", sys.content, req.message),
        None => req.message.clone(),
    };
    let gated_image =
        gate_image(deps.agents.clone(), req.agent.as_deref(), req.image.clone()).await?;
    if let Some(data_url) = gated_image.as_deref() {
        store_chat_image(data_url);
    }
    let outcome = zai
        .execute(ChatCmd {
            message,
            mode: SearchMode::Auto,
            max_tokens: DEFAULT_MAX_TOKENS,
            tokenizer: TokKind::Normal,
            think: false,
            board_token,
            agent: req.agent.clone(),
            image: gated_image,
            thread_id: req.thread_id.map(|id| id.to_string()),
            card_id: req.card_id,
        })
        .await
        .map_err(ApiError::internal)?;
    note_chat_reply(manager, req.agent, &outcome.text).await;
    if let Some(thread_id) = req.thread_id {
        let store = deps.store.clone();
        let (user_text, reply_text) = (req.message, outcome.text.clone());
        tokio::spawn(async move {
            if let Err(e) = store.add_chat_message(thread_id, "user", &user_text).await {
                tracing::warn!("chat transcript save (user) failed: {e}");
            }
            if let Err(e) = store
                .add_chat_message(thread_id, "assistant", &reply_text)
                .await
            {
                tracing::warn!("chat transcript save (assistant) failed: {e}");
            }
        });
    }
    let tools: Vec<ToolUseDto> = outcome.tools.iter().map(ToolUseDto::from_use).collect();
    const ZERO_TPS: f64 = 0.0;
    Ok(Json(ChatReply {
        model: outcome.model,
        reply: outcome.text,
        searched: outcome.searched,
        tools,
        memories: outcome.memories,
        tokenizer: TOKENIZER,
        prompt_tokens: outcome.stats.prompt_tokens,
        prompt_tps: ZERO_TPS,
        decode_tokens: outcome.stats.decode_tokens,
        decode_tps: ZERO_TPS,
    }))
}

/// Buffer of the mpsc feeding the SSE response body.
const SSE_CHANNEL_BUFFER: usize = 64;

type SseItem = Result<Event, Infallible>;
type SseBody = ReceiverStream<SseItem>;

fn stream_event(name: &str, data: &str) -> Result<Event, Infallible> {
    Ok(Event::default().event(name).data(data))
}

async fn persist_transcript(
    store: std::sync::Arc<task_rs::Store>,
    thread_id: i64,
    user_text: &str,
    reply_text: &str,
) {
    if let Err(e) = store.add_chat_message(thread_id, "user", user_text).await {
        tracing::warn!("chat transcript save (user) failed: {e}");
    }
    if let Err(e) = store
        .add_chat_message(thread_id, "assistant", reply_text)
        .await
    {
        tracing::warn!("chat transcript save (assistant) failed: {e}");
    }
}

/// Same tool loop as `chat_zai`, but generation progress is streamed to the
/// client as server-sent events: `turn` (new inference round), `delta`
/// (content chunk), then a final `done` (full reply payload, same shape as
/// the non-streamed JSON) or `error`.
async fn chat_zai_stream(
    Extension(manager): Extension<Arc<manager_rs::manager::Manager>>,
    Extension(deps): Extension<Arc<ZaiChatDeps>>,
    headers: http::HeaderMap,
    Json(req): Json<ZaiChatRequest>,
) -> Result<Sse<SseBody>, ApiError> {
    const TOKENIZER: &str = "zai";
    let board_token = headers
        .get(http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix(BEARER_PREFIX))
        .map(str::to_string);
    note_chat_agent(manager.clone(), req.agent.clone(), &req.message).await;
    let (event_tx, _) = tokio::sync::broadcast::channel::<StreamEvent>(STREAM_EVENT_CAPACITY);
    let cancel = CancelFlag::new();
    if let Some(run_id) = req.run_id.clone() {
        deps.cancels
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(run_id, cancel.clone());
    }
    let engine = ZaiEngine::new(deps.settings.clone(), req.model.clone())
        .with_events(event_tx.clone())
        .with_cancel(cancel);
    let (tool_tx, mut tool_rx) =
        tokio::sync::broadcast::channel::<ToolEvent>(STREAM_EVENT_CAPACITY);
    let zai = ChatUseCase::new(
        deps.searcher.clone(),
        deps.fetcher.clone(),
        deps.runner.clone(),
        Arc::new(engine),
        deps.models.clone(),
        deps.memory.clone(),
        deps.board.clone(),
        deps.agents.clone(),
    )
    .with_agent_git(deps.agent_git.clone())
    .with_agent_run(deps.agent_run.clone())
    .with_thread_envs(deps.thread_envs.clone())
    .with_project_git(deps.project_git.clone())
    .with_resources(deps.resources.clone())
    .with_tool_events(tool_tx);
    let message = match build_system_message(&req.system)? {
        Some(sys) => format!("{}\n\n{}", sys.content, req.message),
        None => req.message.clone(),
    };
    let gated_image =
        gate_image(deps.agents.clone(), req.agent.as_deref(), req.image.clone()).await?;
    if let Some(data_url) = gated_image.as_deref() {
        store_chat_image(data_url);
    }
    let cmd = ChatCmd {
        message,
        mode: SearchMode::Auto,
        max_tokens: DEFAULT_MAX_TOKENS,
        tokenizer: TokKind::Normal,
        think: false,
        board_token,
        agent: req.agent.clone(),
        image: gated_image,
        thread_id: req.thread_id.map(|id| id.to_string()),
        card_id: req.card_id,
    };
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<Result<ChatReply, String>>();
    let store = deps.store.clone();
    let thread_id = req.thread_id;
    let user_text = req.message.clone();
    let agent = req.agent.clone();
    let run_key = req.run_id;
    let cancels = deps.cancels.clone();
    tokio::spawn(async move {
        let reply = match zai.execute(cmd).await {
            Ok(outcome) => {
                note_chat_reply(manager, agent, &outcome.text).await;
                if let Some(thread_id) = thread_id {
                    persist_transcript(store, thread_id, &user_text, &outcome.text).await;
                }
                const ZERO_TPS: f64 = 0.0;
                let tools: Vec<ToolUseDto> =
                    outcome.tools.iter().map(ToolUseDto::from_use).collect();
                ChatReply {
                    model: outcome.model,
                    reply: outcome.text,
                    searched: outcome.searched,
                    tools,
                    memories: outcome.memories,
                    tokenizer: TOKENIZER,
                    prompt_tokens: outcome.stats.prompt_tokens,
                    prompt_tps: ZERO_TPS,
                    decode_tokens: outcome.stats.decode_tokens,
                    decode_tps: ZERO_TPS,
                }
            }
            Err(e) => {
                if let Some(run_key) = run_key {
                    cancels
                        .write()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&run_key);
                }
                let _ = done_tx.send(Err(e));
                return;
            }
        };
        if let Some(run_key) = run_key {
            cancels
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&run_key);
        }
        let _ = done_tx.send(Ok(reply));
    });
    let mut events = event_tx.subscribe();
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(SSE_CHANNEL_BUFFER);
    // tool-progress forwarder: ends when the use case (and its sender) drops
    let tool_sse_tx = tx.clone();
    tokio::spawn(async move {
        while let Ok(ev) = tool_rx.recv().await {
            let payload = serde_json::to_string(&ev).unwrap_or_default();
            if tool_sse_tx
                .send(stream_event("tool", &payload))
                .await
                .is_err()
            {
                return;
            }
        }
    });
    tokio::spawn(async move {
        // forward live deltas until the engine drops its sender, then relay
        // the final result
        while let Ok(ev) = events.recv().await {
            let sent = match ev {
                StreamEvent::Turn => tx.send(stream_event("turn", "")).await,
                StreamEvent::Delta(text) => {
                    let payload = serde_json::json!({ "text": text }).to_string();
                    tx.send(stream_event("delta", &payload)).await
                }
            };
            if sent.is_err() {
                return;
            }
        }
        match done_rx.await {
            Ok(Ok(reply)) => {
                let payload = serde_json::to_string(&reply).unwrap_or_default();
                let _ = tx.send(stream_event("done", &payload)).await;
            }
            Ok(Err(e)) => {
                let payload = serde_json::json!({ "error": e }).to_string();
                let _ = tx.send(stream_event("error", &payload)).await;
            }
            Err(_) => {
                let payload =
                    serde_json::json!({ "error": "chat task ended without a result" }).to_string();
                let _ = tx.send(stream_event("error", &payload)).await;
            }
        }
    });
    Ok(Sse::new(ReceiverStream::new(rx)))
}

#[derive(Deserialize, utoipa::ToSchema)]
struct ZaiChatCancelRequest {
    run_id: String,
}

/// Cancel an in-flight streamed chat run: sets its flag; the engine checks it
/// per delta and before each tool round and bails.
async fn chat_zai_cancel(
    Extension(deps): Extension<Arc<ZaiChatDeps>>,
    Json(req): Json<ZaiChatCancelRequest>,
) -> StatusCode {
    let cancelled = deps
        .cancels
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&req.run_id)
        .is_some_and(|flag| {
            flag.cancel();
            true
        });
    if cancelled {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn list_chat_threads(
    Extension(deps): Extension<Arc<ZaiChatDeps>>,
    Query(query): Query<ChatThreadQuery>,
) -> Result<Json<Vec<task_rs::ChatThreadRow>>, ApiError> {
    let rows = deps
        .store
        .list_chat_threads(query.project_id)
        .await
        .map_err(store_err)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct ChatThreadQuery {
    project_id: Option<i64>,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct NewChatThread {
    agent: String,
    #[serde(default)]
    title: String,
    project_id: Option<i64>,
}

async fn create_chat_thread(
    Extension(deps): Extension<Arc<ZaiChatDeps>>,
    Json(req): Json<NewChatThread>,
) -> Result<Json<task_rs::ChatThreadRow>, ApiError> {
    let row = deps
        .store
        .create_chat_thread(&req.agent, &req.title, req.project_id)
        .await
        .map_err(store_err)?;
    Ok(Json(row))
}

async fn list_chat_messages(
    Extension(deps): Extension<Arc<ZaiChatDeps>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<task_rs::ChatMessageRow>>, ApiError> {
    let rows = deps.store.list_chat_messages(id).await.map_err(store_err)?;
    Ok(Json(rows))
}

async fn delete_chat_thread(
    Extension(deps): Extension<Arc<ZaiChatDeps>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    deps.store.delete_chat_thread(id).await.map_err(store_err)?;
    Ok(StatusCode::NO_CONTENT)
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
        tools: Vec::new(),
        memories: Vec::new(),
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

/// Shared handler state: the composed task app plus the raw store for auth.
struct TaskState {
    app: TaskApp,
    store: std::sync::Arc<task_rs::Store>,
    sched: std::sync::Arc<crate::app::schedule_work::ScheduleHandle>,
    engine: Option<std::sync::Arc<dyn Inference>>,
}

type TaskStore = std::sync::Arc<TaskState>;

/// State for the manager routes: the process table + where finished task
/// output is stored.
#[derive(Clone)]
struct ManagerState {
    manager: Arc<manager_rs::manager::Manager>,
    store: std::sync::Arc<task_rs::Store>,
    settings: Arc<SettingsState>,
}

fn task_state(
    store: std::sync::Arc<task_rs::Store>,
    sched: std::sync::Arc<crate::app::schedule_work::ScheduleHandle>,
    engine: Option<std::sync::Arc<dyn Inference>>,
) -> TaskStore {
    std::sync::Arc::new(TaskState {
        app: crate::app::task::build(store.clone()),
        store,
        sched,
        engine,
    })
}

#[utoipa::path(get, path = "/api/workspaces", responses((status = 200, body = [WorkspaceDto])))]
async fn list_workspaces(
    State(state): State<TaskStore>,
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
    State(state): State<TaskStore>,
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
    record_activity(
        &state.store,
        "workspace",
        format!("created workspace {}", row.name),
    )
    .await;
    Ok(Json(WorkspaceDto::from(row)))
}

#[utoipa::path(delete, path = "/api/workspaces/{id}", responses((status = 200, body = str), (status = 404, body = str)))]
async fn delete_workspace(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state.app.workspaces.remove(id).await.map_err(store_err)?;
    Ok("ok")
}

#[utoipa::path(get, path = "/api/workspaces/{id}/projects", responses((status = 200, body = [ProjectDto])))]
async fn list_projects(
    State(state): State<TaskStore>,
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
    State(state): State<TaskStore>,
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
    record_activity(
        &state.store,
        "project",
        format!("created project {}", row.name),
    )
    .await;
    Ok(Json(ProjectDto::from(row)))
}

#[utoipa::path(delete, path = "/api/projects/{id}", responses((status = 200, body = str), (status = 404, body = str)))]
async fn delete_project(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state.app.projects.remove(id).await.map_err(store_err)?;
    Ok("ok")
}

#[utoipa::path(get, path = "/api/task/cards", responses((status = 200, body = [CardDto])))]
async fn list_cards(
    State(state): State<TaskStore>,
    axum::extract::Query(query): axum::extract::Query<ListCardsQuery>,
    _user: AuthUser,
) -> Result<Json<Vec<CardDto>>, ApiError> {
    let cards = state
        .app
        .cards
        .list(query.project_id)
        .await
        .map_err(task_err)?;
    Ok(Json(cards.into_iter().map(CardDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/task/cards",
    request_body = CreateCardRequest,
    responses((status = 200, body = CardDto), (status = 400, body = str))
)]
async fn create_card(
    State(state): State<TaskStore>,
    user: AuthUser,
    Json(req): Json<CreateCardRequest>,
) -> Result<Json<CardDto>, ApiError> {
    require_edit(&user)?;
    let priority = req
        .priority
        .unwrap_or_else(|| task_rs::PRIORITY_NORMAL.to_string());
    let row = state
        .app
        .cards
        .create(NewCard {
            project_id: req.project_id,
            column_id: req.column_id,
            title: req.title,
            description: req.description.unwrap_or_default(),
            priority,
            labels: req.labels,
            checklist: req.checklist,
            estimate: req.estimate,
        })
        .await
        .map_err(task_err)?;
    record_activity(
        &state.store,
        "card",
        format!("created task \"{}\"", row.title),
    )
    .await;
    crate::app::events::publish(crate::app::events::EventKind::Card);
    Ok(Json(CardDto::from(row)))
}

#[utoipa::path(
    post,
    path = "/api/task/cards/{id}/move",
    request_body = MoveCardRequest,
    responses((status = 200, body = str), (status = 404, body = str))
)]
async fn move_card(
    State(state): State<TaskStore>,
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
            column_id: req.column_id.clone(),
            position: req.position,
        })
        .await
        .map_err(task_err)?;
    record_activity(
        &state.store,
        "card",
        format!("moved task {id} to {}", req.column_id),
    )
    .await;
    crate::app::events::publish(crate::app::events::EventKind::Card);
    Ok("ok")
}

#[utoipa::path(delete, path = "/api/task/cards/{id}", responses((status = 200, body = str), (status = 404, body = str)))]
async fn remove_card(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state.app.cards.remove(id).await.map_err(task_err)?;
    crate::app::events::publish(crate::app::events::EventKind::Card);
    Ok("ok")
}

#[utoipa::path(get, path = "/api/task/cards/{id}/agent", responses((status = 200, body = AgentDto), (status = 404, body = str)))]
async fn get_agent(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    _user: AuthUser,
) -> Result<Json<AgentDto>, ApiError> {
    let card = state
        .app
        .cards
        .get(id)
        .await
        .map_err(task_err)?
        .ok_or(ApiError(
            ApiError::NOT_FOUND_MSG.to_string(),
            StatusCode::NOT_FOUND,
        ))?;
    let agent = state
        .app
        .cards
        .agent(id)
        .await
        .map_err(task_err)?
        .unwrap_or(task_rs::AgentState {
            name: String::new(),
            state: serde_json::Value::Null,
        });
    Ok(Json(AgentDto {
        name: agent.name,
        state: agent.state,
        runner: card.runner().as_kind().to_owned(),
        run_status: card.run_status,
        last_agent: card.last_agent,
    }))
}

#[utoipa::path(
    put,
    path = "/api/task/cards/{id}/agent",
    request_body = SetAgentRequest,
    responses((status = 200, body = str), (status = 404, body = str))
)]
async fn set_agent(
    State(state): State<TaskStore>,
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
            &task_rs::AgentState {
                name: req.name,
                state: req.state.unwrap_or(serde_json::Value::Null),
            },
        )
        .await
        .map_err(task_err)?;
    crate::app::events::publish(crate::app::events::EventKind::Card);
    Ok("ok")
}

#[utoipa::path(
    put,
    path = "/api/task/cards/{id}/image",
    request_body = SetCardImageRequest,
    responses((status = 200, body = str), (status = 404, body = str))
)]
async fn set_card_image(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
    Json(req): Json<SetCardImageRequest>,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state
        .app
        .cards
        .set_card_image(id, req.image.as_deref())
        .await
        .map_err(task_err)?;
    crate::app::events::publish(crate::app::events::EventKind::Card);
    Ok("ok")
}

#[utoipa::path(
    get,
    path = "/api/task/cards/{id}/runs",
    responses((status = 200, body = [CardRunDto]), (status = 404, body = str))
)]
async fn list_card_runs(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    _user: AuthUser,
) -> Result<Json<Vec<CardRunDto>>, ApiError> {
    let runs = state.app.cards.card_runs(id).await.map_err(task_err)?;
    Ok(Json(
        runs.iter().map(|r| CardRunDto::from(r.clone())).collect(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/task/cards/{id}/resources",
    responses((status = 200, body = [ResourceDto]), (status = 404, body = str))
)]
async fn list_card_resources(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    _user: AuthUser,
) -> Result<Json<Vec<ResourceDto>>, ApiError> {
    let rows = state.app.resources.list(id).await.map_err(task_err)?;
    Ok(Json(rows.iter().map(ResourceDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/task/cards/{id}/run",
    responses((status = 200, body = crate::app::card_run::RunRecord), (status = 404, body = str), (status = 400, body = str))
)]
async fn run_card(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
) -> Result<Json<crate::app::card_run::RunRecord>, ApiError> {
    require_edit(&user)?;
    let record = crate::app::card_run::run_card(
        &state.app,
        state.engine.clone(),
        id,
        task_rs::TRIGGER_MANUAL,
    )
    .await
    .map_err(task_err)?;
    record_activity(
        &state.store,
        "run",
        format!("started agent run for task {id}"),
    )
    .await;
    crate::app::events::publish(crate::app::events::EventKind::Card);
    Ok(Json(record))
}

#[utoipa::path(
    put,
    path = "/api/task/cards/{id}/schedule",
    request_body = SetScheduleRequest,
    responses((status = 200, body = str), (status = 404, body = str), (status = 400, body = str))
)]
async fn set_card_schedule(
    State(state): State<TaskStore>,
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
        .map_err(task_err)?;
    crate::app::events::publish(crate::app::events::EventKind::Cron);
    Ok("ok")
}

#[derive(Serialize, utoipa::ToSchema)]
struct CronJobDto {
    card_id: i64,
    title: String,
    cron: String,
    agent_name: Option<String>,
    next_run: u64,
}

#[utoipa::path(
    get,
    path = "/api/cronjobs",
    responses((status = 200, body = [CronJobDto]))
)]
async fn list_cronjobs(
    State(state): State<TaskStore>,
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
        .cards
        .list(None)
        .await
        .map_err(task_err)?
        .into_iter()
        .filter_map(|card| {
            let cron = card.cron.clone()?;
            Some(CronJobDto {
                card_id: card.id,
                title: card.title,
                cron,
                agent_name: card.agent_name,
                next_run: next.get(&card.id).copied().unwrap_or(0),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(jobs))
}

#[utoipa::path(
    put,
    path = "/api/task/cards/{id}",
    request_body = UpdateCardRequest,
    responses((status = 200, body = str), (status = 404, body = str))
)]
async fn update_card(
    State(state): State<TaskStore>,
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
            deadline: req.deadline,
            priority: req.priority,
            labels: req.labels,
            checklist: req.checklist,
            estimate: req.estimate,
        })
        .await
        .map_err(task_err)?;
    crate::app::events::publish(crate::app::events::EventKind::Card);
    let card = state
        .app
        .cards
        .get(id)
        .await
        .map_err(task_err)?
        .ok_or(ApiError(
            ApiError::NOT_FOUND_MSG.to_string(),
            StatusCode::NOT_FOUND,
        ))?;
    Ok(Json(CardDto::from(card)))
}

#[utoipa::path(
    get,
    path = "/api/task/cards/{id}/comments",
    responses((status = 200, body = [CommentDto]), (status = 404, body = str))
)]
async fn list_comments(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    _user: AuthUser,
) -> Result<Json<Vec<CommentDto>>, ApiError> {
    let rows = state.app.comments.list(id).await.map_err(task_err)?;
    Ok(Json(rows.into_iter().map(CommentDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/task/cards/{id}/comments",
    request_body = AddCommentRequest,
    responses((status = 200, body = CommentDto), (status = 404, body = str))
)]
async fn add_comment(
    State(state): State<TaskStore>,
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
        .map_err(task_err)?;
    Ok(Json(CommentDto::from(row)))
}

#[utoipa::path(get, path = "/api/agents", responses((status = 200, body = [AgentConfigDto])))]
async fn list_agents(
    State(state): State<TaskStore>,
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
    State(state): State<TaskStore>,
    user: AuthUser,
    Json(req): Json<AgentConfigRequest>,
) -> Result<Json<AgentConfigDto>, ApiError> {
    require_edit(&user)?;
    let allowed_tools = valid_tools(&req.allowed_tools)?;
    let thinking = valid_thinking(req.thinking.as_deref())?;
    let row = state
        .app
        .agents
        .create(AgentConfigDraft {
            name: req.name.trim().to_string(),
            model: req.model.unwrap_or_default(),
            persona: req.persona.unwrap_or_default(),
            prompt: req.prompt.unwrap_or_default(),
            output: req.output.unwrap_or_default(),
            allowed_tools,
            receive_images: req.receive_images,
            thinking,
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
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
    Json(req): Json<AgentConfigRequest>,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    let allowed_tools = valid_tools(&req.allowed_tools)?;
    let thinking = valid_thinking(req.thinking.as_deref())?;
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
                allowed_tools,
                receive_images: req.receive_images,
                thinking,
            },
        )
        .await
        .map_err(cfg_err)?;
    Ok("ok")
}

#[utoipa::path(delete, path = "/api/agents/{id}", responses((status = 200, body = str), (status = 404, body = str)))]
async fn remove_agent_cfg(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state.app.agents.remove(id).await.map_err(cfg_err)?;
    Ok("ok")
}

#[utoipa::path(get, path = "/api/skills", responses((status = 200, body = [SkillDto])))]
async fn list_skills(
    State(state): State<TaskStore>,
    _user: AuthUser,
) -> Result<Json<Vec<SkillDto>>, ApiError> {
    let rows = state.app.skills.list().await.map_err(cfg_err)?;
    Ok(Json(rows.into_iter().map(SkillDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/skills",
    request_body = SkillRequest,
    responses((status = 200, body = SkillDto), (status = 400, body = str))
)]
async fn create_skill(
    State(state): State<TaskStore>,
    user: AuthUser,
    Json(req): Json<SkillRequest>,
) -> Result<Json<SkillDto>, ApiError> {
    require_edit(&user)?;
    let row = state
        .app
        .skills
        .create(req.name.trim(), req.body.trim())
        .await
        .map_err(cfg_err)?;
    Ok(Json(SkillDto::from(row)))
}

#[utoipa::path(
    put,
    path = "/api/skills/{id}",
    request_body = SkillRequest,
    responses((status = 200, body = str), (status = 404, body = str))
)]
async fn update_skill(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
    Json(req): Json<SkillRequest>,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state
        .app
        .skills
        .update(id, req.body.trim())
        .await
        .map_err(cfg_err)?;
    Ok("ok")
}

#[utoipa::path(delete, path = "/api/skills/{id}", responses((status = 200, body = str), (status = 404, body = str)))]
async fn remove_skill(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state.app.skills.remove(id).await.map_err(cfg_err)?;
    Ok("ok")
}

#[utoipa::path(
    get,
    path = "/api/agents/{id}/skills",
    responses((status = 200, body = [SkillDto]), (status = 404, body = str))
)]
async fn list_agent_skills(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    _user: AuthUser,
) -> Result<Json<Vec<SkillDto>>, ApiError> {
    let rows = state.app.skills.list_for_agent(id).await.map_err(cfg_err)?;
    Ok(Json(rows.into_iter().map(SkillDto::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/agents/{id}/skills",
    request_body = AgentSkillRequest,
    responses((status = 200, body = str), (status = 404, body = str))
)]
async fn attach_agent_skill(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
    Json(req): Json<AgentSkillRequest>,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state
        .app
        .skills
        .attach(id, req.skill_id)
        .await
        .map_err(cfg_err)?;
    Ok("ok")
}

#[utoipa::path(
    delete,
    path = "/api/agents/{id}/skills/{skill_id}",
    responses((status = 200, body = str), (status = 404, body = str))
)]
async fn detach_agent_skill(
    State(state): State<TaskStore>,
    axum::extract::Path(ids): axum::extract::Path<(i64, i64)>,
    user: AuthUser,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    state
        .app
        .skills
        .detach(ids.0, ids.1)
        .await
        .map_err(cfg_err)?;
    Ok("ok")
}

#[utoipa::path(get, path = "/api/activity", responses((status = 200, body = [ActivityDto])))]
async fn list_activity(
    State(state): State<TaskStore>,
    _user: AuthUser,
) -> Result<Json<Vec<ActivityDto>>, ApiError> {
    let rows = state
        .store
        .list_activity(task_rs::ACTIVITY_LIST_DEFAULT)
        .await
        .map_err(store_err)?;
    Ok(Json(rows.into_iter().map(ActivityDto::from).collect()))
}

const OUTPUT_LIST_DEFAULT: i64 = 50;

#[derive(Serialize, utoipa::ToSchema)]
struct AgentOutputDto {
    id: i64,
    agent: String,
    result: Option<String>,
    patch: String,
    commit_oid: Option<String>,
    transcript: String,
    status: String,
    created_at: String,
}

impl From<task_rs::AgentOutputRow> for AgentOutputDto {
    fn from(r: task_rs::AgentOutputRow) -> Self {
        Self {
            id: r.id,
            agent: r.agent,
            result: r.result,
            patch: r.patch,
            commit_oid: r.commit_oid,
            transcript: r.transcript,
            status: r.status,
            created_at: r.created_at.to_rfc3339(),
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
struct AgentOutputStatusRequest {
    status: String,
}

#[utoipa::path(
    get,
    path = "/api/agent-outputs",
    responses((status = 200, body = [AgentOutputDto]), (status = 401, body = str))
)]
async fn list_agent_outputs_handler(
    State(state): State<TaskStore>,
    axum::extract::Query(q): axum::extract::Query<AgentOutputStatusQuery>,
    _user: AuthUser,
) -> Result<Json<Vec<AgentOutputDto>>, ApiError> {
    let status = q.status.as_deref().filter(|s| !s.is_empty());
    let rows = state
        .store
        .list_agent_outputs(status, OUTPUT_LIST_DEFAULT)
        .await
        .map_err(store_err)?;
    Ok(Json(rows.into_iter().map(AgentOutputDto::from).collect()))
}

#[derive(Deserialize, utoipa::ToSchema)]
struct AgentOutputStatusQuery {
    status: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/agent-outputs/{id}",
    responses((status = 200, body = AgentOutputDto), (status = 404, body = str))
)]
async fn get_agent_output_handler(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    _user: AuthUser,
) -> Result<Json<AgentOutputDto>, ApiError> {
    let row = state
        .store
        .get_agent_output(id)
        .await
        .map_err(store_err)?
        .ok_or_else(|| ApiError(ApiError::NOT_FOUND_MSG.to_string(), StatusCode::NOT_FOUND))?;
    Ok(Json(AgentOutputDto::from(row)))
}

#[utoipa::path(
    post,
    path = "/api/agent-outputs/{id}/status",
    request_body = AgentOutputStatusRequest,
    responses((status = 200, body = str), (status = 400, body = str), (status = 404, body = str))
)]
async fn set_agent_output_status_handler(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    user: AuthUser,
    Json(req): Json<AgentOutputStatusRequest>,
) -> Result<&'static str, ApiError> {
    require_edit(&user)?;
    let status = match req.status.as_str() {
        task_rs::OUTPUT_STATUS_APPROVED => task_rs::OUTPUT_STATUS_APPROVED,
        task_rs::OUTPUT_STATUS_REJECTED => task_rs::OUTPUT_STATUS_REJECTED,
        task_rs::OUTPUT_STATUS_PENDING => task_rs::OUTPUT_STATUS_PENDING,
        _ => {
            return Err(ApiError::bad_request(
                "status must be approved | rejected | pending",
            ));
        }
    };
    state
        .store
        .set_agent_output_status(id, status)
        .await
        .map_err(task_err)?;
    record_activity(
        &state.store,
        "agent-output",
        format!("output #{id} marked {status} by {}", user.0.username),
    )
    .await;
    Ok("ok")
}

#[derive(Deserialize)]
struct EventTokenQuery {
    token: Option<String>,
}

async fn events_ws(
    State(state): State<TaskStore>,
    axum::extract::Query(q): axum::extract::Query<EventTokenQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let token = q.token.unwrap_or_default();
    let authorized = state
        .store
        .auth(&token)
        .await
        .map(|u| u.is_some())
        .unwrap_or(false);
    if !authorized {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut rx = crate::app::events::subscribe();
    ws.on_upgrade(move |mut socket| async move {
        loop {
            tokio::select! {
                ev = rx.recv() => {
                    if let Ok(ev) = ev {
                        let Ok(text) = serde_json::to_string(&ev) else { continue };
                        if socket.send(Message::text(text)).await.is_err() {
                            break;
                        }
                    }
                }
                msg = socket.recv() => {
                    if matches!(
                        msg,
                        None | Some(Err(_)) | Some(Ok(Message::Close(_)))
                    ) {
                        break;
                    }
                }
            }
        }
    })
}

async fn record_activity(store: &task_rs::Store, kind: &str, message: impl std::fmt::Display) {
    if let Err(e) = store.record_activity(kind, &message.to_string()).await {
        tracing::warn!(kind, error = %e, "activity log write failed");
    }
}

fn store_err(e: task_rs::StoreError) -> ApiError {
    ApiError::internal(e.to_string())
}

fn workspace_err(e: task_rs::StoreError) -> ApiError {
    match e {
        task_rs::StoreError::WorkspaceTaken => {
            ApiError::bad_request("workspace name already taken")
        }
        task_rs::StoreError::ProjectTaken => ApiError::bad_request("project name already taken"),
        other => ApiError::internal(other.to_string()),
    }
}

fn cfg_err(e: task_rs::StoreError) -> ApiError {
    match e {
        task_rs::StoreError::NoSuchCard
        | task_rs::StoreError::NoSuchAgent
        | task_rs::StoreError::NoSuchSkill => {
            ApiError(ApiError::NOT_FOUND_MSG.to_string(), StatusCode::NOT_FOUND)
        }
        task_rs::StoreError::AgentTaken => ApiError::bad_request("agent name already taken"),
        task_rs::StoreError::SkillTaken => ApiError::bad_request("skill name already taken"),
        task_rs::StoreError::BadSpec(msg) => ApiError::bad_request(msg),
        other => ApiError::internal(other.to_string()),
    }
}

fn task_err(e: task_rs::StoreError) -> ApiError {
    match e {
        task_rs::StoreError::NoSuchCard | task_rs::StoreError::NoSuchColumn => {
            ApiError(ApiError::NOT_FOUND_MSG.to_string(), StatusCode::NOT_FOUND)
        }
        task_rs::StoreError::BadSpec(msg) => ApiError::bad_request(msg),
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
    get,
    path = "/api/host",
    responses((status = 200, body = HostSpec))
)]
async fn host_spec_handler() -> Json<HostSpec> {
    Json(host_spec::host_spec())
}

/// Default model-server URL when no endpoint is saved in local settings.
const MODEL_SERVER_DEFAULT: &str = "http://127.0.0.1:8992";
/// Hub probe budget: machines panel must stay responsive when the hub is down.
const HUB_TIMEOUT_SECS: u64 = 3;

#[derive(Serialize, utoipa::ToSchema)]
struct MachineView {
    hostname: String,
    os: String,
    arch: String,
    /// "backend" for this machine; hub clients report model|worker.
    role: String,
    /// Installed RAM in GiB (probe value for hub clients).
    ram_gib: u64,
    /// Reachability: true for this machine; a hub client is ok when its
    /// round trips through the hub succeeded.
    ok: bool,
    local: bool,
    /// Hub client id when the machine is a registered remote worker.
    client_id: Option<u64>,
    /// Agents running on this machine: name/runs/last-command.
    agents: Vec<AgentBriefDto>,
    sandboxes: Vec<SandboxDirInfo>,
}

/// OpenAPI mirror of `proto_rs::AgentBrief` (proto-rs has no utoipa dep).
#[derive(Serialize, utoipa::ToSchema)]
struct AgentBriefDto {
    name: String,
    runs: u64,
    last_cmd: Option<String>,
}

impl AgentBriefDto {
    fn of(a: proto_rs::AgentBrief) -> Self {
        Self {
            name: a.name,
            runs: a.runs,
            last_cmd: a.last_cmd,
        }
    }
}

fn model_server_url() -> String {
    crate::infra::settings::local::read_saved()
        .map(|s| s.endpoint)
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| MODEL_SERVER_DEFAULT.to_string())
}

fn sandbox_info(d: &core_agent::podman::SandboxDir) -> SandboxDirInfo {
    SandboxDirInfo {
        pid: d.pid,
        alive: d.alive,
        path: d.path.display().to_string(),
    }
}

/// Remote hub clients; each reachable client's sandbox root is resolved with
/// a `pwd` dispatch through the hub command channel.
fn remote_machines(local_hostname: &str) -> Vec<MachineView> {
    let url = model_server_url();
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(HUB_TIMEOUT_SECS))
        .build();
    let Ok(resp) = agent.get(&format!("{url}/api/clients")).call() else {
        return Vec::new();
    };
    let Ok(val) = resp.into_json::<serde_json::Value>() else {
        return Vec::new();
    };
    let Some(rows) = val.as_array() else {
        return Vec::new();
    };
    rows.iter()
        .map(|c| {
            let id = c["id"].as_u64();
            let agents = hub_client_agents(&agent, &url, id);
            let sandboxes = hub_client_sandbox(&agent, &url, id);
            let ok = agents.is_some() || sandboxes.is_some();
            MachineView {
                hostname: c["hostname"].as_str().unwrap_or("client").to_string(),
                os: c["os"].as_str().unwrap_or_default().to_string(),
                arch: c["arch"].as_str().unwrap_or_default().to_string(),
                role: c["role"].as_str().unwrap_or_default().to_string(),
                ram_gib: c["ram_gib"].as_u64().unwrap_or(0),
                ok,
                local: false,
                client_id: id,
                agents: agents
                    .unwrap_or_default()
                    .into_iter()
                    .map(AgentBriefDto::of)
                    .collect(),
                sandboxes: sandboxes.unwrap_or_default(),
            }
        })
        .filter(|m| m.hostname != local_hostname)
        .collect()
}

/// Agent names a remote client machine currently holds; `None` when the
/// round trip through the hub failed (machine unreachable).
fn hub_client_agents(agent: &ureq::Agent, url: &str, id: Option<u64>) -> Option<Vec<AgentBrief>> {
    let id = id?;
    let resp = agent
        .get(&format!("{url}/api/clients/{id}/agents"))
        .call()
        .ok()?;
    let names = resp.into_json::<Vec<AgentBrief>>().ok()?;
    Some(names)
}

/// Sandbox roots of a remote client; `None` when the `pwd` dispatch through
/// the hub failed (machine unreachable).
fn hub_client_sandbox(
    agent: &ureq::Agent,
    url: &str,
    id: Option<u64>,
) -> Option<Vec<SandboxDirInfo>> {
    let id = id?;
    let body = serde_json::json!({ "cmd": "pwd" }).to_string();
    let resp = agent
        .post(&format!("{url}/api/clients/{id}/command"))
        .send_string(&body)
        .ok()?;
    let val = resp.into_json::<serde_json::Value>().ok()?;
    match val["output"].as_str() {
        Some(path) if !path.trim().is_empty() => Some(vec![SandboxDirInfo {
            pid: 0,
            alive: true,
            path: path.trim().to_string(),
        }]),
        // reachable but no sandbox yet
        _ => Some(Vec::new()),
    }
}

fn machine_views(manager: &manager_rs::manager::Manager) -> Vec<MachineView> {
    const GIB: u64 = 1024 * 1024 * 1024;
    let host = host_spec::host_spec();
    let mut views = vec![MachineView {
        hostname: host.hostname.clone(),
        os: host.os,
        arch: host.arch,
        role: "backend".to_string(),
        ram_gib: host.memory_bytes / GIB,
        ok: true,
        local: true,
        client_id: None,
        agents: manager
            .snapshot()
            .into_iter()
            .map(|a| AgentBriefDto {
                name: a.agent,
                runs: a.runs,
                last_cmd: a.last_cmd,
            })
            .collect(),
        sandboxes: AgentSandbox::dirs().iter().map(sandbox_info).collect(),
    }];
    views.extend(remote_machines(&host.hostname));
    views
}

/// State for the machines routes: agent inventory of this backend.
#[derive(Clone)]
struct MachinesState {
    manager: std::sync::Arc<manager_rs::manager::Manager>,
}

#[utoipa::path(
    get,
    path = "/api/machines",
    responses((status = 200, body = [MachineView]))
)]
async fn machines(State(state): State<MachinesState>) -> Json<Vec<MachineView>> {
    let manager = state.manager;
    Json(
        tokio::task::spawn_blocking(move || machine_views(&manager))
            .await
            .unwrap_or_default(),
    )
}

#[derive(Serialize, utoipa::ToSchema)]
struct MachineKickReply {
    hostname: String,
    kicked: bool,
}

/// Forcibly disconnect a registered client machine from the hub. The client
/// node reconnects every 3 s while alive, so this drops dead/stale
/// registrations; stopping the client process is the real uninstall.
async fn kick_machine(Path(hostname): Path<String>) -> Result<Json<MachineKickReply>, ApiError> {
    let lookup = hostname.clone();
    let kicked = tokio::task::spawn_blocking(move || -> Result<bool, String> {
        let url = model_server_url();
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(HUB_TIMEOUT_SECS))
            .build();
        let rows = agent
            .get(&format!("{url}/api/clients"))
            .call()
            .map_err(|e| format!("hub unreachable: {e}"))?
            .into_json::<serde_json::Value>()
            .map_err(|e| format!("hub reply: {e}"))?;
        let id = rows
            .as_array()
            .and_then(|rows| {
                rows.iter()
                    .find(|c| c["hostname"].as_str() == Some(lookup.as_str()))
                    .and_then(|c| c["id"].as_u64())
            })
            .ok_or_else(|| format!("machine {lookup} is not registered"))?;
        let reply = agent
            .post(&format!("{url}/api/clients/{id}/kick"))
            .call()
            .map_err(|e| format!("kick failed: {e}"))?
            .into_json::<serde_json::Value>()
            .map_err(|e| format!("kick reply: {e}"))?;
        Ok(reply["kicked"].as_bool().unwrap_or(false))
    })
    .await
    .map_err(|e| ApiError::bad_request(e.to_string()))?
    .map_err(ApiError::bad_request)?;
    Ok(Json(MachineKickReply { hostname, kicked }))
}

#[derive(Serialize, utoipa::ToSchema)]
struct SandboxLogEntry {
    role: String,
    content: String,
}

/// Transcript of the workspace sandbox at `path`, read from its persisted
/// state file (works even if the owning process is gone).
#[utoipa::path(
    get,
    path = "/api/sandbox/logs",
    params(("path" = String, Query)),
    responses((status = 200, body = [SandboxLogEntry]), (status = 400, body = str))
)]
async fn sandbox_logs(
    Query(req): Query<SandboxLogsRequest>,
) -> Result<Json<Vec<SandboxLogEntry>>, ApiError> {
    let path = std::path::PathBuf::from(&req.path);
    // Only ever serve persisted sandbox dirs under the temp sandbox prefix —
    // never arbitrary host paths.
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if !file_name.starts_with(core_agent::podman::SANDBOX_PREFIX) {
        return Err(ApiError::bad_request("not a sandbox directory"));
    }
    let history = tokio::task::spawn_blocking(move || core_agent::podman::load_transcript(&path))
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
        .map_err(ApiError::bad_request)?;
    Ok(Json(
        history
            .into_iter()
            .map(|h| SandboxLogEntry {
                role: format!("{:?}", h.role).to_lowercase(),
                content: h.content,
            })
            .collect(),
    ))
}

#[derive(Deserialize, utoipa::ToSchema)]
struct SandboxLogsRequest {
    path: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct MachineAgentRunRequest {
    cmd: String,
}

#[derive(Serialize, utoipa::ToSchema)]
struct MachineAgentReply {
    output: String,
}

/// Dispatch a per-agent command to the shared machine `hostname` (a hub
/// client). The client node spawns the agent's own work tree on first call,
/// so this both spawns and runs in one round trip.
async fn dispatch_machine_agent(
    hostname: String,
    agent: String,
    cmd: String,
) -> Result<Json<MachineAgentReply>, ApiError> {
    if agent.is_empty() {
        return Err(ApiError::bad_request("agent name required"));
    }
    let url = model_server_url();
    let output = tokio::task::spawn_blocking(move || {
        let agent_ = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(HUB_TIMEOUT_SECS))
            .build();
        let rows = agent_
            .get(&format!("{url}/api/clients"))
            .call()
            .map_err(|e| format!("hub unreachable: {e}"))?
            .into_json::<serde_json::Value>()
            .map_err(|e| format!("hub reply: {e}"))?;
        let id = rows
            .as_array()
            .and_then(|rows| {
                rows.iter()
                    .find(|c| c["hostname"].as_str() == Some(hostname.as_str()))
                    .and_then(|c| c["id"].as_u64())
            })
            .ok_or_else(|| format!("machine {hostname} is not registered"))?;
        agent_
            .post(&format!("{url}/api/clients/{id}/command"))
            .send_string(&serde_json::json!({ "cmd": cmd, "agent": agent }).to_string())
            .map_err(|e| format!("dispatch failed: {e}"))?
            .into_json::<serde_json::Value>()
            .map_err(|e| format!("dispatch reply: {e}"))?["output"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "dispatch reply missing output".to_string())
    })
    .await
    .map_err(|e| ApiError::bad_request(e.to_string()))?
    .map_err(ApiError::bad_request)?;
    Ok(Json(MachineAgentReply { output }))
}

#[utoipa::path(
    post,
    path = "/api/machines/{hostname}/agents/{agent}/run",
    request_body = MachineAgentRunRequest,
    responses((status = 200, body = MachineAgentReply), (status = 400, body = str))
)]
async fn run_machine_agent(
    Path((hostname, agent)): Path<(String, String)>,
    Json(req): Json<MachineAgentRunRequest>,
) -> Result<Json<MachineAgentReply>, ApiError> {
    dispatch_machine_agent(hostname, agent, req.cmd).await
}

#[derive(Deserialize, utoipa::ToSchema)]
struct MachineAgentGitRequest {
    op: String,
    url: Option<String>,
    token: Option<String>,
    name: Option<String>,
    message: Option<String>,
    branch: Option<String>,
    title: Option<String>,
    head: Option<String>,
    base: Option<String>,
    project_id: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/api/machines/{hostname}/agents/{agent}/git",
    request_body = MachineAgentGitRequest,
    responses((status = 200, body = MachineAgentReply), (status = 400, body = str))
)]
async fn run_machine_git(
    Path((hostname, agent)): Path<(String, String)>,
    Json(req): Json<MachineAgentGitRequest>,
) -> Result<Json<MachineAgentReply>, ApiError> {
    let tool = AgentGitRequest {
        op: req.op,
        url: req.url,
        token: req.token,
        name: req.name,
        message: req.message,
        branch: req.branch,
        title: req.title,
        head: req.head,
        base: req.base,
        project_id: req.project_id,
    }
    .tool()?;
    if agent.is_empty() {
        return Err(ApiError::bad_request("agent name required"));
    }
    let url = model_server_url();
    let output = tokio::task::spawn_blocking(move || {
        let agent_ = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(HUB_TIMEOUT_SECS))
            .build();
        let rows = agent_
            .get(&format!("{url}/api/clients"))
            .call()
            .map_err(|e| format!("hub unreachable: {e}"))?
            .into_json::<serde_json::Value>()
            .map_err(|e| format!("hub reply: {e}"))?;
        let id = rows
            .as_array()
            .and_then(|rows| {
                rows.iter()
                    .find(|c| c["hostname"].as_str() == Some(hostname.as_str()))
                    .and_then(|c| c["id"].as_u64())
            })
            .ok_or_else(|| format!("machine {hostname} is not registered"))?;
        agent_
            .post(&format!("{url}/api/clients/{id}/command"))
            .send_string(&serde_json::json!({ "agent": agent, "git": tool }).to_string())
            .map_err(|e| format!("dispatch failed: {e}"))?
            .into_json::<serde_json::Value>()
            .map_err(|e| format!("dispatch reply: {e}"))?["output"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "dispatch reply missing output".to_string())
    })
    .await
    .map_err(|e| ApiError::bad_request(e.to_string()))?
    .map_err(ApiError::bad_request)?;
    Ok(Json(MachineAgentReply { output }))
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
    /// Seed the agent work tree from this project's configured git repo
    /// (only applies on the first spawn, while the work tree is fresh).
    project_id: Option<i64>,
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

async fn manager_list_agents(State(state): State<ManagerState>) -> Json<AgentListReply> {
    Json(AgentListReply {
        agents: state
            .manager
            .snapshot()
            .iter()
            .map(|a| serde_json::to_value(a).unwrap_or_default())
            .collect(),
    })
}

async fn spawn_agent(
    State(state): State<ManagerState>,
    Json(req): Json<AgentSpawnRequest>,
) -> Result<Json<AgentSpawnReply>, ApiError> {
    let repo = req.project_id.and_then(|id| {
        state
            .settings
            .git_repo(id)
            .map(|r| manager_rs::manager::RemoteRepo {
                url: r.url,
                token: r.secret,
            })
    });
    let work_tree = state
        .manager
        .spawn_with_repo(&req.agent, repo.as_ref())
        .map_err(ApiError::bad_request)?;
    Ok(Json(AgentSpawnReply {
        agent: req.agent,
        work_tree: work_tree.display().to_string(),
    }))
}

async fn run_agent_command(
    State(state): State<ManagerState>,
    Path(agent): Path<String>,
    Json(req): Json<AgentRunRequest>,
) -> Result<Json<AgentRunReply>, ApiError> {
    let manager = Arc::clone(&state.manager);
    let cmd_agent = agent.clone();
    let output = tokio::task::spawn_blocking(move || manager.run(&cmd_agent, &req.cmd))
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
        .map_err(ApiError::bad_request)?;
    Ok(Json(AgentRunReply { agent, output }))
}

#[derive(Deserialize, utoipa::ToSchema)]
struct AgentTokenRequest {
    project_id: Option<i64>,
    card_id: Option<i64>,
    thread_id: Option<i64>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct AgentTokenReply {
    token: String,
    agent: String,
    project_id: Option<i64>,
    card_id: Option<i64>,
    thread_id: Option<i64>,
    expires_at: String,
}

/// Issue a one-shot run ticket so an agent can post results back without a
/// human session. The plaintext token is shown once; only its SHA-256 hash is
/// stored, and it expires after `TOKEN_TTL_SECS`.
async fn issue_agent_run_token(
    State(state): State<ManagerState>,
    headers: http::HeaderMap,
    Path(agent): Path<String>,
    Json(req): Json<AgentTokenRequest>,
) -> Result<Json<AgentTokenReply>, ApiError> {
    let user = auth_user_from_store(&state.store, &headers).await?;
    require_edit(&AuthUser(user))?;
    let issued = state
        .store
        .issue_agent_token(&task_rs::NewAgentRunToken {
            agent: &agent,
            project_id: req.project_id,
            card_id: req.card_id,
            thread_id: req.thread_id,
            machine: &host_spec::host_spec().hostname,
        })
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(Json(AgentTokenReply {
        token: issued.token,
        agent: issued.row.agent,
        project_id: issued.row.project_id,
        card_id: issued.row.card_id,
        thread_id: issued.row.thread_id,
        expires_at: issued.row.expires_at.to_rfc3339(),
    }))
}

async fn auth_user_from_store(
    store: &std::sync::Arc<task_rs::Store>,
    headers: &http::HeaderMap,
) -> Result<task_rs::UserRow, ApiError> {
    let token = headers
        .get(http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix(BEARER_PREFIX))
        .filter(|t| !t.starts_with(task_rs::TOKEN_PREFIX))
        .ok_or_else(|| ApiError(UNAUTHORIZED_MSG.to_string(), StatusCode::UNAUTHORIZED))?;
    store
        .auth(token)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError(UNAUTHORIZED_MSG.to_string(), StatusCode::UNAUTHORIZED))
}

#[derive(Deserialize, utoipa::ToSchema)]
struct AgentResultRequest {
    result: String,
}

#[derive(Serialize, utoipa::ToSchema)]
struct AgentResultReply {
    card_comment: bool,
    thread_message: bool,
}

/// Agent callback authenticated by a run ticket: routes the result to the
/// card and/or thread bound to the ticket, then consumes the ticket.
async fn agent_results(
    State(state): State<TaskStore>,
    AuthAgent(ticket): AuthAgent,
    Json(req): Json<AgentResultRequest>,
) -> Result<Json<AgentResultReply>, ApiError> {
    let mut card_comment = false;
    let mut thread_message = false;
    if let Some(card_id) = ticket.card_id {
        state
            .store
            .add_comment(card_id, &ticket.agent, &req.result)
            .await
            .map_err(|e| ApiError::bad_request(e.to_string()))?;
        card_comment = true;
    }
    if let Some(thread_id) = ticket.thread_id {
        state
            .store
            .add_chat_message(thread_id, task_rs::ROLE_ASSISTANT, &req.result)
            .await
            .map_err(|e| ApiError::bad_request(e.to_string()))?;
        thread_message = true;
    }
    state
        .store
        .consume_agent_token(&ticket.token_hash)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(AgentResultReply {
        card_comment,
        thread_message,
    }))
}

#[derive(Serialize, utoipa::ToSchema)]
struct ProjectAgentDto {
    name: String,
    card_ids: Vec<i64>,
    threads: i64,
    running: bool,
    machine: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct ProjectAgentsReply {
    agents: Vec<ProjectAgentDto>,
}

/// Who works in this project: agents attached to cards, agents owning chat
/// threads, and agents currently holding a live run ticket.
async fn project_agents(
    State(state): State<TaskStore>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    _user: AuthUser,
) -> Result<Json<ProjectAgentsReply>, ApiError> {
    let cards = state
        .store
        .list(Some(id))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let threads = state
        .store
        .list_chat_threads(Some(id))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let runs = state
        .store
        .list_active_agent_tokens()
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let mut merged: std::collections::BTreeMap<String, ProjectAgentDto> =
        std::collections::BTreeMap::new();
    for card in &cards {
        if let Some(name) = card.agent_name.as_deref() {
            let entry = merged
                .entry(name.to_owned())
                .or_insert_with(|| ProjectAgentDto {
                    name: name.to_owned(),
                    card_ids: Vec::new(),
                    threads: 0,
                    running: false,
                    machine: None,
                });
            entry.card_ids.push(card.id);
        }
        // A run record (last_agent) also surfaces the agent on the project,
        // without being a preference on the card.
        if let Some(name) = card.last_agent.as_deref()
            && card.run_status == task_rs::RUN_STATUS_RUNNING
        {
            let entry = merged
                .entry(name.to_owned())
                .or_insert_with(|| ProjectAgentDto {
                    name: name.to_owned(),
                    card_ids: Vec::new(),
                    threads: 0,
                    running: false,
                    machine: None,
                });
            entry.running = true;
        }
    }
    for thread in &threads {
        let entry = merged
            .entry(thread.agent.clone())
            .or_insert_with(|| ProjectAgentDto {
                name: thread.agent.clone(),
                card_ids: Vec::new(),
                threads: 0,
                running: false,
                machine: None,
            });
        entry.threads += 1;
    }
    for run in &runs {
        if run.project_id != Some(id) {
            continue;
        }
        let entry = merged
            .entry(run.agent.clone())
            .or_insert_with(|| ProjectAgentDto {
                name: run.agent.clone(),
                card_ids: Vec::new(),
                threads: 0,
                running: false,
                machine: None,
            });
        entry.running = true;
        if !run.machine.is_empty() {
            entry.machine = Some(run.machine.clone());
        }
    }
    Ok(Json(ProjectAgentsReply {
        agents: merged.into_values().collect(),
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
struct AgentGitRequest {
    /// `clone` needs `url`; `branch` needs `name`; `commit` needs `message`;
    /// `push` takes `branch`; `pr` takes `title`/`head`/`base`; `status` and
    /// `diff` take no arguments. `project_id` supplies the stored repo
    /// secret when `push`/`pr` carry no explicit `token`.
    op: String,
    url: Option<String>,
    token: Option<String>,
    name: Option<String>,
    message: Option<String>,
    branch: Option<String>,
    title: Option<String>,
    head: Option<String>,
    base: Option<String>,
    project_id: Option<i64>,
}

impl AgentGitRequest {
    fn tool(&self) -> Result<GitTool, ApiError> {
        match self.op.as_str() {
            "clone" => Ok(GitTool::Clone {
                url: self
                    .url
                    .clone()
                    .ok_or_else(|| ApiError::bad_request("clone needs url"))?,
                token: self.token.clone(),
            }),
            "status" => Ok(GitTool::Status),
            "diff" => Ok(GitTool::Diff),
            "branch" => Ok(GitTool::Branch {
                name: self
                    .name
                    .clone()
                    .filter(|n| !n.is_empty())
                    .ok_or_else(|| ApiError::bad_request("branch needs name"))?,
            }),
            "commit" => Ok(GitTool::Commit {
                message: self
                    .message
                    .clone()
                    .filter(|m| !m.is_empty())
                    .ok_or_else(|| ApiError::bad_request("commit needs message"))?,
            }),
            "push" => Ok(GitTool::Push {
                branch: self
                    .branch
                    .clone()
                    .filter(|b| !b.is_empty())
                    .ok_or_else(|| ApiError::bad_request("push needs branch"))?,
                url: self.url.clone(),
                token: self.token.clone(),
            }),
            "pr" => Ok(GitTool::PullRequest {
                title: self
                    .title
                    .clone()
                    .filter(|t| !t.is_empty())
                    .ok_or_else(|| ApiError::bad_request("pr needs title"))?,
                head: self.head.clone().unwrap_or_default(),
                base: self.base.clone().unwrap_or_default(),
                url: self.url.clone(),
                token: self.token.clone(),
            }),
            other => Err(ApiError::bad_request(format!(
                "unknown git op {other} (clone | status | diff | branch | commit | push | pr)"
            ))),
        }
    }
}

/// Host-side doc note: clone/status/diff run in the work tree on the host;
/// branch/commit/push/pr run inside the named agent's own container.
async fn run_agent_git(
    State(state): State<ManagerState>,
    Path(agent): Path<String>,
    Json(mut req): Json<AgentGitRequest>,
) -> Result<Json<AgentRunReply>, ApiError> {
    if req.token.is_none()
        && let Some(id) = req.project_id
    {
        req.token = state.settings.git_repo(id).and_then(|r| r.secret);
    }
    let tool = req.tool()?;
    let manager = Arc::clone(&state.manager);
    let git_agent = agent.clone();
    let output = tokio::task::spawn_blocking(move || manager.git_tool(&git_agent, &tool))
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
        .map_err(ApiError::bad_request)?;
    Ok(Json(AgentRunReply { agent, output }))
}

async fn agent_logs(
    State(state): State<ManagerState>,
    Path(agent): Path<String>,
) -> Result<Json<AgentLogsReply>, ApiError> {
    let manager = state.manager;
    let logs_agent = agent.clone();
    let logs = tokio::task::spawn_blocking(move || manager.logs(&logs_agent))
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
        .map_err(ApiError::bad_request)?;
    Ok(Json(AgentLogsReply {
        agent: logs.agent,
        work_tree: logs.work_tree.display().to_string(),
        runs: logs.runs,
        transcript: logs.transcript,
        last_result: logs.last_result,
        machine: host_spec::host_spec().hostname,
    }))
}

#[derive(Serialize, utoipa::ToSchema)]
struct AgentLogsReply {
    agent: String,
    work_tree: String,
    runs: u64,
    transcript: Vec<String>,
    last_result: Option<String>,
    /// Hostname of the machine holding this agent's sandbox.
    machine: String,
}

#[derive(Serialize, utoipa::ToSchema)]
struct AgentWhereReply {
    agent: String,
    /// Hostname of the machine the agent runs on.
    machine: String,
    local: bool,
}

/// Which machine holds `agent`: the backend's own manager first, then every
/// registered client machine (hub agent round trip). 404 when nowhere.
async fn agent_whereis(
    State(state): State<MachinesState>,
    Path(agent): Path<String>,
) -> Result<Json<AgentWhereReply>, ApiError> {
    if agent.is_empty() {
        return Err(ApiError::bad_request("agent name required"));
    }
    let manager = state.manager;
    let agent_name = agent.clone();
    let found = tokio::task::spawn_blocking(move || {
        if manager.snapshot().iter().any(|a| a.agent == agent_name) {
            return Some((host_spec::host_spec().hostname, true));
        }
        remote_machines(&host_spec::host_spec().hostname)
            .into_iter()
            .find(|m| m.agents.iter().any(|a| a.name == agent_name))
            .map(|m| (m.hostname, false))
    })
    .await
    .map_err(|e| ApiError::bad_request(e.to_string()))?;
    match found {
        Some((machine, local)) => Ok(Json(AgentWhereReply {
            agent,
            machine,
            local,
        })),
        None => Err(ApiError::not_found(format!(
            "agent {agent} is not running on any machine"
        ))),
    }
}

/// Optional body for the finish route: `{ push: true, project_id?, branch? }`.
/// Pushes the task's branch (repo + token from the project's bound git repo)
/// before the work tree is torn down.
#[derive(Deserialize, utoipa::ToSchema)]
struct FinishRequest {
    #[serde(default)]
    push: bool,
    /// Project whose bound git repo supplies the push url + token.
    #[serde(default)]
    project_id: Option<i64>,
    /// Overrides the pushed branch (default: task branch, else current).
    #[serde(default)]
    branch: Option<String>,
}

impl FinishRequest {
    fn finish_push(&self, settings: &SettingsState) -> Option<manager_rs::manager::FinishPush> {
        if !self.push {
            return None;
        }
        let repo = self.project_id.and_then(|id| settings.git_repo(id));
        Some(manager_rs::manager::FinishPush {
            repo_url: repo.as_ref().map(|r| r.url.clone()),
            token: repo.and_then(|r| r.secret).unwrap_or_default(),
            branch: self.branch.clone(),
        })
    }
}

async fn finish_agent(
    State(state): State<ManagerState>,
    Path(agent): Path<String>,
    req: Option<Json<FinishRequest>>,
) -> Result<Json<StoredOutcome>, ApiError> {
    const STATUS_FINISHED: &str = "finished";
    const STATUS_ERROR: &str = "error";
    let settings = Arc::new(SettingsState::load());
    let finish_agent_name = agent.clone();
    let manager = Arc::clone(&state.manager);
    // Optional push: repo + token come from the project's bound git repo.
    let finish_push = req.and_then(|Json(r)| r.finish_push(&state.settings));
    let outcome = tokio::task::spawn_blocking(move || {
        manager.finish_with_push(&finish_agent_name, finish_push)
    })
    .await
    .map_err(|e| ApiError::bad_request(e.to_string()))?;
    match &outcome {
        Ok(out) => {
            let (settings, agent, status) =
                (Arc::clone(&settings), out.agent.clone(), STATUS_FINISHED);
            let _ = tokio::task::spawn_blocking(move || {
                crate::infra::alerts::notify_agent_finished(&settings, &agent, status)
            })
            .await;
        }
        Err(_) => {
            let (settings, agent, status) = (Arc::clone(&settings), agent.clone(), STATUS_ERROR);
            let _ = tokio::task::spawn_blocking(move || {
                crate::infra::alerts::notify_agent_finished(&settings, &agent, status)
            })
            .await;
        }
    }
    let outcome = outcome.map_err(ApiError::bad_request)?;
    // Persist the task output (patch + transcript) before the work tree is
    // gone; review happens against the stored copy.
    let transcript = outcome
        .state
        .history
        .iter()
        .map(|e| format!("{:?}: {}", e.role, e.content))
        .collect::<Vec<_>>()
        .join("\n");
    let output_id = state
        .store
        .insert_agent_output(
            &outcome.agent,
            outcome.result.as_deref(),
            &outcome.patch,
            outcome.commit.as_deref(),
            &transcript,
        )
        .await
        .map_err(store_err)?;
    Ok(Json(StoredOutcome {
        agent: outcome.agent,
        result: outcome.result,
        patch: outcome.patch,
        commit: outcome.commit,
        output_id,
        push: outcome.push.map(|r| match r {
            Ok(out) => {
                let out = out.trim();
                if out.is_empty() {
                    "pushed".to_string()
                } else {
                    out.to_string()
                }
            }
            Err(e) => format!("push failed: {e}"),
        }),
    }))
}

#[derive(Serialize, utoipa::ToSchema)]
struct StoredOutcome {
    agent: String,
    result: Option<String>,
    patch: String,
    commit: Option<String>,
    output_id: i64,
    /// Present when the finish asked for a push: remote output, or the
    /// failure reason prefixed with "push failed:".
    #[serde(skip_serializing_if = "Option::is_none")]
    push: Option<String>,
}

/// Installer for remote sandbox clients; `role` selects auto/model/worker.
/// Unauthenticated by design — it only probes the downloading machine.
async fn install_script(
    axum::extract::Query(q): axum::extract::Query<crate::infra::client::install::InstallQuery>,
) -> Response {
    use crate::infra::client::install;
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
        .unwrap_or_else(|| "http://127.0.0.1:8991".to_owned());
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
        zai_quota,
        quota_board,
        zai_hi,
        get_system_prompt,
        set_system_prompt,
        get_client_env,
        set_client_env,
        get_alert_settings,
        set_alert_settings,
        get_local_endpoint,
        set_local_endpoint,
        chat_zai,
        render_prompt,
        list_sandboxes,
        host_spec_handler,
        purge_sandbox,
        sweep_sandboxes,
        list_cards,
        create_card,
        update_card,
        move_card,
        remove_card,
        get_agent,
        set_agent,
        list_card_runs,
        run_card,
        set_card_image,
        set_card_schedule,
        list_cronjobs,
        list_comments,
        add_comment,
        list_activity,
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
        ArtifactDto,
        ClaudeCodeRequest,
        ClaudeChatRequest,
        ZaiSettingsReply,
        ZaiSettingsRequest,
        ZaiQuotaReply,
        QuotaBoard,
        crate::infra::provider_quota::PlatformQuota,
        ZaiHiReply,
        ZaiModelReply,
        ZaiModelAction,
        SystemPromptReply,
        SystemPromptRequest,
        ClientEnvReply,
        ClientEnvRequest,
        AlertSettingsReply,
        AlertSettingsRequest,
        LocalEndpointReply,
        LocalEndpointRequest,
        ZaiChatRequest,
        PromptSectionDto,
        RenderPromptRequest,
        RenderPromptReply,
        SandboxDirInfo,
        HostSpec,
        SandboxPurgeRequest,
        SandboxSweepReply,
        CardDto,
        AgentDto,
        crate::app::card_run::RunRecord,
        CreateCardRequest,
        ListCardsQuery,
        MoveCardRequest,
        SetAgentRequest,
        SetScheduleRequest,
        CronJobDto,
        crate::app::schedule_work::CronJobState,
        UpdateCardRequest,
        CommentDto,
        ActivityDto,
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
    /// Agent name from the chat modal; registers the exchange with the
    /// manager so the agent shows in machines with its transcript.
    agent: Option<String>,
    /// Chat thread id; scopes long-term memory recall/remember to the thread.
    thread_id: Option<i64>,
    /// Target task card: tool artifacts produced this turn (written files,
    /// images, text output) are attached to it as card resources.
    #[serde(default)]
    card_id: Option<i64>,
}
/// One tool call the agent made, for display in the chat UI.
#[derive(Serialize, utoipa::ToSchema)]
struct ToolUseDto {
    tool: String,
    input: String,
    ok: bool,
    summary: String,
    /// Files the call produced (also attached to the turn's target card).
    artifacts: Vec<ArtifactDto>,
}

impl ToolUseDto {
    fn from_use(u: &ToolUse) -> Self {
        Self {
            tool: TOOL_KIND_NAMES
                .iter()
                .find(|(_, k)| *k == u.kind)
                .map_or("unknown", |(n, _)| *n)
                .to_string(),
            input: u.input.clone(),
            ok: u.ok,
            summary: u.summary.clone(),
            artifacts: u
                .artifacts
                .iter()
                .map(|a| ArtifactDto {
                    kind: a.kind.as_str().to_string(),
                    path: a.path.clone(),
                })
                .collect(),
        }
    }
}

/// One file a tool call produced; also attached to the turn's target card.
#[derive(Serialize, utoipa::ToSchema)]
struct ArtifactDto {
    /// "image" | "text" | "file"
    kind: String,
    /// Workspace-relative path.
    path: String,
}

#[derive(Serialize, utoipa::ToSchema)]
struct ChatReply {
    model: Option<String>,
    reply: String,
    searched: bool,
    /// Tool calls made this turn, in order (shell, search, fetch, board).
    tools: Vec<ToolUseDto>,
    /// Recalled memory lines used as context; empty = memory not used.
    memories: Vec<String>,
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
    /// Daily automatic z.ai ping time, `HH:MM` in the configured timezone.
    say_hi_time: Option<String>,
    /// When set, the ping repeats every N minutes after the start time.
    say_hi_interval_mins: Option<u64>,
    /// IANA timezone used for the schedule and quota reset display.
    timezone: Option<String>,
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
    /// `HH:MM` daily say-hi schedule. Omit to leave unchanged, null to clear.
    say_hi_time: Option<Option<String>>,
    /// Repeat interval in minutes. Omit to leave unchanged, null for once a day.
    say_hi_interval_mins: Option<Option<u64>>,
    /// IANA timezone name. Omit to leave unchanged, null for server-local time.
    timezone: Option<Option<String>>,
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
    /// Agent name from the chat modal; registers the exchange with the
    /// manager so the agent shows in machines with its transcript.
    agent: Option<String>,
    /// Server-side thread id; when set, the turn is stored in Postgres.
    thread_id: Option<i64>,
    /// Optional image as a data URL (`data:image/png;base64,…`). Gated by the
    /// agent's `receive_images` flag.
    #[serde(default)]
    image: Option<String>,
    /// Target task card: tool artifacts produced this turn are attached to
    /// it as card resources.
    #[serde(default)]
    card_id: Option<i64>,
    /// Client-generated id for this run; when set, the run can be cancelled
    /// via `/api/chat/zai/cancel`.
    #[serde(default)]
    run_id: Option<String>,
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
    /// JSON array of label strings, e.g. ["bug","infra"].
    labels: Option<String>,
    /// JSON array of {text, done} objects.
    checklist: Option<String>,
    /// Story points.
    estimate: Option<i32>,
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
    /// Last recorded run status; a record, not a preference.
    run_status: String,
    /// Agent that executed the most recent run.
    last_agent: Option<String>,
    assignee: Option<String>,
    /// Sandbox image used by the card's runs; null = default.
    image: Option<String>,
    cron: Option<String>,
    deadline: Option<String>,
    /// JSON array of label strings.
    labels: Option<String>,
    /// JSON array of {text, done} objects.
    checklist: Option<String>,
    estimate: Option<i32>,
}

impl From<task_rs::CardRow> for CardDto {
    fn from(r: task_rs::CardRow) -> Self {
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
            run_status: r.run_status,
            last_agent: r.last_agent,
            assignee: r.assignee,
            image: r.image,
            cron: r.cron,
            deadline: r.deadline,
            labels: r.labels,
            checklist: r.checklist,
            estimate: r.estimate,
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
struct UpdateCardRequest {
    title: String,
    description: String,
    /// Person assigned to this card; null to unassign.
    assignee: Option<String>,
    /// Target date (ISO `YYYY-MM-DD`); empty string clears it.
    deadline: Option<String>,
    /// "low" | "normal" | "high" | "critical"; null leaves unchanged.
    priority: Option<String>,
    /// JSON array of label strings; null leaves unchanged, empty string clears.
    labels: Option<String>,
    /// JSON array of {text, done} objects; null leaves unchanged, empty string clears.
    checklist: Option<String>,
    /// Story points; null leaves unchanged.
    estimate: Option<i32>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct CommentDto {
    id: i64,
    card_id: i64,
    author: String,
    body: String,
    created_at: String,
}

impl From<task_rs::CommentRow> for CommentDto {
    fn from(c: task_rs::CommentRow) -> Self {
        Self {
            id: c.id,
            card_id: c.card_id,
            author: c.author,
            body: c.body,
            created_at: c.created_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
struct ActivityDto {
    id: i64,
    kind: String,
    message: String,
    created_at: String,
}

impl From<task_rs::ActivityRow> for ActivityDto {
    fn from(r: task_rs::ActivityRow) -> Self {
        Self {
            id: r.id,
            kind: r.kind,
            message: r.message,
            created_at: r.created_at.to_rfc3339(),
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
struct AddCommentRequest {
    body: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct SetCardImageRequest {
    /// Sandbox image name, or null to clear.
    image: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct SetScheduleRequest {
    /// 5-field cron expression (UTC), or null to unschedule the card.
    cron: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct ResourceDto {
    id: i64,
    card_id: i64,
    name: String,
    content: String,
    created_at: String,
}

impl From<&task_rs::ResourceRow> for ResourceDto {
    fn from(r: &task_rs::ResourceRow) -> Self {
        Self {
            id: r.id,
            card_id: r.card_id,
            name: r.name.clone(),
            content: r.content.clone(),
            created_at: r.created_at.to_rfc3339(),
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
    /// Tool allow-list (search|fetch|shell|board); empty = all tools.
    #[serde(default)]
    allowed_tools: Vec<String>,
    /// False = the agent must never receive images. Defaults to true.
    #[serde(default = "default_true")]
    receive_images: bool,
    /// Reasoning depth: "off" (default) | "low" | "medium" | "high".
    #[serde(default)]
    thinking: Option<String>,
}

fn default_true() -> bool {
    true
}

fn valid_thinking(level: Option<&str>) -> Result<String, ApiError> {
    let Some(level) = level.map(str::trim).filter(|l| !l.is_empty()) else {
        return Ok(task_rs::ThinkLevel::default().as_str().to_owned());
    };
    task_rs::ThinkLevel::parse(level)
        .map(|l| l.as_str().to_owned())
        .ok_or_else(|| ApiError::bad_request(format!("unknown thinking level: {level}")))
}

#[derive(Serialize, utoipa::ToSchema)]
struct AgentConfigDto {
    id: i64,
    name: String,
    model: String,
    persona: String,
    prompt: String,
    output: String,
    allowed_tools: Vec<String>,
    receive_images: bool,
    thinking: String,
}

impl From<task_rs::AgentConfigRow> for AgentConfigDto {
    fn from(r: task_rs::AgentConfigRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            model: r.model,
            persona: r.persona,
            prompt: r.prompt,
            output: r.output,
            allowed_tools: r.allowed_tools,
            receive_images: r.receive_images,
            thinking: r.thinking,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
struct SkillDto {
    id: i64,
    name: String,
    body: String,
}

impl From<task_rs::SkillRow> for SkillDto {
    fn from(s: task_rs::SkillRow) -> Self {
        Self {
            id: s.id,
            name: s.name,
            body: s.body,
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
struct SkillRequest {
    name: String,
    body: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct AgentSkillRequest {
    skill_id: i64,
}

#[derive(Serialize, utoipa::ToSchema)]
struct AgentDto {
    name: String,
    state: serde_json::Value,
    /// Runner kind: "human" | "pinned".
    runner: String,
    run_status: String,
    last_agent: Option<String>,
}

impl From<task_rs::AgentState> for AgentDto {
    fn from(a: task_rs::AgentState) -> Self {
        Self {
            name: a.name,
            state: a.state,
            runner: task_rs::RUNNER_HUMAN.into(),
            run_status: task_rs::RUN_STATUS_IDLE.into(),
            last_agent: None,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
struct CardRunDto {
    id: i64,
    trigger: String,
    agent: String,
    ok: bool,
    summary: String,
    started_at: String,
    finished_at: Option<String>,
}

impl From<task_rs::RunRecordRow> for CardRunDto {
    fn from(r: task_rs::RunRecordRow) -> Self {
        Self {
            id: r.id,
            trigger: r.trigger,
            agent: r.agent,
            ok: r.ok,
            summary: r.summary,
            started_at: r.started_at.to_rfc3339(),
            finished_at: r.finished_at.map(|t| t.to_rfc3339()),
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

impl From<task_rs::UserRow> for UserDto {
    fn from(u: task_rs::UserRow) -> Self {
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

impl From<task_rs::WorkspaceRow> for WorkspaceDto {
    fn from(w: task_rs::WorkspaceRow) -> Self {
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

impl From<task_rs::ProjectRow> for ProjectDto {
    fn from(p: task_rs::ProjectRow) -> Self {
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

    fn not_found(msg: impl Into<String>) -> Self {
        Self(msg.into(), StatusCode::NOT_FOUND)
    }

    fn forbidden(msg: impl Into<String>) -> Self {
        Self(msg.into(), StatusCode::FORBIDDEN)
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
        tracing::debug!(status = status.as_u16(), error = %msg, "api error");
        (status, msg).into_response()
    }
}
