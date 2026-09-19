use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use backend::api;
use backend::app::ChatUseCase;
use backend::app::board::BoardService;
use backend::infra::client::node::ClientNode;
use backend::infra::codex::auth::codex_home;
use backend::infra::manager_git::ManagerGit;
use backend::infra::model_client::RemoteModel;
use backend::infra::podman::AgentSandbox;
use backend::infra::postgres::{codex_usage, connect};
use backend::infra::qdant::chat_memory;
use backend::infra::search::{DuckDuckGo, PageFetcher};
use backend::infra::settings::local;
use backend::port::outbound::ChatMemory;
use manager_rs::manager::Manager;
use tracing_subscriber::EnvFilter;

const PORT: u16 = 8991;
const DEFAULT_LOG_LEVEL: &str = "info";
const SERVER_URL: &str = "http://127.0.0.1:8992";
const SERVER_URL_ENV: &str = "SUSUTAKU_LOCAL_MODEL_URL";
const HUB_ADDR_ENV: &str = "SUSUTAKU_HUB_ADDR";
const HEALTH_INTERVAL: Duration = Duration::from_secs(5 * 60);

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn local_model_url() -> String {
    let saved = local::read_saved()
        .map(|s| s.endpoint)
        .filter(|e| !e.is_empty());
    saved.unwrap_or_else(|| env_or(SERVER_URL_ENV, SERVER_URL))
}

fn chat_memory() -> Option<Arc<dyn ChatMemory>> {
    chat_memory::from_env().map(|m| Arc::new(m) as Arc<dyn ChatMemory>)
}

async fn spawn_client_node(sandbox: Arc<AgentSandbox>) {
    let Ok(hub_addr) = std::env::var(HUB_ADDR_ENV) else {
        return;
    };
    let node = ClientNode::new(&hub_addr, sandbox);
    tokio::spawn(async move {
        if let Err(err) = node.run().await {
            tracing::error!("client node stopped: {err}");
        }
    });
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_LEVEL));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn spawn_health_log(manager: Arc<Manager>) {
    tokio::spawn(async move {
        let start = tokio::time::Instant::now() + HEALTH_INTERVAL;
        let mut tick = tokio::time::interval_at(start, HEALTH_INTERVAL);
        loop {
            tick.tick().await;
            let agents = manager.snapshot();
            if agents.is_empty() {
                tracing::info!("health: no agents");
            }
            for a in agents {
                tracing::info!(
                    "health: agent {} runs {} work_tree {}",
                    a.agent,
                    a.runs,
                    a.work_tree.display()
                );
            }
        }
    });
}

#[tokio::main]
async fn main() {
    init_tracing();
    let task_store = Arc::new(connect().await);
    let model = Arc::new(RemoteModel::new(&local_model_url()));
    let sandbox = Arc::new(AgentSandbox::restore().expect("agent sandbox init"));
    spawn_client_node(sandbox.clone()).await;
    let codex_workspace = sandbox.root();
    let manager = Manager::new();
    let board = Arc::new(BoardService::new(
        task_store.clone(),
        Some(model.clone()),
        Some(Arc::new(backend::infra::zai::router::ZaiRouter::new(
            Arc::new(backend::infra::zai::settings::SettingsState::load()),
        ))),
        Some(Arc::new(backend::app::card_run::WorkTree::new(
            manager.clone(),
        ))),
    ));
    api::seed_project_skill(&task_store).await;
    let skills = Arc::new(backend::domain::SkillService::new(Arc::new(
        backend::infra::postgres::PgTask::new(task_store.clone()),
    )));
    let agents: Arc<dyn backend::port::outbound::AgentConfigRepo> =
        Arc::new(backend::infra::postgres::PgTask::new(task_store.clone()));
    let resources = std::sync::Arc::new(backend::domain::ResourceService::new(Arc::new(
        backend::infra::postgres::PgTask::new(task_store.clone()),
    )));
    let use_case = Arc::new(
        ChatUseCase::new(
            Arc::new(DuckDuckGo),
            Arc::new(PageFetcher),
            sandbox.clone(),
            model.clone(),
            model.clone(),
            chat_memory(),
            board,
            agents,
        )
        .with_resources(resources)
        .with_agent_git(Arc::new(ManagerGit::new(manager.clone())))
        .with_agent_run(Arc::new(backend::infra::manager_run::ManagerRun::new(
            manager.clone(),
        )))
        .with_thread_envs(Arc::new(backend::infra::thread_env::ThreadEnvManager::new()))
        .with_project_git(Arc::new(
            backend::infra::project_git::SettingsProjectGit::new(task_store.clone()),
        ))
        .with_skills(skills)
        .with_engines(Arc::new(backend::infra::zai::router::ZaiRouter::new(
            Arc::new(backend::infra::zai::settings::SettingsState::load()),
        ))),
    );

    let usage_store = Arc::new(codex_usage::connect().await);
    codex_usage::spawn_scheduler(usage_store.clone(), codex_home());
    spawn_health_log(manager.clone());
    let addr = SocketAddr::from(([0, 0, 0, 0], PORT));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    tracing::info!("backend listening on http://{addr}");
    axum::serve(
        listener,
        api::router(
            use_case,
            model.clone(),
            codex_workspace,
            sandbox.clone(),
            task_store,
            usage_store,
            manager,
            model.clone(),
        ),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
    .expect("serve");
}
