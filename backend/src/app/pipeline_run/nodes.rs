//! Per-node executors: one handler per pipeline stage.

use std::sync::Arc;

use kanban_rs::AgentConfigRow;
use piplines::agent::{AgentNode, META_AGENT};
use piplines::graph::NodeDef;
use piplines::payload::{Payload, PayloadKind};
use piplines::stage::Stage;
use susutaku_mlx::tok::TokKind;

use crate::port::outbound::Inference;

use super::http::http_fetch;
use super::model::{
    INFER_MAX_TOKENS, NOTE_INFER_DONE, NOTE_INFER_DROP, NOTE_INFER_ERR, NOTE_NO_ENGINE, NodeResult,
    PROMPT_INPUT, PROMPT_INSTRUCTION, PROMPT_OUTPUT, PROMPT_PERSONA, RunStage, STAGE_NOTE_INGEST,
    STAGE_NOTE_PASSTHROUGH, STAGE_NOTE_RESOURCE, StageStatus,
};

pub const PARAM_NAME: &str = "name";
pub const PARAM_URL: &str = "url";
pub const PARAM_METHOD: &str = "method";
pub const PARAM_QUERY: &str = "query";
pub const PARAM_PATH: &str = "path";
pub const FETCH_NOTE_PREFIX: &str = "fetched ";
pub const META_IMAGE_PATH: &str = "image_path";
pub const REF_IMAGE_NOTE_PREFIX: &str = "loaded image ";
pub const REF_IMAGE_NOTE_BYTES: &str = " bytes";
pub const OP_UPPER: &str = "upper";
pub const OP_LOWER: &str = "lower";
pub const OP_TRIM: &str = "trim";

pub async fn apply_node(
    node: &NodeDef,
    payload: Payload,
    engine: Option<&Arc<dyn Inference>>,
    agent: Option<&AgentConfigRow>,
) -> Result<NodeResult, String> {
    match node.stage.as_str() {
        piplines::graph::STAGE_INGEST => Ok(NodeResult::passthrough(
            payload,
            STAGE_NOTE_INGEST.to_owned(),
        )),
        piplines::graph::STAGE_AGENT => {
            let agent = AgentNode::from_params(&node.params)?;
            let next = agent.apply(payload).map_err(|e| e.to_string())?;
            let note = format!(
                "agent set to {}",
                next.get_meta(META_AGENT).unwrap_or_default()
            );
            Ok(NodeResult::passthrough(next, note))
        }
        piplines::graph::STAGE_FETCH => fetch_node(node, payload).await,
        piplines::graph::STAGE_SEARCH => search_node(node, payload).await,
        piplines::graph::STAGE_REF_IMAGE => ref_image_node(node, payload).await,
        piplines::graph::STAGE_TRANSFORM => transform(node, payload),
        piplines::graph::STAGE_MODEL_INFER => infer_node(payload, engine, agent).await,
        piplines::graph::STAGE_OUTPUT_RESOURCE => output_resource(node, payload),
        piplines::graph::STAGE_RENDER => Ok(NodeResult::passthrough(
            payload,
            STAGE_NOTE_PASSTHROUGH.to_owned(),
        )),
        other => Err(format!(
            "stage \"{other}\" is not wired to an engine yet (search/ref_image/parse/model_infer pending)"
        )),
    }
}

/// Calls the model with the resolved agent's settings (persona, instruction,
/// output format) prepended to the payload text; the reply replaces the text.
async fn infer_node(
    payload: Payload,
    engine: Option<&Arc<dyn Inference>>,
    agent: Option<&AgentConfigRow>,
) -> Result<NodeResult, String> {
    let Some(engine) = engine else {
        return Err(NOTE_NO_ENGINE.to_owned());
    };
    let mut prompt = String::new();
    if let Some(cfg) = agent {
        for (header, body) in [
            (PROMPT_PERSONA, cfg.persona.as_str()),
            (PROMPT_INSTRUCTION, cfg.prompt.as_str()),
            (PROMPT_OUTPUT, cfg.output.as_str()),
        ] {
            if !body.is_empty() {
                prompt.push_str(header);
                prompt.push_str(body);
                prompt.push('\n');
            }
        }
    }
    prompt.push_str(PROMPT_INPUT);
    prompt.push_str(payload.as_str().unwrap_or_default());
    let rx = engine
        .submit(prompt, INFER_MAX_TOKENS, TokKind::Normal, false)
        .map_err(|e| format!("{NOTE_INFER_ERR}{e}"))?;
    let reply = rx
        .await
        .map_err(|_| NOTE_INFER_DROP.to_string())?
        .map_err(|e| format!("{NOTE_INFER_ERR}{e}"))?;
    let chars = reply.text.chars().count();
    let agent_meta = payload.get_meta(META_AGENT).map(str::to_owned);
    let mut next = Payload::text(reply.text);
    if let Some(name) = agent_meta {
        next.set_meta(META_AGENT, name);
    }
    Ok(NodeResult::passthrough(
        next,
        format!("{NOTE_INFER_DONE} {chars} chars"),
    ))
}

