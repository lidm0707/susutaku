//! Application layer: kanban board operations exposed to the chat tool loop.
//! Authorization lives here: a valid bearer token with editor role is required
//! for every operation.

use std::sync::Arc;

use async_trait::async_trait;
use kanban_rs::Role;

use crate::domain::{BoardOp, BoardRequest, BoardResult};
use crate::port::outbound::{BoardOps, Inference};

pub struct BoardService {
    store: Arc<kanban_rs::Store>,
    app: super::kanban::KanbanApp,
    engine: Option<Arc<dyn Inference>>,
}

impl BoardService {
    pub fn new(store: Arc<kanban_rs::Store>, engine: Option<Arc<dyn Inference>>) -> Self {
        let app = super::kanban::build(store.clone());
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
const EMPTY_SPEC: &str = r#"{"nodes":[],"links":[]}"#;
const NO_CARDS_MATCH: &str = "no cards match ";
const NO_PIPELINE: &str = "none";
const NO_CRON: &str = "none";
const FIND_HITS_MAX: usize = 8;
const TITLE_WEIGHT: u32 = 3;
const EXACT_SUBSTR_SCORE: u32 = 100;

fn card_line(card: &kanban_rs::CardRow) -> String {
    let cron = card.cron.as_deref().unwrap_or(NO_CRON);
    let pipeline = card
        .pipeline_id
        .map(|p| p.to_string())
        .unwrap_or_else(|| NO_PIPELINE.to_owned());
    let project = card
        .project_id
        .map(|p| p.to_string())
        .unwrap_or_else(|| "-".to_owned());
    format!(
        "card {} `{}` (project {}, pipeline {}, cron {})",
        card.id, card.title, project, pipeline, cron
    )
}

fn run_summary(card_id: i64, r: &super::pipeline_run::RunRecord) -> String {
    const OUTPUT_PREVIEW_MAX: usize = 400;
    let status = match r.status {
        super::pipeline_run::StageStatus::Ok => "ok",
        super::pipeline_run::StageStatus::Failed => "failed",
    };
    let mut line = format!(
        "card {card_id} ran pipeline {} `{}`: {status}",
        r.pipeline_id, r.pipeline_name
    );
    if let Some(out) = &r.output {
        let preview: String = out.chars().take(OUTPUT_PREVIEW_MAX).collect();
        line.push_str("\noutput: ");
        line.push_str(preview.trim());
    }
    line
}

/// Token-overlap relevance: exact substring wins, then weighted matches of
/// each query token in the title (×3) and description (×1).
fn find_score(card: &kanban_rs::CardRow, tokens: &[String]) -> u32 {
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
            BoardOp::CreatePipeline { name, spec } => {
                let spec = spec
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| EMPTY_SPEC.to_owned());
                let id = self
                    .store
                    .create_pipeline(&name, &spec)
                    .await
                    .map_err(|e| e.to_string())?;
                crate::app::events::publish(crate::app::events::EventKind::Pipeline);
                Ok(format!("pipeline {id} created: {name}"))
            }
            BoardOp::CreateCard {
                project_id,
                title,
                description,
            } => {
                let description = description.unwrap_or_default();
                let card = kanban_rs::AddCard {
                    project_id: Some(project_id),
                    column_id: kanban_rs::DEFAULT_COLUMNS[0].0,
                    title: &title,
                    description: &description,
                    priority: kanban_rs::PRIORITY_NORMAL,
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
            BoardOp::LinkPipeline {
                card_id,
                pipeline_id,
            } => {
                self.store
                    .set_card_pipeline(card_id, Some(pipeline_id))
                    .await
                    .map_err(|e| e.to_string())?;
                crate::app::events::publish(crate::app::events::EventKind::Card);
                Ok(format!("pipeline {pipeline_id} attached to card {card_id}"))
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
                let record =
                    super::pipeline_run::run_card_pipeline(&self.app, self.engine.clone(), card_id)
                        .await
                        .map_err(|e| e.to_string())?;
                if let Err(e) = self
                    .store
                    .record_activity("run", &format!("started pipeline run for task {card_id}"))
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
                let mut hits: Vec<(u32, &kanban_rs::CardRow)> = cards
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
        for pipeline in self
            .store
            .list_pipelines()
            .await
            .map_err(|e| e.to_string())?
        {
            lines.push(format!("pipeline {} `{}`", pipeline.id, pipeline.name));
        }
        for card in self.store.list(None).await.map_err(|e| e.to_string())? {
            lines.push(card_line(&card));
        }
        Ok(lines.join("\n"))
    }
}
