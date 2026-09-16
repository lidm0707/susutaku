//! Task application layer: composition root. Wires repo ports into the
//! 1:1 domain services — not SQL.

use std::sync::Arc;

use crate::domain::{
    AgentConfigService, CardService, CommentService, ProjectService, ResourceService, SkillService,
    WorkspaceService,
};
use crate::port::outbound::{
    AgentConfigRepo, CardRepo, CommentRepo, ProjectRepo, ResourceRepo, SkillRepo, WorkspaceRepo,
};

pub struct TaskApp {
    pub cards: CardService,
    pub comments: CommentService,
    pub resources: ResourceService,
    pub agents: AgentConfigService,
    pub skills: SkillService,
    pub workspaces: WorkspaceService,
    pub projects: ProjectService,
    /// Shared Postgres store: routine CRUD/runs live outside the 1:1 ports.
    pub store: Arc<task_rs::Store>,
}

impl TaskApp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        store: Arc<task_rs::Store>,
        cards: Arc<dyn CardRepo>,
        comments: Arc<dyn CommentRepo>,
        resources: Arc<dyn ResourceRepo>,
        agents: Arc<dyn AgentConfigRepo>,
        skills: Arc<dyn SkillRepo>,
        workspaces: Arc<dyn WorkspaceRepo>,
        projects: Arc<dyn ProjectRepo>,
    ) -> Self {
        Self {
            cards: CardService::new(cards),
            comments: CommentService::new(comments),
            resources: ResourceService::new(resources),
            agents: AgentConfigService::new(agents),
            skills: SkillService::new(skills),
            workspaces: WorkspaceService::new(workspaces),
            projects: ProjectService::new(projects),
            store,
        }
    }
}

/// Composition root: wires the Postgres adapters into the app services.
pub fn build(store: Arc<task_rs::Store>) -> TaskApp {
    use crate::infra::postgres::task::PgTask;
    let pg = Arc::new(PgTask::new(Arc::clone(&store)));
    TaskApp::new(
        Arc::clone(&store),
        Arc::clone(&pg) as Arc<dyn CardRepo>,
        Arc::clone(&pg) as Arc<dyn CommentRepo>,
        Arc::clone(&pg) as Arc<dyn ResourceRepo>,
        Arc::clone(&pg) as Arc<dyn AgentConfigRepo>,
        Arc::clone(&pg) as Arc<dyn SkillRepo>,
        Arc::clone(&pg) as Arc<dyn WorkspaceRepo>,
        pg as Arc<dyn ProjectRepo>,
    )
}
