//! Executes a card's attached pipeline: topological stage walk over the spec,
//! per-stage log, result merged into the card's agent state.

use std::collections::VecDeque;

use kanban_rs::resource::UpsertResource;
use kanban_rs::store::{AgentState, CardRow, StoreError};
use kanban_rs::{COLUMN_DOING, COLUMN_DONE, COLUMN_FAILED};
use piplines::agent::{AgentNode, META_AGENT};
use piplines::graph::{NodeDef, PipelineSpec};
use piplines::payload::{Payload, PayloadKind};
use piplines::stage::Stage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

use super::kanban::KanbanApp;
use crate::domain::CardMove;

pub const RUN_KEY: &str = "run";
pub const RUNNER_NAME: &str = "pipeline-runner";
pub const MOVE_POSITION_TOP: i32 = 0;
pub const FETCH_TIMEOUT_SECS: u64 = 10;
pub const FETCH_MAX_BYTES: usize = 1 << 20;
pub const HTTP_GET: &str = "GET";
pub const NETWORK_HINT: &str =
    "(backend has no outbound network access — check host/container network)";
pub const STAGE_NOTE_INGEST: &str = "seeded card title + description";
pub const STAGE_NOTE_PASSTHROUGH: &str = "payload passed through";
pub const STAGE_NOTE_OUTPUT: &str = "captured pipeline output";
pub const STAGE_NOTE_RESOURCE: &str = "stored resource: ";
pub const TEXT_SEP: &str = "\n\n";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Ok,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunStage {
    pub node: String,
    pub stage: String,
    pub status: StageStatus,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunRecord {
    pub pipeline_id: i64,
    pub pipeline_name: String,
    pub status: StageStatus,
    pub stages: Vec<RunStage>,
    pub output: Option<String>,
    pub resources: Vec<String>,
    pub finished_at: String,
}

/// Result of a run attempt: record to persist + whether it should replace the agent name.
pub struct RunOutcome {
    pub record: RunRecord,
    pub agent_name: String,
    pub resources: Vec<(String, String)>,
}

pub async fn run_card_pipeline(app: &KanbanApp, card_id: i64) -> Result<RunRecord, StoreError> {
    let card = app
        .cards
        .get(card_id)
        .await?
        .ok_or(StoreError::NoSuchCard)?;
    let pipeline_id = card.pipeline_id.ok_or(StoreError::NoSuchPipeline)?;
    let pipeline = app
        .pipelines
        .list()
        .await?
        .into_iter()
        .find(|p| p.id == pipeline_id)
        .ok_or(StoreError::NoSuchPipeline)?;
    let spec: PipelineSpec =
        serde_json::from_str(&pipeline.spec).map_err(|e| StoreError::BadSpec(e.to_string()))?;
    spec.validate()
        .map_err(|e| StoreError::BadSpec(e.to_string()))?;
    move_to_column(app, card_id, &card.column_id, COLUMN_DOING).await?;

    let outcome = execute(&spec, seed_payload(&card), pipeline_id, &pipeline.name).await;
    persist(app, card_id, &card, outcome).await
}

/// Move a card unless it already sits in the target column.
async fn move_to_column(
    app: &KanbanApp,
    card_id: i64,
    current: &str,
    target: &str,
) -> Result<(), StoreError> {
    if current == target {
        return Ok(());
    }
    app.cards
        .move_card(CardMove {
            id: card_id,
            column_id: target.to_owned(),
            position: MOVE_POSITION_TOP,
        })
        .await
}

/// Dry-run a saved pipeline with caller-supplied seed text: no card, no
/// resource writes, no agent-state changes. Returns the run record only.
pub async fn test_pipeline(
    app: &KanbanApp,
    pipeline_id: i64,
    seed_text: &str,
) -> Result<RunRecord, StoreError> {
    let pipeline = app
        .pipelines
        .list()
        .await?
        .into_iter()
        .find(|p| p.id == pipeline_id)
        .ok_or(StoreError::NoSuchPipeline)?;
    let spec: PipelineSpec =
        serde_json::from_str(&pipeline.spec).map_err(|e| StoreError::BadSpec(e.to_string()))?;
    spec.validate()
        .map_err(|e| StoreError::BadSpec(e.to_string()))?;
    let outcome = execute(&spec, Payload::text(seed_text), pipeline_id, &pipeline.name).await;
    Ok(outcome.record)
}

fn seed_payload(card: &CardRow) -> Payload {
    let mut text = card.title.clone();
    if !card.description.is_empty() {
        text.push_str(TEXT_SEP);
        text.push_str(&card.description);
    }
    Payload::text(text)
}

async fn execute(spec: &PipelineSpec, seed: Payload, pipeline_id: i64, name: &str) -> RunOutcome {
    let mut stages: Vec<RunStage> = Vec::new();
    let mut failed = false;
    // Output of each completed node, routed to consumers via links.
    let mut done: HashMap<&str, Payload> = HashMap::new();
    let mut resources: Vec<(String, String)> = Vec::new();
    let mut final_output: Option<Payload> = None;
    let mut agent_name = RUNNER_NAME.to_owned();
    for node in topo_order(spec) {
        let inputs = inputs_for(spec, node, &seed, &done);
        let Some(payload) = inputs else {
            stages.push(skipped(node));
            continue;
        };
        if failed {
            stages.push(skipped(node));
            continue;
        }
        match apply_node(node, payload).await {
            Ok(next) => {
                if let Some(res) = next.resource {
                    resources.push(res);
                }
                if let Some(agent) = next.payload.get_meta(META_AGENT) {
                    agent_name = agent.to_owned();
                }
                final_output = Some(next.payload.clone());
                done.insert(node.id.as_str(), next.payload);
                stages.push(RunStage {
                    node: node.id.clone(),
                    stage: node.stage.clone(),
                    status: StageStatus::Ok,
                    note: next.note,
                });
            }
            Err(note) => {
                failed = true;
                stages.push(RunStage {
                    node: node.id.clone(),
                    stage: node.stage.clone(),
                    status: StageStatus::Failed,
                    note,
                });
            }
        }
    }
    // Output = last text payload produced, falling back to the seed.
    let output = final_output.and_then(|p| p.as_str().map(str::to_owned));
    let record = RunRecord {
        pipeline_id,
        pipeline_name: name.to_owned(),
        status: if failed {
            StageStatus::Failed
        } else {
            StageStatus::Ok
        },
        stages,
        output,
        resources: resources.iter().map(|(name, _)| name.clone()).collect(),
        finished_at: now_iso(),
    };
    RunOutcome {
        agent_name,
        resources,
        record,
    }
}

/// Gather inputs for a node: join its link predecessors' outputs; roots take
/// the seed. None if a predecessor did not produce output (failed/skipped).
fn inputs_for(
    spec: &PipelineSpec,
    node: &NodeDef,
    seed: &Payload,
    done: &HashMap<&str, Payload>,
) -> Option<Payload> {
    let preds: Vec<&str> = spec
        .links
        .iter()
        .filter(|l| l.to == node.id)
        .map(|l| l.from.as_str())
        .collect();
    if preds.is_empty() {
        return Some(seed.clone());
    }
    let mut texts: Vec<&str> = Vec::new();
    let mut first: Option<Payload> = None;
    for pred in preds {
        let payload = done.get(pred)?;
        if let Some(text) = payload.as_str() {
            texts.push(text);
        }
        if first.is_none() {
            first = Some(payload.clone());
        }
    }
    let mut merged = first?;
    if texts.len() > 1 {
        merged.data = texts.join(TEXT_SEP).into_bytes();
        merged.kind = PayloadKind::Text;
    }
    Some(merged)
}

struct NodeResult {
    payload: Payload,
    note: String,
    /// (name, content) for output_resource nodes, persisted post-run.
    resource: Option<(String, String)>,
}

impl NodeResult {
    fn passthrough(payload: Payload, note: String) -> Self {
        Self {
            payload,
            note,
            resource: None,
        }
    }
}

async fn apply_node(node: &NodeDef, payload: Payload) -> Result<NodeResult, String> {
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

pub const PARAM_NAME: &str = "name";

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

pub const PARAM_URL: &str = "url";
pub const PARAM_METHOD: &str = "method";
pub const FETCH_NOTE_PREFIX: &str = "fetched ";

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
        .unwrap_or_else(|| HTTP_GET.to_owned());
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

fn http_fetch(method: &str, url: &str, body: &str) -> Result<String, String> {
    use std::io::Read;
    let timeout = std::time::Duration::from_secs(FETCH_TIMEOUT_SECS);
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    let resp = if method == HTTP_GET {
        agent.get(url).call()
    } else {
        agent.request(method, url).send_string(body)
    };
    match resp {
        Ok(resp) => {
            // Cap the read in bytes before buffering the whole body.
            let mut reader = resp.into_reader().take(FETCH_MAX_BYTES as u64);
            let mut text = String::new();
            reader
                .read_to_string(&mut text)
                .map(|_| text)
                .map_err(|e| format!("fetch {url}: failed to read body: {e}"))
        }
        Err(ureq::Error::Status(code, resp)) => {
            let detail = resp.into_string().unwrap_or_default();
            Err(format!(
                "fetch {method} {url}: http {code} {detail} {NETWORK_HINT}"
            ))
        }
        Err(ureq::Error::Transport(t)) => {
            Err(format!("fetch {method} {url} failed: {t} {NETWORK_HINT}"))
        }
    }
}

pub const PARAM_QUERY: &str = "query";
pub const PARAM_PATH: &str = "path";
pub const META_IMAGE_PATH: &str = "image_path";
pub const REF_IMAGE_NOTE_PREFIX: &str = "loaded image ";
pub const REF_IMAGE_NOTE_BYTES: &str = " bytes";

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

pub const OP_UPPER: &str = "upper";
pub const OP_LOWER: &str = "lower";
pub const OP_TRIM: &str = "trim";

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

fn skipped(node: &NodeDef) -> RunStage {
    RunStage {
        node: node.id.clone(),
        stage: node.stage.clone(),
        status: StageStatus::Failed,
        note: "skipped: earlier stage failed".to_owned(),
    }
}

/// Kahn ordering over the validated acyclic spec; falls back to spec order.
fn topo_order(spec: &PipelineSpec) -> Vec<&NodeDef> {
    let idx = |id: &str| spec.nodes.iter().position(|n| n.id == id);
    let mut indegree = vec![0usize; spec.nodes.len()];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); spec.nodes.len()];
    for link in &spec.links {
        if let (Some(from), Some(to)) = (idx(&link.from), idx(&link.to)) {
            adj[from].push(to);
            indegree[to] += 1;
        }
    }
    let mut queue: VecDeque<usize> = (0..spec.nodes.len())
        .filter(|&i| indegree[i] == 0)
        .collect();
    let mut ordered: Vec<&NodeDef> = Vec::with_capacity(spec.nodes.len());
    while let Some(i) = queue.pop_front() {
        ordered.push(&spec.nodes[i]);
        for &next in &adj[i] {
            indegree[next] -= 1;
            if indegree[next] == 0 {
                queue.push_back(next);
            }
        }
    }
    if ordered.len() == spec.nodes.len() {
        ordered
    } else {
        spec.nodes.iter().collect()
    }
}

async fn persist(
    app: &KanbanApp,
    card_id: i64,
    card: &CardRow,
    outcome: RunOutcome,
) -> Result<RunRecord, StoreError> {
    for (name, content) in &outcome.resources {
        app.resources
            .upsert(UpsertResource {
                card_id,
                name,
                content,
            })
            .await?;
    }
    let mut state = card
        .agent_state
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
    let obj = state
        .as_object_mut()
        .ok_or_else(|| StoreError::BadSpec("agent state is not a JSON object".into()))?;
    obj.insert(
        RUN_KEY.to_owned(),
        serde_json::to_value(&outcome.record).map_err(|e| StoreError::BadSpec(e.to_string()))?,
    );
    let name = if card.agent_name.is_some() {
        card.agent_name.clone().expect("checked above")
    } else {
        outcome.agent_name
    };
    app.cards
        .set_agent(card_id, &AgentState { name, state })
        .await?;
    let target = match outcome.record.status {
        StageStatus::Ok => COLUMN_DONE,
        StageStatus::Failed => COLUMN_FAILED,
    };
    move_to_column(app, card_id, &card.column_id, target).await?;
    Ok(outcome.record)
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}
