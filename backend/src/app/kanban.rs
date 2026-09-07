//! Kanban application layer: one service per repo (1:1) and the composition
//! root. Cross-repo composition (pipeline names on cards) happens here, not
//! in SQL.

use std::collections::HashMap;
use std::sync::Arc;

use kanban_rs::{
    AgentConfigRow, AgentState, CardRow, CommentRow, PipelineRow, ProjectRow, StoreError,
    WorkspaceRow,
};

use crate::port::outbound::{
    AgentConfigDraft, AgentConfigRepo, CardMove, CardPatch, CardRepo, CommentRepo, NewCard,
    NewPipeline, NewProject, NewWorkspace, PipelineRepo, ProjectRepo, WorkspaceRepo,
};

pub struct CardService {
    repo: Arc<dyn CardRepo>,
}

impl CardService {
    pub fn new(repo: Arc<dyn CardRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self, project_id: Option<i64>) -> Result<Vec<CardRow>, StoreError> {
        self.repo.list(project_id).await
    }

    pub async fn get(&self, id: i64) -> Result<Option<CardRow>, StoreError> {
        self.repo.get(id).await
    }

    /// Write + read-back inside one transaction.
    pub async fn create(&self, card: NewCard) -> Result<CardRow, StoreError> {
        let mut tx = self.repo.tx().await?;
        let id = tx.add(card).await?;
        let row = tx.get(id).await?.ok_or(StoreError::NoSuchCard)?;
        tx.commit().await?;
        Ok(row)
    }

    /// Edit + read-back inside one transaction.
    pub async fn update(&self, patch: CardPatch) -> Result<CardRow, StoreError> {
        let id = patch.id;
        let mut tx = self.repo.tx().await?;
        tx.update(patch).await?;
        let row = tx.get(id).await?.ok_or(StoreError::NoSuchCard)?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn move_card(&self, mv: CardMove) -> Result<(), StoreError> {
        self.repo.move_card(mv).await
    }

    pub async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.repo.remove(id).await
    }

    pub async fn set_agent(&self, id: i64, agent: &AgentState) -> Result<(), StoreError> {
        self.repo.set_agent(id, agent).await
    }

    pub async fn agent(&self, id: i64) -> Result<Option<AgentState>, StoreError> {
        self.repo.agent(id).await
    }

    pub async fn set_pipeline(
        &self,
        card_id: i64,
        pipeline_id: Option<i64>,
    ) -> Result<(), StoreError> {
        self.repo.set_pipeline(card_id, pipeline_id).await
    }
}

pub struct CommentService {
    repo: Arc<dyn CommentRepo>,
}

impl CommentService {
    pub fn new(repo: Arc<dyn CommentRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self, card_id: i64) -> Result<Vec<CommentRow>, StoreError> {
        self.repo.list(card_id).await
    }

    /// Exists-check + insert inside one transaction.
    pub async fn add(
        &self,
        card_id: i64,
        author: String,
        body: String,
    ) -> Result<CommentRow, StoreError> {
        let mut tx = self.repo.tx().await?;
        if !tx.card_exists(card_id).await? {
            tx.rollback().await?;
            return Err(StoreError::NoSuchCard);
        }
        let row = tx.add(card_id, &author, &body).await?;
        tx.commit().await?;
        Ok(row)
    }
}

pub struct PipelineService {
    repo: Arc<dyn PipelineRepo>,
}

impl PipelineService {
    pub fn new(repo: Arc<dyn PipelineRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self) -> Result<Vec<PipelineRow>, StoreError> {
        self.repo.list().await
    }

    /// Create + read-back inside one transaction.
    pub async fn create(&self, pipeline: NewPipeline) -> Result<PipelineRow, StoreError> {
        let mut tx = self.repo.tx().await?;
        let id = tx.create(pipeline).await?;
        let row = tx.get(id).await?.ok_or(StoreError::NoSuchPipeline)?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn update(&self, id: i64, pipeline: NewPipeline) -> Result<(), StoreError> {
        self.repo.update(id, pipeline).await
    }

    pub async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.repo.remove(id).await
    }
}

pub struct AgentConfigService {
    repo: Arc<dyn AgentConfigRepo>,
}

impl AgentConfigService {
    pub fn new(repo: Arc<dyn AgentConfigRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self) -> Result<Vec<AgentConfigRow>, StoreError> {
        self.repo.list().await
    }

    pub async fn create(&self, cfg: AgentConfigDraft) -> Result<AgentConfigRow, StoreError> {
        self.repo.create(cfg).await
    }

    pub async fn update(&self, id: i64, cfg: AgentConfigDraft) -> Result<(), StoreError> {
        self.repo.update(id, cfg).await
    }

    pub async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.repo.remove(id).await
    }
}

pub struct WorkspaceService {
    repo: Arc<dyn WorkspaceRepo>,
}

impl WorkspaceService {
    pub fn new(repo: Arc<dyn WorkspaceRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self) -> Result<Vec<WorkspaceRow>, StoreError> {
        self.repo.list().await
    }

    pub async fn create(&self, ws: NewWorkspace) -> Result<WorkspaceRow, StoreError> {
        self.repo.create(ws).await
    }

    pub async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.repo.remove(id).await
    }
}

pub struct ProjectService {
    repo: Arc<dyn ProjectRepo>,
}

impl ProjectService {
    pub fn new(repo: Arc<dyn ProjectRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self, workspace_id: i64) -> Result<Vec<ProjectRow>, StoreError> {
        self.repo.list(workspace_id).await
    }

    pub async fn create(&self, p: NewProject) -> Result<ProjectRow, StoreError> {
        self.repo.create(p).await
    }

    pub async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.repo.remove(id).await
    }
}

/// A card enriched with its pipeline name — composed in the app layer.
pub struct CardView {
    pub card: CardRow,
    pub pipeline_name: Option<String>,
}

pub struct KanbanApp {
    pub cards: CardService,
    pub comments: CommentService,
    pub pipelines: PipelineService,
    pub agents: AgentConfigService,
    pub workspaces: WorkspaceService,
    pub projects: ProjectService,
}

impl KanbanApp {
    pub fn new(
        cards: Arc<dyn CardRepo>,
        comments: Arc<dyn CommentRepo>,
        pipelines: Arc<dyn PipelineRepo>,
        agents: Arc<dyn AgentConfigRepo>,
        workspaces: Arc<dyn WorkspaceRepo>,
        projects: Arc<dyn ProjectRepo>,
    ) -> Self {
        Self {
            cards: CardService::new(cards),
            comments: CommentService::new(comments),
            pipelines: PipelineService::new(pipelines),
            agents: AgentConfigService::new(agents),
            workspaces: WorkspaceService::new(workspaces),
            projects: ProjectService::new(projects),
        }
    }

    pub async fn card_views(
        &self,
        project_id: Option<i64>,
    ) -> Result<Vec<CardView>, StoreError> {
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
        Ok(Some(CardView { card, pipeline_name }))
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
        Arc::clone(&pg) as Arc<dyn AgentConfigRepo>,
        Arc::clone(&pg) as Arc<dyn WorkspaceRepo>,
        pg as Arc<dyn ProjectRepo>,
    )
}
