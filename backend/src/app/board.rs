//! Application layer: kanban board operations exposed to the chat tool loop.
//! Authorization lives here: a valid bearer token with editor role is required
//! for every operation.

use std::sync::Arc;

use async_trait::async_trait;
use kanban_rs::Role;

use crate::domain::{BoardOp, BoardRequest, BoardResult};
use crate::port::outbound::BoardOps;

pub struct BoardService {
    store: Arc<kanban_rs::Store>,
}

impl BoardService {
    pub fn new(store: Arc<kanban_rs::Store>) -> Self {
        Self { store }
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

#[async_trait]
impl BoardOps for BoardService {
    async fn exec(&self, req: BoardRequest) -> BoardResult {
        self.editor(&req.token).await?;
        match req.op {
            BoardOp::CreatePipeline { name } => {
                let spec = serde_json::json!({ "stages": [] }).to_string();
                let id = self
                    .store
                    .create_pipeline(&name, &spec)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(format!("pipeline {id} created: {name}"))
            }
            BoardOp::CreateCard { project_id, title } => {
                let card = kanban_rs::AddCard {
                    project_id: Some(project_id),
                    column_id: kanban_rs::DEFAULT_COLUMNS[0].0,
                    title: &title,
                    description: "",
                    priority: kanban_rs::PRIORITY_NORMAL,
                    labels: None,
                    checklist: None,
                    estimate: None,
                };
                let id = self.store.add(card).await.map_err(|e| e.to_string())?;
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
                match cron {
                    Some(expr) => Ok(format!(
                        "card {card_id} scheduled with cron `{expr}`{CRON_HINT}"
                    )),
                    None => Ok(format!("card {card_id} unscheduled")),
                }
            }
            BoardOp::Summary => {
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
                    let cron = match &card.cron {
                        Some(expr) => format!(", cron `{expr}`"),
                        None => String::new(),
                    };
                    let project = match card.project_id {
                        Some(p) => format!(" (project {p})"),
                        None => String::new(),
                    };
                    lines.push(format!(
                        "card {} `{}`{}{}",
                        card.id, card.title, project, cron
                    ));
                }
                Ok(lines.join("\n"))
            }
        }
    }
}
