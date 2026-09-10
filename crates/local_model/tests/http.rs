use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::StatusCode;
use local_model::api::{AppState, InferenceRequest, router};
use local_model::hub::Hub;
use local_model::ports::{Inference, ModelSwitch, ReplyRx};
use proto_rs::client;
use susutaku_mlx::tok::TokKind;
use tokio::net::TcpListener;

const ONE_CLIENT: usize = 1;

struct FailingEngine;

impl Inference for FailingEngine {
    fn submit(
        &self,
        _prompt: String,
        _max_tokens: usize,
        _tok: TokKind,
        _think: bool,
    ) -> Result<ReplyRx, String> {
        Err("no model loaded".to_string())
    }
}

struct NoModel;

impl ModelSwitch for NoModel {
    fn select(&self, _name: &str) -> Result<(), String> {
        Err("no models".to_string())
    }

    fn selected(&self) -> Option<String> {
        None
    }
}

fn test_state(hub: Arc<Hub>) -> Arc<AppState> {
    Arc::new(AppState {
        pool: Arc::new(NoModel),
        inference: Arc::new(FailingEngine),
        hub,
    })
}

#[tokio::test]
async fn inference_error_path_returns_500() {
    let hub = Hub::new();
    let app = router(test_state(hub));
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(axum::serve(listener, app).into_future());
    let req = InferenceRequest {
        prompt: "hi".to_string(),
        max_tokens: None,
        tok: None,
        think: None,
    };
    let (status, body) = request(addr, "POST", "/api/inference", &req).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body.contains("no model loaded"), "body: {body}");
}

#[tokio::test]
async fn hub_registration_and_dispatch() {
    let hub = Hub::new();
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind");
    let tcp_addr = listener.local_addr().expect("addr");
    tokio::spawn(Arc::clone(&hub).run(listener));

    let tcp_str = tcp_addr.to_string();
    let client_task = tokio::spawn(async move {
        client::connect(
            &tcp_str,
            "worker-1".to_string(),
            "macos".to_string(),
            |cmd| format!("ran:{cmd}"),
        )
        .await
    });

    let deadline = std::time::Duration::from_secs(2);
    let waited = std::time::Instant::now();
    while hub.client_ids().len() != ONE_CLIENT {
        assert!(waited.elapsed() < deadline, "client never registered");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let app = router(test_state(Arc::clone(&hub)));
    let http_listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind");
    let http_addr = http_listener.local_addr().expect("addr");
    tokio::spawn(axum::serve(http_listener, app).into_future());

    let (status, body) = request(http_addr, "GET", "/api/clients", &()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("1"), "clients body: {body}");

    let (status, body) = request(
        http_addr,
        "POST",
        "/api/clients/1/command",
        &serde_json::json!({ "cmd": "ls" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("ran:ls"), "command body: {body}");

    client_task.abort();
}

async fn request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    body: &impl serde::ser::Serialize,
) -> (StatusCode, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let payload = serde_json::to_vec(body).expect("json");
    let http = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream.write_all(http.as_bytes()).await.expect("write head");
    stream.write_all(&payload).await.expect("write body");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read");
    let text = String::from_utf8_lossy(&buf).to_string();
    let status_line = text.lines().next().unwrap_or_default().to_string();
    let status = if status_line.contains("200") {
        StatusCode::OK
    } else if status_line.contains("500") {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::BAD_REQUEST
    };
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    (status, body)
}
