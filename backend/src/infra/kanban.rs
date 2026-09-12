//! Infrastructure: Postgres adapter implementing the kanban outbound ports
//! over `kanban_rs::Store`.

use std::sync::Arc;

use async_trait::async_trait;
use kanban_rs::{
    AddCard, AgentConfigRow, AgentConfigUpdate, AgentState, CardRow, CommentRow, DbTx, MoveCard,
    NewSkill, PipelineRow, ProjectRow, ResourceRow, SkillRow, Store, StoreError, UpdateCard,
    UpsertResource, WorkspaceRow,
};

use crate::domain::{
    AgentConfigDraft, CardMove, CardPatch, NewCard, NewPipeline, NewProject, NewWorkspace,
};
use crate::port::outbound::{
    AgentConfigRepo, CardRepo, CardTx, CommentRepo, CommentTx, PipelineRepo, PipelineTx,
    ProjectRepo, ResourceRepo, SkillRepo, WorkspaceRepo,
};

pub const DATABASE_URL_ENV: &str = "DATABASE_URL";

const CONNECT_RETRIES: usize = 5;
const CONNECT_RETRY_DELAY_SECS: u64 = 2;

pub async fn connect() -> Store {
    let url = std::env::var(DATABASE_URL_ENV).unwrap_or_else(|_| Store::default_url().into());
    let mut store = None;
    for attempt in 1..=CONNECT_RETRIES {
        match Store::connect(&url).await {
            Ok(s) => {
                store = Some(s);
                break;
            }
            Err(err) => {
                eprintln!(
                    "kanban postgres connect (attempt {attempt}/{CONNECT_RETRIES}): {err:?} \
                     — is the postgres container up? (docker/compose/base.yml, host port 5434)"
                );
                if attempt < CONNECT_RETRIES {
                    tokio::time::sleep(std::time::Duration::from_secs(CONNECT_RETRY_DELAY_SECS))
                        .await;
                }
            }
        }
    }
    let store = store.expect("kanban postgres connect: gave up");
    store
        .ensure_default_admin()
        .await
        .expect("seed default admin");
    store
        .ensure_default_board()
        .await
        .expect("seed default workspace/project");
    store
}

/// Postgres-backed implementation of every kanban repo port.
pub struct PgKanban {
    store: Arc<Store>,
}

impl PgKanban {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    fn store(&self) -> &Store {
        &self.store
    }
}

fn as_add(card: &NewCard) -> AddCard<'_> {
    AddCard {
        project_id: card.project_id,
        column_id: &card.column_id,
        title: &card.title,
        description: &card.description,
        priority: &card.priority,
        labels: card.labels.as_deref(),
        checklist: card.checklist.as_deref(),
        estimate: card.estimate,
    }
}

fn as_move(mv: &CardMove) -> MoveCard<'_> {
    MoveCard {
        id: mv.id,
        column_id: &mv.column_id,
        position: mv.position,
    }
}

fn as_patch(patch: &CardPatch) -> UpdateCard<'_> {
    UpdateCard {
        id: patch.id,
        title: &patch.title,
        description: &patch.description,
        assignee: patch.assignee.as_deref(),
        deadline: patch.deadline.as_deref(),
        priority: patch.priority.as_deref(),
        labels: patch.labels.as_deref(),
        checklist: patch.checklist.as_deref(),
        estimate: patch.estimate,
    }
}

#[async_trait]
impl WorkspaceRepo for PgKanban {
    async fn list(&self) -> Result<Vec<WorkspaceRow>, StoreError> {
        self.store().list_workspaces().await
    }

    async fn create(&self, ws: NewWorkspace) -> Result<WorkspaceRow, StoreError> {
        self.store().create_workspace(&ws.name).await
    }

    async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.store().delete_workspace(id).await
    }
}

#[async_trait]
impl ProjectRepo for PgKanban {
    async fn list(&self, workspace_id: i64) -> Result<Vec<ProjectRow>, StoreError> {
        self.store().list_projects(workspace_id).await
    }

    async fn create(&self, p: NewProject) -> Result<ProjectRow, StoreError> {
        self.store().create_project(p.workspace_id, &p.name).await
    }

