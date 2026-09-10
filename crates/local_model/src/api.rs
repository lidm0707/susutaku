//! HTTP API over the model pool and the client hub.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use susutaku_mlx::tok::TokKind;

use crate::hub::Hub;
use crate::ports::{GenReply, Inference, ModelSwitch};
use crate::{DEFAULT_MAX_TOKENS, MODELS_ROOT};

pub struct AppState {
    pub pool: Arc<dyn ModelSwitch>,
    pub inference: Arc<dyn Inference>,
    pub hub: Arc<Hub>,
}

#[derive(Serialize)]
pub struct ModelDto {
    pub name: String,
    pub loadable: bool,
    pub bytes: u64,
    pub engine: String,
    pub selected: bool,
}

#[derive(Deserialize)]
pub struct SelectRequest {
    pub name: String,
}

#[derive(Deserialize, Serialize)]
pub struct InferenceRequest {
    pub prompt: String,
    pub max_tokens: Option<usize>,
    pub tok: Option<String>,
    pub think: Option<bool>,
}

#[derive(Serialize)]
pub struct GenStatsDto {
    pub prompt_tokens: usize,
    pub prompt_tps: f64,
    pub decode_tokens: usize,
    pub decode_tps: f64,
}

#[derive(Serialize)]
pub struct GenReplyDto {
    pub model: String,
    pub text: String,
    pub stats: GenStatsDto,
}

#[derive(Deserialize)]
pub struct CommandRequest {
    pub cmd: String,
}

#[derive(Serialize)]
pub struct CommandReply {
    pub output: String,
}

fn stats_dto(stats: &susutaku_mlx::stats::GenStats) -> GenStatsDto {
    GenStatsDto {
        prompt_tokens: stats.prompt_tokens,
        prompt_tps: stats.prompt_tps(),
        decode_tokens: stats.decode_tokens,
        decode_tps: stats.decode_tps(),
    }
}

fn reply_dto(reply: GenReply) -> GenReplyDto {
    GenReplyDto {
        model: reply.model,
        text: reply.text,
        stats: stats_dto(&reply.stats),
    }
}

async fn models(State(state): State<Arc<AppState>>) -> Json<Vec<ModelDto>> {
    let selected = state.pool.selected();
    Json(
        hf_loader::loadable_models(std::path::Path::new(MODELS_ROOT))
            .iter()
            .map(|m| ModelDto {
                name: m.name.clone(),
                loadable: m.is_loadable(),
                bytes: m.bytes,
                engine: m.engine().to_string(),
                selected: selected.as_deref() == Some(m.name.as_str()),
            })
            .collect(),
    )
}

async fn select_model(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SelectRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state
        .pool
        .select(&req.name)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(serde_json::json!({ "selected": req.name })))
}

async fn inference(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InferenceRequest>,
) -> Result<Json<GenReplyDto>, (StatusCode, String)> {
    let tok = TokKind::parse(req.tok.as_deref());
    let max_tokens = req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
    let rx = state
        .inference
        .submit(req.prompt, max_tokens, tok, req.think.unwrap_or(false))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let reply = rx
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "job dropped".to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(reply_dto(reply)))
}

async fn clients(State(state): State<Arc<AppState>>) -> Json<Vec<u64>> {
    Json(state.hub.client_ids())
}

async fn client_command(
    State(state): State<Arc<AppState>>,
    Path(client_id): Path<u64>,
    Json(req): Json<CommandRequest>,
) -> Result<Json<CommandReply>, (StatusCode, String)> {
    let output = state
        .hub
        .dispatch(client_id, req.cmd)
        .await
        .map_err(|e| (StatusCode::GATEWAY_TIMEOUT, e))?;
    Ok(Json(CommandReply { output }))
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/models", get(models))
        .route("/api/models/select", post(select_model))
        .route("/api/inference", post(inference))
        .route("/api/clients", get(clients))
        .route("/api/clients/{id}/command", post(client_command))
        .with_state(state)
}
