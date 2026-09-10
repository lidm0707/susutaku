use std::net::SocketAddr;
use std::sync::Arc;

use local_model::api::{AppState, router};
use local_model::engine::ModelPool;
use local_model::hub::{self, Hub};
use local_model::{HTTP_PORT, HTTP_PORT_ENV, TCP_PORT, TCP_PORT_ENV};

fn port_from_env(key: &str, default: u16) -> u16 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[tokio::main]
async fn main() {
    // The model engine is optional: the hub and its HTTP API work without
    // one (inference requests fail per-job) — e.g. a hub-only container.
    let pool = match ModelPool::spawn_first() {
        Ok(pool) => pool,
        Err(e) => {
            println!("model init failed ({e}); starting hub-only");
            ModelPool::empty()
        }
    };
    let hub = Hub::new();
    let tcp_port = port_from_env(TCP_PORT_ENV, TCP_PORT);
    let hub_listener = hub::bind(SocketAddr::from(([0, 0, 0, 0], tcp_port)))
        .await
        .expect("bind tcp hub");
    let hub_addr = hub_listener.local_addr().expect("hub addr");
    println!("local-model hub listening on tcp://{hub_addr}");
    tokio::spawn(Arc::clone(&hub).run(hub_listener));

    let state = Arc::new(AppState {
        pool: Arc::new(pool.clone()),
        inference: Arc::new(pool),
        hub,
    });
    let http_port = port_from_env(HTTP_PORT_ENV, HTTP_PORT);
    let addr = SocketAddr::from(([0, 0, 0, 0], http_port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind http");
    println!("local-model listening on http://{addr}");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("serve");
}