    async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.store().delete_project(id).await
    }
}

#[async_trait]
impl CardRepo for PgKanban {
    async fn list(&self, project_id: Option<i64>) -> Result<Vec<CardRow>, StoreError> {
        self.store().list(project_id).await
    }

    async fn get(&self, id: i64) -> Result<Option<CardRow>, StoreError> {
        self.store().get(id).await
    }

    async fn move_card(&self, mv: CardMove) -> Result<(), StoreError> {
        self.store().move_card(as_move(&mv)).await
    }

    async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.store().remove(id).await
    }

    async fn set_agent(&self, id: i64, agent: &AgentState) -> Result<(), StoreError> {
        self.store().set_agent(id, agent).await
    }

    async fn agent(&self, id: i64) -> Result<Option<AgentState>, StoreError> {
        self.store().agent(id).await
    }

    async fn set_pipeline(&self, card_id: i64, pipeline_id: Option<i64>) -> Result<(), StoreError> {
        self.store().set_card_pipeline(card_id, pipeline_id).await
    }

    async fn set_cron(&self, card_id: i64, cron: Option<String>) -> Result<(), StoreError> {
        self.store().set_cron(card_id, cron.as_deref()).await
    }

    async fn tx(&self) -> Result<Box<dyn CardTx>, StoreError> {
        Ok(Box::new(PgCardTx {
            store: Arc::clone(&self.store),
            tx: self.store().begin().await?,
        }))
    }
}

struct PgCardTx {
    store: Arc<Store>,
    tx: DbTx,
}

#[async_trait]
impl CardTx for PgCardTx {
    async fn add(&mut self, card: NewCard) -> Result<i64, StoreError> {
        self.store.add_tx(&mut self.tx, as_add(&card)).await
    }

    async fn get(&mut self, id: i64) -> Result<Option<CardRow>, StoreError> {
        self.store.get_tx(&mut self.tx, id).await
    }

    async fn update(&mut self, patch: CardPatch) -> Result<(), StoreError> {
        self.store
            .update_card_tx(&mut self.tx, as_patch(&patch))
            .await
    }

    async fn commit(self: Box<Self>) -> Result<(), StoreError> {
        let PgCardTx { tx, .. } = *self;
        commit_tx(tx).await
    }

    async fn rollback(self: Box<Self>) -> Result<(), StoreError> {
        let PgCardTx { tx, .. } = *self;
        rollback_tx(tx).await
    }
}

#[async_trait]
impl CommentRepo for PgKanban {
    async fn list(&self, card_id: i64) -> Result<Vec<CommentRow>, StoreError> {
        self.store().list_comments(card_id).await
    }

    async fn tx(&self) -> Result<Box<dyn CommentTx>, StoreError> {
        Ok(Box::new(PgCommentTx {
            store: Arc::clone(&self.store),
            tx: self.store().begin().await?,
        }))
    }
}

struct PgCommentTx {
    store: Arc<Store>,
    tx: DbTx,
}

#[async_trait]
impl CommentTx for PgCommentTx {
    async fn card_exists(&mut self, card_id: i64) -> Result<bool, StoreError> {
        self.store.card_exists_tx(&mut self.tx, card_id).await
    }

    async fn add(
        &mut self,
        card_id: i64,
        author: &str,
        body: &str,
    ) -> Result<CommentRow, StoreError> {
        self.store
            .add_comment_tx(&mut self.tx, card_id, author, body)
            .await
    }

    async fn commit(self: Box<Self>) -> Result<(), StoreError> {
        commit_tx(self.tx).await
    }

    async fn rollback(self: Box<Self>) -> Result<(), StoreError> {
        rollback_tx(self.tx).await
    }
}

#[async_trait]
impl PipelineRepo for PgKanban {
    async fn list(&self) -> Result<Vec<PipelineRow>, StoreError> {
        self.store().list_pipelines().await
    }

    async fn update(&self, id: i64, p: NewPipeline) -> Result<(), StoreError> {
        self.store().update_pipeline(id, &p.name, &p.spec).await
    }

