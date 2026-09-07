use std::net::SocketAddr;
use std::sync::Arc;

use backend::api;
use backend::app::ChatUseCase;
use backend::infra::engine::ModelPool;
use backend::infra::kanban;
use backend::infra::sandbox::AgentSandbox;
use backend::infra::search::{DuckDuckGo, PageFetcher};

const PORT: u16 = 8991;

#[tokio::main]
async fn main() {
    let engine = ModelPool::spawn_first().expect("model init");
    let sandbox = Arc::new(AgentSandbox::restore().expect("agent sandbox init"));
    let codex_workspace = sandbox.root();
    let use_case = Arc::new(ChatUseCase::new(
        Arc::new(DuckDuckGo),
        Arc::new(PageFetcher),
        sandbox,
        Arc::new(engine.clone()),
        Arc::new(engine),
    ));

    let kanban_store = Arc::new(kanban::connect().await);
    let addr = SocketAddr::from(([0, 0, 0, 0], PORT));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    println!("backend listening on http://{addr}");
    axum::serve(listener, api::router(use_case, codex_workspace, kanban_store))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("serve");
}