fn output_resource(node: &NodeDef, payload: Payload) -> Result<NodeResult, String> {
    let name = node
        .params
        .get(PARAM_NAME)
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("output_resource node params missing \"{PARAM_NAME}\""))?
        .to_owned();
    let content = payload
        .as_str()
        .ok_or("output_resource needs a text payload")?
        .to_owned();
    let note = format!("{}{name} ({} chars)", STAGE_NOTE_RESOURCE, content.len());
    Ok(NodeResult {
        payload,
        note,
        resource: Some((name, content)),
    })
}

async fn fetch_node(node: &NodeDef, mut payload: Payload) -> Result<NodeResult, String> {
    let url = node
        .params
        .get(PARAM_URL)
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("fetch node params missing \"{PARAM_URL}\""))?
        .to_owned();
    let method = node
        .params
        .get(PARAM_METHOD)
        .and_then(serde_json::Value::as_str)
        .map(str::to_ascii_uppercase)
        .unwrap_or_else(|| super::http::HTTP_GET.to_owned());
    // ureq is blocking; keep it off the async executor threads.
    let body = payload.as_str().unwrap_or_default().to_owned();
    let fetch_url = url.clone();
    let text = tokio::task::spawn_blocking(move || http_fetch(&method, &fetch_url, &body))
        .await
        .map_err(|e| format!("fetch task join failed: {e}"))??;
    let note = format!("{FETCH_NOTE_PREFIX}{} bytes from {url}", text.len());
    payload.data = text.into_bytes();
    payload.kind = PayloadKind::Text;
    Ok(NodeResult::passthrough(payload, note))
}

async fn search_node(node: &NodeDef, mut payload: Payload) -> Result<NodeResult, String> {
    // empty query param falls back to the incoming text as the search term
    let from_params = node
        .params
        .get(PARAM_QUERY)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let query = if from_params.is_empty() {
        payload.as_str().unwrap_or_default().to_owned()
    } else {
        from_params
    };
    if query.trim().is_empty() {
        return Err("search node needs a query param or a text payload".to_owned());
    }
    // core-agent web_search is blocking (ureq); keep it off the async executor.
    let note_query = query.clone();
    let results =
        tokio::task::spawn_blocking(move || core_agent::toolcall::web_search::search(&query))
            .await
            .map_err(|e| format!("search task join failed: {e}"))??;
    let note = format!("searched \"{note_query}\": {} results", results.len());
    payload.data = core_agent::toolcall::web_search::SearchResult::summarize(&results).into_bytes();
    payload.kind = PayloadKind::Text;
    Ok(NodeResult::passthrough(payload, note))
}

async fn ref_image_node(node: &NodeDef, mut payload: Payload) -> Result<NodeResult, String> {
    let path = node
        .params
        .get(PARAM_PATH)
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("ref_image node params missing \"{PARAM_PATH}\""))?
        .to_owned();
    let data = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("ref_image {path}: {e}"))?;
    let note = format!(
        "{REF_IMAGE_NOTE_PREFIX}{path} ({}{REF_IMAGE_NOTE_BYTES})",
        data.len()
    );
    payload.data = data;
    payload.kind = PayloadKind::Binary;
    payload.set_meta(META_IMAGE_PATH, path);
    Ok(NodeResult::passthrough(payload, note))
}

fn transform(node: &NodeDef, mut payload: Payload) -> Result<NodeResult, String> {
    let op = node
        .params
        .get("op")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "transform node params missing \"op\"".to_string())?;
    let text = payload.as_str().ok_or("transform needs a text payload")?;
    let transformed = match op {
        OP_UPPER => text.to_uppercase(),
        OP_LOWER => text.to_lowercase(),
        OP_TRIM => text.trim().to_owned(),
        other => return Err(format!("unknown transform op: {other}")),
    };
    payload.data = transformed.into_bytes();
    payload.kind = PayloadKind::Text;
    Ok(NodeResult::passthrough(payload, format!("op {op} applied")))
}

pub fn skipped(node: &NodeDef) -> RunStage {
    RunStage {
        node: node.id.clone(),
        stage: node.stage.clone(),
        status: StageStatus::Failed,
        note: "skipped: earlier stage failed".to_owned(),
    }
}