    async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.store().remove_pipeline(id).await
    }

    async fn tx(&self) -> Result<Box<dyn PipelineTx>, StoreError> {
        Ok(Box::new(PgPipelineTx {
            store: Arc::clone(&self.store),
            tx: self.store().begin().await?,
        }))
    }
}

struct PgPipelineTx {
    store: Arc<Store>,
    tx: DbTx,
}

#[async_trait]
impl PipelineTx for PgPipelineTx {
    async fn create(&mut self, p: NewPipeline) -> Result<i64, StoreError> {
        self.store
            .create_pipeline_tx(&mut self.tx, &p.name, &p.spec)
            .await
    }

    async fn get(&mut self, id: i64) -> Result<Option<PipelineRow>, StoreError> {
        self.store.get_pipeline_tx(&mut self.tx, id).await
    }

    async fn commit(self: Box<Self>) -> Result<(), StoreError> {
        commit_tx(self.tx).await
    }

    async fn rollback(self: Box<Self>) -> Result<(), StoreError> {
        rollback_tx(self.tx).await
    }
}

#[async_trait]
impl ResourceRepo for PgKanban {
    async fn upsert<'a>(&self, res: UpsertResource<'a>) -> Result<(), StoreError> {
        self.store().upsert_resource(res).await
    }

    async fn list(&self, card_id: i64) -> Result<Vec<ResourceRow>, StoreError> {
        self.store().list_resources(card_id).await
    }
}

#[async_trait]
impl AgentConfigRepo for PgKanban {
    async fn list(&self) -> Result<Vec<AgentConfigRow>, StoreError> {
        self.store().list_agents().await
    }

    async fn by_name(&self, name: &str) -> Result<Option<AgentConfigRow>, StoreError> {
        self.store().agent_by_name(name).await
    }

    async fn create(&self, cfg: AgentConfigDraft) -> Result<AgentConfigRow, StoreError> {
        let row = AgentConfigRow {
            id: 0,
            name: cfg.name,
            model: cfg.model,
            persona: cfg.persona,
            prompt: cfg.prompt,
            output: cfg.output,
            allowed_tools: cfg.allowed_tools,
        };
        let id = self.store().create_agent(&row).await?;
        Ok(AgentConfigRow { id, ..row })
    }

    async fn update(&self, id: i64, cfg: AgentConfigDraft) -> Result<(), StoreError> {
        let upd = AgentConfigUpdate {
            id,
            name: Box::leak(cfg.name.into_boxed_str()),
            model: Box::leak(cfg.model.into_boxed_str()),
            persona: Box::leak(cfg.persona.into_boxed_str()),
            prompt: Box::leak(cfg.prompt.into_boxed_str()),
            output: Box::leak(cfg.output.into_boxed_str()),
            allowed_tools: Box::leak(cfg.allowed_tools.into_boxed_slice()),
        };
        self.store().update_agent(upd).await
    }

    async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.store().remove_agent(id).await
    }
}

#[async_trait]
impl SkillRepo for PgKanban {
    async fn list(&self) -> Result<Vec<SkillRow>, StoreError> {
        self.store().list_skills().await
    }

    async fn create(&self, name: &str, body: &str) -> Result<SkillRow, StoreError> {
        self.store().create_skill(NewSkill { name, body }).await
    }

    async fn update(&self, id: i64, body: &str) -> Result<(), StoreError> {
        self.store().update_skill(id, body).await
    }

    async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.store().remove_skill(id).await
    }

    async fn list_for_agent(&self, agent_id: i64) -> Result<Vec<SkillRow>, StoreError> {
        self.store().list_agent_skills(agent_id).await
    }

    async fn attach(&self, agent_id: i64, skill_id: i64) -> Result<(), StoreError> {
        self.store().attach_agent_skill(agent_id, skill_id).await
    }

    async fn detach(&self, agent_id: i64, skill_id: i64) -> Result<(), StoreError> {
        self.store().detach_agent_skill(agent_id, skill_id).await
    }
}

async fn commit_tx(tx: DbTx) -> Result<(), StoreError> {
    tx.commit().await.map_err(Into::into)
}

async fn rollback_tx(tx: DbTx) -> Result<(), StoreError> {
    tx.rollback().await.map_err(Into::into)
}
