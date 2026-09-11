//! Kanban application layer: composition root. Wires repo ports into the
//! 1:1 domain services and composes cross-repo views (pipeline names on
//! cards) — not SQL.

use std::collections::HashMap;
use std::sync::Arc;

use kanban_rs::{CardRow, StoreError};

use crate::domain::{
    AgentConfigService, CardService, CommentService, PipelineService, ProjectService,
    ResourceService, WorkspaceService,
};
use crate::port::outbound::{
    AgentConfigRepo, CardRepo, CommentRepo, PipelineRepo, ProjectRepo, ResourceRepo, WorkspaceRepo,
};

/// A card enriched with its pipeline name — composed in the app layer.
pub struct CardView {
    pub card: CardRow,
    pub pipeline_name: Option<String>,
}

pub struct KanbanApp {
    pub cards: CardService,
    pub comments: CommentService,
    pub pipelines: PipelineService,
    pub resources: ResourceService,
    pub agents: AgentConfigService,
    pub workspaces: WorkspaceService,
    pub projects: ProjectService,
}

impl KanbanApp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cards: Arc<dyn CardRepo>,
        comments: Arc<dyn CommentRepo>,
        pipelines: Arc<dyn PipelineRepo>,
        resources: Arc<dyn ResourceRepo>,
        agents: Arc<dyn AgentConfigRepo>,
        workspaces: Arc<dyn WorkspaceRepo>,
        projects: Arc<dyn ProjectRepo>,
    ) -> Self {
        Self {
            cards: CardService::new(cards),
            comments: CommentService::new(comments),
            pipelines: PipelineService::new(pipelines),
            resources: ResourceService::new(resources),
            agents: AgentConfigService::new(agents),
            workspaces: WorkspaceService::new(workspaces),
            projects: ProjectService::new(projects),
        }
    }

    pub async fn card_views(&self, project_id: Option<i64>) -> Result<Vec<CardView>, StoreError> {
        let cards = self.cards.list(project_id).await?;
        if cards.is_empty() {
            return Ok(Vec::new());
        }
        let names: HashMap<i64, String> = self
            .pipelines
            .list()
            .await?
            .into_iter()
            .map(|p| (p.id, p.name))
            .collect();
        Ok(cards
            .into_iter()
            .map(|card| CardView {
                pipeline_name: card.pipeline_id.and_then(|id| names.get(&id).cloned()),
                card,
            })
            .collect())
    }

    pub async fn card_view(&self, id: i64) -> Result<Option<CardView>, StoreError> {
        let Some(card) = self.cards.get(id).await? else {
            return Ok(None);
        };
        let pipeline_name = match card.pipeline_id {
            Some(pid) => self
                .pipelines
                .list()
                .await?
                .into_iter()
                .find(|p| p.id == pid)
                .map(|p| p.name),
            None => None,
        };
        Ok(Some(CardView {
            card,
            pipeline_name,
        }))
    }
}

/// Composition root: wires the Postgres adapters into the app services.
pub fn build(store: Arc<kanban_rs::Store>) -> KanbanApp {
    use crate::infra::kanban::PgKanban;
    let pg = Arc::new(PgKanban::new(store));
    KanbanApp::new(
        Arc::clone(&pg) as Arc<dyn CardRepo>,
        Arc::clone(&pg) as Arc<dyn CommentRepo>,
        Arc::clone(&pg) as Arc<dyn PipelineRepo>,
        Arc::clone(&pg) as Arc<dyn ResourceRepo>,
        Arc::clone(&pg) as Arc<dyn AgentConfigRepo>,
        Arc::clone(&pg) as Arc<dyn WorkspaceRepo>,
        pg as Arc<dyn ProjectRepo>,
    )
}
