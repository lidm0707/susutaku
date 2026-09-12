use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use backend::infra::model_client::{ModelCatalog, RemoteModel};
use backend::port::outbound::{Inference, ModelEndpoint, ModelSwitch};
use susutaku_mlx::tok::TokKind;

const STUB_MODEL: &str = "stub-model";
const MODELS_BODY: &str = r#"{"data":[{"id":"stub-model"}]}"#;
const CHAT_BODY: &str = r#"{"choices":[{"message":{"content":"stub reply"}}],"usage":{"prompt_tokens":7,"completion_tokens":3}}"#;
const NOT_FOUND_STATUS: &str = "404 Not Found";
const OK_STATUS: &str = "200 OK";

fn spawn_stub() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let addr = listener.local_addr().expect("stub addr");
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            handle(stream);
        }
    });
    format!("http://{addr}")
}

fn handle(mut stream: TcpStream) {
    let request = read_request(&mut stream);
    let line = request.lines().next().unwrap_or_default().to_string();
    let (status, body) = if line.starts_with("GET /v1/models") {
        (OK_STATUS, MODELS_BODY)
    } else if line.starts_with("POST /v1/chat/completions") {
        (OK_STATUS, CHAT_BODY)
    } else {
        (NOT_FOUND_STATUS, "{}")
    };
    respond(&mut stream, status, body);
}

/// Read until the full request arrived (headers + content-length body).
/// A single read can return a partial request under load, which would make
/// the stub answer (and close) while the client is still sending.
fn read_request(stream: &mut TcpStream) -> String {
    let mut data: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&data);
        let Some(head_end) = text.find("\r\n\r\n") else {
            continue;
        };
        let body_len = text
            .lines()
            .find_map(|l| {
                let (k, v) = l.split_once(':')?;
                if k.eq_ignore_ascii_case("content-length") {
                    v.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        if data.len() >= head_end + 4 + body_len {
            break;
        }
    }
    String::from_utf8_lossy(&data).to_string()
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

#[tokio::test]
async fn openai_stub_list_select_submit() {
    let model = RemoteModel::new(&spawn_stub());

    let models = ModelCatalog::list(&model).expect("list");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, STUB_MODEL);
    assert!(models[0].loadable);
    assert_eq!(models[0].bytes, 0);
    assert_eq!(models[0].engine, "openai");
    assert!(!models[0].selected);

    ModelSwitch::select(&model, STUB_MODEL).expect("select");
    assert_eq!(ModelSwitch::selected(&model).as_deref(), Some(STUB_MODEL));
    let models = ModelCatalog::list(&model).expect("list after select");
    assert!(models[0].selected);

    let rx = model
        .submit("hi".to_owned(), 16, TokKind::Normal, false)
        .expect("submit");
    let reply = rx.await.expect("join").expect("reply");
    assert_eq!(reply.model, STUB_MODEL);
    assert_eq!(reply.text, "stub reply");
    assert_eq!(reply.stats.prompt_tokens, 7);
    assert_eq!(reply.stats.decode_tokens, 3);
    assert_eq!(reply.stats.prompt_secs, 0.0);
    assert_eq!(reply.stats.decode_secs, 0.0);
}

#[test]
fn set_base_url_trims_and_resets() {
    let model = RemoteModel::new(&spawn_stub());
    ModelSwitch::select(&model, STUB_MODEL).expect("select");
    assert_eq!(ModelSwitch::selected(&model).as_deref(), Some(STUB_MODEL));

    model.set_base_url("http://127.0.0.1:1/");
    assert_eq!(model.base_url(), "http://127.0.0.1:1");
    assert_eq!(ModelSwitch::selected(&model), None);
}
