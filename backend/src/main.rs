mod api;
mod app;
mod domain;
mod infra;
mod port;

use std::net::SocketAddr;
use std::sync::Arc;

use app::ChatUseCase;
use infra::engine::ModelPool;
use infra::search::{DuckDuckGo, PageFetcher};

const PORT: u16 = 8991;

#[tokio::main]
async fn main() {
    let engine = ModelPool::spawn_first().expect("model init");
    let use_case = Arc::new(ChatUseCase::new(
        Arc::new(DuckDuckGo),
        Arc::new(PageFetcher),
        Arc::new(engine.clone()),
        Arc::new(engine),
    ));

    let addr = SocketAddr::from(([0, 0, 0, 0], PORT));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    println!("backend listening on http://{addr}");
    axum::serve(listener, api::router(use_case))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("serve");
}
