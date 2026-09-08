use std::net::SocketAddr;

use proto_rs::client;
use proto_rs::envelope::{Envelope, Kind};
use proto_rs::server::{ClientConn, Registry};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

#[tokio::test]
async fn command_round_trip_and_heartbeat() {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    let registry = std::sync::Arc::new(Registry::new());
    let (tx, mut connected) = mpsc::channel::<ClientConn>(1);
    tokio::spawn(proto_rs::server::serve(
        listener,
        tx,
        std::sync::Arc::clone(&registry),
    ));

    let client_task = tokio::spawn({
        let addr = addr.clone();
        async move {
            client::connect(
                &addr,
                "test-host".to_string(),
                "test-os".to_string(),
                |cmd| format!("echo:{cmd}"),
            )
            .await
        }
    });

    let mut conn = connected.recv().await.expect("client connected");
    assert_eq!(conn.id, 1);
    assert!(conn.peer.to_string().starts_with("127.0.0.1"));

    registry
        .send(
            conn.id,
            Envelope {
                id: 42,
                kind: Kind::Command {
                    cmd: "ping".to_string(),
                },
            },
        )
        .await
        .expect("dispatch");

    let result = conn.rx.recv().await.expect("result envelope");
    assert_eq!(result.id, 42);
    match result.kind {
        Kind::Result { output } => assert_eq!(output, "echo:ping"),
        other => panic!("expected Result, got {other:?}"),
    }

    registry
        .send(
            conn.id,
            Envelope {
                id: 43,
                kind: Kind::Heartbeat,
            },
        )
        .await
        .expect("dispatch heartbeat");

    let pong = conn.rx.recv().await.expect("heartbeat envelope");
    assert_eq!(pong.id, 43);
    assert!(matches!(pong.kind, Kind::Heartbeat));

    assert_eq!(registry.ids(), vec![conn.id]);
    client_task.abort();
    let _ = &mut conn;
}
