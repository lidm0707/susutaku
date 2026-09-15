//! Application layer: task board operations exposed to the chat tool loop.
//! Authorization lives here: a valid bearer token with editor role is required
//! for every operation.

use std::sync::Arc;

use async_trait::async_trait;
use task_rs::Role;

use crate::domain::{BoardOp, BoardRequest, BoardResult};
use crate::port::outbound::{BoardOps, Inference};

pub struct BoardService {
    store: Arc<task_rs::Store>,
    app: super::task::TaskApp,
    engine: Option<Arc<dyn Inference>>,
}

impl BoardService {
    pub fn new(store: Arc<task_rs::Store>, engine: Option<Arc<dyn Inference>>) -> Self {
        let app = super::task::build(store.clone());
        Self { store, app, engine }
    }

    async fn editor(&self, token: &Option<String>) -> Result<Role, String> {
        let token = token.as_deref().ok_or(FORBIDDEN)?;
        let user = self
            .store
            .auth(token)
            .await
            .map_err(|e| e.to_string())?
            .ok_or(FORBIDDEN)?;
        let role = user.role.parse::<Role>().map_err(|e| e.to_string())?;
        if role.can_edit() {
            Ok(role)
        } else {
            Err(FORBIDDEN.to_string())
        }
    }
}

const FORBIDDEN: &str = "forbidden: editor role required";
const CRON_HINT: &str = " (5-field cron, UTC, e.g. 0 */5 * * * for every 5 hours)";
const NO_CARDS_MATCH: &str = "no cards match ";
const NO_AGENT: &str = "no-agent";
const NO_IMAGE: &str = "no-image";
const NO_CRON: &str = "none";
const FIND_HITS_MAX: usize = 8;
const TITLE_WEIGHT: u32 = 3;
const EXACT_SUBSTR_SCORE: u32 = 100;

fn card_line(card: &task_rs::CardRow) -> String {
    let cron = card.cron.as_deref().unwrap_or(NO_CRON);
    let agent = card.agent_name.as_deref().unwrap_or(NO_AGENT);
    let image = card.image.as_deref().unwrap_or(NO_IMAGE);
    let project = card
        .project_id
        .map(|p| p.to_string())
        .unwrap_or_else(|| "-".to_owned());
    format!(
        "card {} `{}` (project {}, agent {}, image {}, cron {})",
        card.id, card.title, project, agent, image, cron
    )
}

fn run_summary(card_id: i64, r: &super::card_run::RunRecord) -> String {
    const OUTPUT_PREVIEW_MAX: usize = 400;
    let status = match r.status {
        super::card_run::RunStatus::Ok => "ok",
        super::card_run::RunStatus::Failed => "failed",
    };
    let mut line = format!("card {card_id} ran agent {}: {status}", r.agent);
    if let Some(out) = &r.output {
        let preview: String = out.chars().take(OUTPUT_PREVIEW_MAX).collect();
        line.push_str("\noutput: ");
        line.push_str(preview.trim());
    }
    line
}

/// Token-overlap relevance: exact substring wins, then weighted matches of
/// each query token in the title (×3) and description (×1).
fn find_score(card: &task_rs::CardRow, tokens: &[String]) -> u32 {
    const MISS: u32 = 0;
    let title = card.title.to_lowercase();
    let desc = card.description.to_lowercase();
    let joined = format!("{title} {desc}");
    if tokens.iter().all(|t| joined.contains(t)) {
        return EXACT_SUBSTR_SCORE;
    }
    let score = tokens
        .iter()
        .map(|t| {
            if title.contains(t) {
                TITLE_WEIGHT
            } else if desc.contains(t) {
                1
            } else {
                MISS
            }
        })
        .sum::<u32>();
    if tokens.iter().any(|t| joined.contains(t)) {
        score
    } else {
        MISS
    }
}

