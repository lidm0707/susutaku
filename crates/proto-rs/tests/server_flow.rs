use proto_rs::codec::write_frame;
use proto_rs::envelope::{Envelope, Kind};
use proto_rs::server::{self, ClientConn, Registry};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

async fn bind_loopback() -> (TcpListener, String) {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    (listener, addr)
}

async fn spawn_serve(listener: TcpListener, registry: Arc<Registry>) -> mpsc::Receiver<ClientConn> {
    let (tx, rx) = mpsc::channel::<ClientConn>(1);
    tokio::spawn(server::serve(listener, tx, registry));
    rx
}

#[tokio::test]
async fn registry_default_is_empty() {
    let registry = Registry::default();
    assert!(registry.ids().is_empty());
}

#[tokio::test]
async fn outbound_write_error_breaks_writer_and_frees_registry() {
    let (listener, addr) = bind_loopback().await;
    let registry = Arc::new(Registry::new());
    let mut connected = spawn_serve(listener, Arc::clone(&registry)).await;

    let stream = TcpStream::connect(addr).await.expect("connect");
    let conn = connected.recv().await.expect("conn");

    for i in 0..16 {
        registry
            .send(
                conn.id,
                Envelope {
                    id: i,
                    kind: Kind::Command {
                        cmd: "x".repeat(1024 * 1024),
                    },
                },
            )
            .await
            .expect("queue outbound");
    }
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    drop(stream);
    let mut err = None;
    for _ in 0..100 {
        match registry
            .send(
                conn.id,
                Envelope {
                    id: 99,
                    kind: Kind::Heartbeat,
                },
            )
            .await
        {
            Ok(()) => continue,
            Err(e) => {
                err = Some(e);
                break;
            }
        }
    }
    let err = err.expect("client gone");
    assert!(err.contains("no client") || err.contains("stopped"));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(registry.ids().is_empty());
}

#[tokio::test]
async fn send_to_unknown_client_fails() {
    let registry = Registry::new();
    let err = registry
        .send(
            999,
            Envelope {
                id: 1,
                kind: Kind::Heartbeat,
            },
        )
        .await
        .expect_err("unknown");
    assert!(err.contains("no client 999"));
}

#[tokio::test]
async fn send_fails_after_client_writer_stops() {
    let (listener, addr) = bind_loopback().await;
    let registry = Arc::new(Registry::new());
    let mut connected = spawn_serve(listener, Arc::clone(&registry)).await;

    let stream = TcpStream::connect(addr.clone()).await.expect("connect");
    let conn = connected.recv().await.expect("conn");

    let ok = registry
        .send(
            conn.id,
            Envelope {
                id: 2,
                kind: Kind::Heartbeat,
            },
        )
        .await;
    assert!(ok.is_ok(), "first send buffered");
    drop(stream);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let err = registry
        .send(
            conn.id,
            Envelope {
                id: 3,
                kind: Kind::Heartbeat,
            },
        )
        .await
        .expect_err("stopped");
    assert!(err.contains("stopped") || err.contains("no client"));
    assert!(registry.ids().is_empty());
}

#[tokio::test]
async fn serve_continues_when_connected_channel_dropped() {
    let (listener, addr) = bind_loopback().await;
    let registry = Arc::new(Registry::new());
    spawn_serve(listener, Arc::clone(&registry)).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let _ = stream.write_all(&[0u8; 4]).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(registry.ids().is_empty());
}

#[tokio::test]
async fn inbound_envelope_reaches_conn_rx_and_register_is_skipped() {
    let (listener, addr) = bind_loopback().await;
    let registry = Arc::new(Registry::new());
    let mut connected = spawn_serve(listener, Arc::clone(&registry)).await;

    let mut stream = TcpStream::connect(addr.clone()).await.expect("connect");
    let mut conn = connected.recv().await.expect("conn");
    assert_eq!(registry.ids(), vec![conn.id]);

    write_frame(
        &mut stream,
        &Envelope {
            id: 1,
            kind: Kind::Register {
                hostname: "h".into(),
                os: "o".into(),
            },
        },
    )
    .await
    .expect("register");
    write_frame(
        &mut stream,
        &Envelope {
            id: 2,
            kind: Kind::Command { cmd: "hi".into() },
        },
    )
    .await
    .expect("command");

    let env = conn.rx.recv().await.expect("inbound");
    assert_eq!(env.id, 2);
    assert!(matches!(env.kind, Kind::Command { ref cmd } if cmd == "hi"));

    drop(conn.rx);
    write_frame(
        &mut stream,
        &Envelope {
            id: 3,
            kind: Kind::Heartbeat,
        },
    )
    .await
    .expect("heartbeat after rx dropped");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

#[tokio::test]
async fn client_outbound_write_error_stops_writer() {
    let (listener, addr) = bind_loopback().await;
    let registry = Arc::new(Registry::new());
    let mut connected = spawn_serve(listener, Arc::clone(&registry)).await;

    let stream = TcpStream::connect(addr).await.expect("connect");
    let conn = connected.recv().await.expect("conn");

    drop(stream);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let err = registry
        .send(
            conn.id,
            Envelope {
                id: 4,
                kind: Kind::Heartbeat,
            },
        )
        .await
        .expect_err("client gone");
    assert!(err.contains("no client") || err.contains("stopped"));
    assert!(registry.ids().is_empty());
}
