use std::net::SocketAddr;
use std::sync::Arc;

use backend::api;
use backend::app::ChatUseCase;
use backend::infra::client_node::ClientNode;
use backend::infra::kanban;
use backend::infra::model_client::RemoteModel;
use backend::infra::sandbox::AgentSandbox;
use backend::infra::search::{DuckDuckGo, PageFetcher};

const PORT: u16 = 8991;
const SERVER_URL: &str = "http://127.0.0.1:8992";
const HUB_ADDR: &str = "127.0.0.1:8993";
const SERVER_URL_ENV: &str = "SUSUTAKU_MODEL_SERVER_URL";
const HUB_ADDR_ENV: &str = "SUSUTAKU_HUB_ADDR";

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() {
    let model = Arc::new(RemoteModel::new(&env_or(SERVER_URL_ENV, SERVER_URL)));
    let sandbox = Arc::new(AgentSandbox::restore().expect("agent sandbox init"));
    let node = ClientNode::new(&env_or(HUB_ADDR_ENV, HUB_ADDR), sandbox.clone());
    tokio::spawn(async move {
        if let Err(err) = node.run().await {
            eprintln!("client node stopped: {err}");
        }
    });
    let codex_workspace = sandbox.root();
    let use_case = Arc::new(ChatUseCase::new(
        Arc::new(DuckDuckGo),
        Arc::new(PageFetcher),
        sandbox,
        model.clone(),
        model.clone(),
    ));

    let kanban_store = Arc::new(kanban::connect().await);
    let addr = SocketAddr::from(([0, 0, 0, 0], PORT));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    println!("backend listening on http://{addr}");
    axum::serve(
        listener,
        api::router(use_case, model.clone(), codex_workspace, kanban_store),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
    .expect("serve");
}
