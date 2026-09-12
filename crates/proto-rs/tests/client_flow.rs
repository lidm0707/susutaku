use proto_rs::ClientMeta;
use proto_rs::client;
use proto_rs::codec::{read_frame, write_frame};
use proto_rs::envelope::{Envelope, Kind};
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
type ClientHandle = tokio::task::JoinHandle<Result<(), String>>;

fn test_meta() -> ClientMeta {
    ClientMeta {
        hostname: "h".into(),
        os: "o".into(),
        arch: "aarch64".into(),
        role: "worker".into(),
        ram_gib: 16,
    }
}

async fn bind_loopback() -> (TcpListener, String) {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    (listener, addr)
}

async fn read_register(stream: &mut TcpStream) -> Envelope {
    let env = read_frame(stream).await.expect("register frame");
    assert!(matches!(env.kind, Kind::Register { .. }));
    assert_eq!(env.id, proto_rs::client::FIRST_ENVELOPE_ID);
    env
}

fn spawn_client_raw(
    addr: String,
    hostname: String,
    os: String,
    on_command: impl FnMut(&str, &str) -> String + Send + 'static,
) -> ClientHandle {
    tokio::spawn(async move {
        let mut meta = test_meta();
        meta.hostname = hostname;
        meta.os = os;
        client::connect(&addr, meta, on_command, || Vec::new()).await
    })
}

fn spawn_client(
    addr: String,
    on_command: impl FnMut(&str, &str) -> String + Send + 'static,
) -> ClientHandle {
    spawn_client_agents(addr, on_command, || Vec::new())
}

fn spawn_client_agents(
    addr: String,
    on_command: impl FnMut(&str, &str) -> String + Send + 'static,
    on_agents: impl FnMut() -> Vec<proto_rs::AgentBrief> + Send + 'static,
) -> ClientHandle {
    tokio::spawn(async move { client::connect(&addr, test_meta(), on_command, on_agents).await })
}

async fn connect_fails_when_refused() {
    let (listener, addr) = bind_loopback().await;
    drop(listener);
    let err = client::connect(&addr, test_meta(), |_agent, c| c.to_string(), || Vec::new())
        .await
        .expect_err("refused");
    assert!(err.contains("connect to"));
}

#[tokio::test]
async fn client_rejects_register_from_server() {
    let (listener, addr) = bind_loopback().await;
    let task = spawn_client(addr, |_agent, c| c.to_string());
    let (mut stream, _) = listener.accept().await.expect("accept");
    read_register(&mut stream).await;
    write_frame(
        &mut stream,
        &Envelope {
            id: 9,
            kind: Kind::Register {
                hostname: "srv".into(),
                os: "x".into(),
                arch: "aarch64".into(),
                role: "worker".into(),
                ram_gib: 16,
            },
        },
    )
    .await
    .expect("write");
    let err = task.await.expect("task").expect_err("register rejected");
    assert!(err.contains("unexpected Register"));
}

#[tokio::test]
async fn client_rejects_result_from_server() {
    let (listener, addr) = bind_loopback().await;
    let task = spawn_client(addr, |_agent, c| c.to_string());
    let (mut stream, _) = listener.accept().await.expect("accept");
    read_register(&mut stream).await;
    write_frame(
        &mut stream,
        &Envelope {
            id: 9,
            kind: Kind::Result { output: "x".into() },
        },
    )
    .await
    .expect("write");
    let err = task.await.expect("task").expect_err("result rejected");
    assert!(err.contains("unexpected Result"));
}

const BIG: usize = 8 * 1024 * 1024;

#[tokio::test]
async fn register_write_fails_when_server_never_reads() {
    let (listener, addr) = bind_loopback().await;
    let task = spawn_client_raw(addr, "h".repeat(BIG), "o".repeat(BIG), |_agent, c| {
        c.to_string()
    });
    let (stream, _) = listener.accept().await.expect("accept");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    drop(stream);
    let err = task.await.expect("task").expect_err("write failed");
    assert!(err.contains("write frame"));
}

#[tokio::test]
async fn reply_write_fails_when_server_drops_without_reading() {
    let (listener, addr) = bind_loopback().await;
    let task = spawn_client_raw(addr, "h".into(), "o".into(), |_a, _| "y".repeat(BIG));
    let (mut stream, _) = listener.accept().await.expect("accept");
    read_register(&mut stream).await;
    write_frame(
        &mut stream,
        &Envelope {
            id: 7,
            kind: Kind::Command {
                agent: String::new(),
                cmd: "go".into(),
            },
        },
    )
    .await
    .expect("command");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    drop(stream);
    let err = task.await.expect("task").expect_err("reply write failed");
    assert!(err.contains("write frame"));
}

#[tokio::test]
async fn client_fails_when_server_drops() {
    let (listener, addr) = bind_loopback().await;
    let task = spawn_client(addr, |_agent, c| c.to_string());
    let (mut stream, _) = listener.accept().await.expect("accept");
    read_register(&mut stream).await;
    drop(stream);
    let err = task.await.expect("task").expect_err("closed");
    assert!(err.contains("read frame"));
}

#[tokio::test]
async fn client_answers_agent_names() {
    let (listener, addr) = bind_loopback().await;
    spawn_client_agents(
        addr,
        |_agent, c| format!("out:{c}"),
        || {
            vec![
                proto_rs::AgentBrief {
                    name: "fix-login".into(),
                    runs: 2,
                    last_cmd: Some("echo hi".into()),
                },
                proto_rs::AgentBrief {
                    name: "scan".into(),
                    runs: 0,
                    last_cmd: None,
                },
            ]
        },
    );
    let (mut stream, _) = listener.accept().await.expect("accept");
    read_register(&mut stream).await;

    write_frame(
        &mut stream,
        &Envelope {
            id: 8,
            kind: Kind::AgentNames,
        },
    )
    .await
    .expect("agent names query");
    let reply = read_frame(&mut stream).await.expect("reply");
    assert_eq!(reply.id, 8);
    match reply.kind {
        Kind::AgentNamesResult { agents } => {
            assert_eq!(agents.len(), 2);
            assert_eq!(agents[0].name, "fix-login");
            assert_eq!(agents[0].runs, 2);
            assert_eq!(agents[0].last_cmd.as_deref(), Some("echo hi"));
            assert_eq!(agents[1].last_cmd, None);
        }
        other => panic!("expected AgentNamesResult, got {other:?}"),
    }
}

#[tokio::test]
async fn client_answers_command_and_heartbeat() {
    let (listener, addr) = bind_loopback().await;
    spawn_client(addr, |_agent, c| format!("out:{c}"));
    let (mut stream, _) = listener.accept().await.expect("accept");
    read_register(&mut stream).await;

    write_frame(
        &mut stream,
        &Envelope {
            id: 5,
            kind: Kind::Command {
                agent: String::new(),
                cmd: "ls".into(),
            },
        },
    )
    .await
    .expect("command");
    let reply = read_frame(&mut stream).await.expect("reply");
    assert_eq!(reply.id, 5);
    match reply.kind {
        Kind::Result { output } => assert_eq!(output, "out:ls"),
        other => panic!("expected Result, got {other:?}"),
    }

    write_frame(
        &mut stream,
        &Envelope {
            id: 6,
            kind: Kind::Heartbeat,
        },
    )
    .await
    .expect("heartbeat");
    let pong = read_frame(&mut stream).await.expect("pong");
    assert_eq!(pong.id, 6);
    assert!(matches!(pong.kind, Kind::Heartbeat));
}
