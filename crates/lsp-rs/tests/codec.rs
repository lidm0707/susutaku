use std::io::{BufReader, Cursor};

use lsp_rs::Severity;
use lsp_rs::codec::{read_message, write_message};
use lsp_rs::types::utf16_col;

const BODY: &str = "{\"jsonrpc\":\"2.0\",\"id\":1}";
const HEADER: &str = "Content-Length: 24\r\n\r\n";

#[test]
fn message_round_trip() {
    let mut wire = Vec::new();
    write_message(&mut wire, BODY.as_bytes()).expect("write");
    assert_eq!(wire, format!("{HEADER}{BODY}").into_bytes());

    let mut body = Vec::new();
    read_message(&mut BufReader::new(Cursor::new(&wire)), &mut body).expect("read");
    assert_eq!(body, BODY.as_bytes());
}

#[test]
fn read_two_messages_sequentially() {
    let mut wire = Vec::new();
    write_message(&mut wire, b"one").unwrap();
    write_message(&mut wire, b"second!!").unwrap();
    let mut reader = BufReader::new(Cursor::new(wire));
    let mut body = Vec::new();
    read_message(&mut reader, &mut body).unwrap();
    assert_eq!(body, b"one");
    read_message(&mut reader, &mut body).unwrap();
    assert_eq!(body, b"second!!");
}

#[test]
fn utf16_column_matches_lsp_encoding() {
    let line = "héllo wörld";
    assert_eq!(utf16_col(line, 1), 1);
    assert_eq!(utf16_col(line, 3), 2);
    let emoji = concat!("a", "\u{1F980}", "b");
    assert_eq!(utf16_col(emoji, 5), 3);
}

#[test]
fn severity_mapping() {
    assert_eq!(Severity::from_num(1).as_str(), "error");
    assert_eq!(Severity::from_num(2).as_str(), "warning");
    assert_eq!(Severity::from_num(99).as_str(), "hint");
}