#[async_trait]
impl BoardOps for BoardService {
    async fn exec(&self, req: BoardRequest) -> BoardResult {
        self.editor(&req.token).await?;
        match req.op {
            BoardOp::CreateCard {
                project_id,
                title,
                description,
            } => {
                let description = description.unwrap_or_default();
                let card = task_rs::AddCard {
                    project_id: Some(project_id),
                    column_id: task_rs::DEFAULT_COLUMNS[0].0,
                    title: &title,
                    description: &description,
                    priority: task_rs::PRIORITY_NORMAL,
                    labels: None,
                    checklist: None,
                    estimate: None,
                };
                let id = self.store.add(card).await.map_err(|e| e.to_string())?;
                crate::app::events::publish(crate::app::events::EventKind::Card);
                Ok(format!(
                    "card {id} created in project {project_id}: {title}"
                ))
            }
            BoardOp::AssignAgent { card_id, agent } => {
                // Pin the agent, keep the existing run-ledger state.
                let state = self
                    .store
                    .agent(card_id)
                    .await
                    .map_err(|e| e.to_string())?
                    .map(|a| a.state)
                    .unwrap_or(serde_json::Value::Null);
                self.store
                    .set_agent(
                        card_id,
                        &task_rs::AgentState {
                            name: agent.clone(),
                            state,
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                crate::app::events::publish(crate::app::events::EventKind::Card);
                Ok(format!("agent {agent} assigned to card {card_id}"))
            }
            BoardOp::SetImage { card_id, image } => {
                self.store
                    .set_card_image(card_id, image.as_deref())
                    .await
                    .map_err(|e| e.to_string())?;
                crate::app::events::publish(crate::app::events::EventKind::Card);
                match image {
                    Some(img) => Ok(format!("image `{img}` set on card {card_id}")),
                    None => Ok(format!("image cleared on card {card_id}")),
                }
            }
            BoardOp::SetCron { card_id, cron } => {
                if let Some(expr) = &cron {
                    work::services::cron::Cron::parse(expr).map_err(|e| e.to_string())?;
                }
                self.store
                    .set_cron(card_id, cron.as_deref())
                    .await
                    .map_err(|e| e.to_string())?;
                crate::app::events::publish(crate::app::events::EventKind::Cron);
                match cron {
                    Some(expr) => Ok(format!("card {card_id} routine set: `{expr}`{CRON_HINT}")),
                    None => Ok(format!("card {card_id} routine cleared")),
                }
            }
            BoardOp::RunCard { card_id } => {
                let record = super::card_run::run_card(
                    &self.app,
                    self.engine.clone(),
                    card_id,
                    task_rs::TRIGGER_MANUAL,
                )
                .await
                .map_err(|e| e.to_string())?;
                if let Err(e) = self
                    .store
                    .record_activity("run", &format!("started agent run for task {card_id}"))
                    .await
                {
                    tracing::warn!(error = %e, "activity log write failed");
                }
                Ok(run_summary(card_id, &record))
            }
            BoardOp::Summary | BoardOp::FindCards { .. } => {
                if let BoardOp::Summary = req.op {
                    return self.summary().await;
                }
                let BoardOp::FindCards { query } = &req.op else {
                    unreachable!("matched above")
                };
                let tokens: Vec<String> = query
                    .trim()
                    .to_lowercase()
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect();
                if tokens.is_empty() {
                    return Ok(format!("{NO_CARDS_MATCH}`{query}`"));
                }
                let cards = self.store.list(None).await.map_err(|e| e.to_string())?;
                let mut hits: Vec<(u32, &task_rs::CardRow)> = cards
                    .iter()
                    .map(|c| (find_score(c, &tokens), c))
                    .filter(|(s, _)| *s > 0)
                    .collect();
                if hits.is_empty() {
                    return Ok(format!("{NO_CARDS_MATCH}`{query}`"));
                }
                hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.id.cmp(&b.1.id)));
                hits.truncate(FIND_HITS_MAX);
                Ok(hits
                    .iter()
                    .map(|(_, c)| card_line(c))
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
        }
    }
}

impl BoardService {
    async fn summary(&self) -> Result<String, String> {
        let workspaces = self
            .store
            .list_workspaces()
            .await
            .map_err(|e| e.to_string())?;
        let mut lines = Vec::new();
        for ws in &workspaces {
            for project in self
                .store
                .list_projects(ws.id)
                .await
                .map_err(|e| e.to_string())?
            {
                lines.push(format!(
                    "project {} `{}` (workspace {})",
                    project.id, project.name, ws.id
                ));
            }
        }
        for card in self.store.list(None).await.map_err(|e| e.to_string())? {
            lines.push(card_line(&card));
        }
        Ok(lines.join("\n"))
    }
}
