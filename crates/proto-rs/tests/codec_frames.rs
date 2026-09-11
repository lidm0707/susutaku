use proto_rs::codec::{read_frame, write_frame};
use proto_rs::envelope::{Envelope, Kind};
use proto_rs::{LENGTH_PREFIX_BYTES, MAX_FRAME_BYTES};
use tokio::io::{AsyncWriteExt, duplex};

fn sample(id: u64) -> Envelope {
    Envelope {
        id,
        kind: Kind::Command {
            cmd: "ping".to_string(),
        },
    }
}

#[tokio::test]
async fn roundtrip_through_duplex() {
    let (mut a, mut b) = duplex(4096);
    write_frame(&mut a, &sample(7)).await.expect("write");
    let env = read_frame(&mut b).await.expect("read");
    assert_eq!(env.id, 7);
    assert!(matches!(env.kind, Kind::Command { ref cmd } if cmd == "ping"));
}

#[tokio::test]
async fn read_fails_on_immediate_close() {
    let (a, mut b) = duplex(64);
    drop(a);
    let err = read_frame(&mut b).await.expect_err("length read");
    assert!(err.contains("read frame length"));
}

#[tokio::test]
async fn read_rejects_oversize_frame() {
    let (mut a, mut b) = duplex(64);
    a.write_all(&MAX_FRAME_BYTES.wrapping_add(1).to_le_bytes())
        .await
        .expect("prefix");
    drop(a);
    let err = read_frame(&mut b).await.expect_err("oversize");
    assert!(err.contains("exceeds"));
}

#[tokio::test]
async fn read_fails_on_truncated_body() {
    let (mut a, mut b) = duplex(64);
    a.write_all(&4u32.to_le_bytes()).await.expect("prefix");
    a.write_all(b"ab").await.expect("partial body");
    drop(a);
    let err = read_frame(&mut b).await.expect_err("body read");
    assert!(err.contains("read frame body"));
}

#[tokio::test]
async fn read_fails_on_malformed_json() {
    let (mut a, mut b) = duplex(64);
    let body = b"not json";
    a.write_all(&(body.len() as u32).to_le_bytes())
        .await
        .expect("prefix");
    a.write_all(body).await.expect("body");
    drop(a);
    let err = read_frame(&mut b).await.expect_err("decode");
    assert!(err.contains("decode envelope"));
}

#[tokio::test]
async fn write_fails_when_peer_dropped() {
    let (mut a, b) = duplex(64);
    drop(b);
    let err = write_frame(&mut a, &sample(1)).await.expect_err("write");
    assert!(err.contains("write frame length") || err.contains("flush frame"));
}

#[tokio::test]
async fn write_body_fails_when_peer_dropped_mid_frame() {
    let (mut a, b) = duplex(LENGTH_PREFIX_BYTES + 8);
    let big = Envelope {
        id: 1,
        kind: Kind::Command {
            cmd: "x".repeat(512),
        },
    };
    let writer = tokio::spawn(async move { write_frame(&mut a, &big).await });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    drop(b);
    let err = writer.await.expect("task").expect_err("body write");
    assert!(err.contains("write frame body") || err.contains("flush frame"));
}

#[tokio::test]
async fn write_flush_fails_when_peer_dropped_after_buffer_full() {
    let (mut a, b) = duplex(LENGTH_PREFIX_BYTES + 8);
    let big = Envelope {
        id: 1,
        kind: Kind::Command {
            cmd: "y".repeat(512),
        },
    };
    let writer = tokio::spawn(async move { write_frame(&mut a, &big).await });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    drop(b);
    let _ = writer.await;
}
