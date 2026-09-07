use susutaku_mlx::tok::{from_bytes, ChatTok, TokKind};

const KATGPT: TokKind = TokKind::Katgpt;
const NORMAL: TokKind = TokKind::Normal;

fn fake_tokenizer_json() -> Vec<u8> {
    // Tiny BPE: chars a, b, ▁ space marker, merge a+b; bos is special.
    serde_json::json!({
        "model": {
            "type": "BPE",
            "vocab": { "a": 0, "b": 1, "\u{2581}": 2, "ab": 3 },
            "merges": [["a", "b"]]
        },
        "added_tokens": [
            { "content": "<bos>", "id": 4 },
            { "content": "<turn|>", "id": 5 }
        ]
    })
    .to_string()
    .into_bytes()
}

#[test]
fn added_tokens_encode_atomically() {
    let t = from_bytes(&fake_tokenizer_json()).expect("build");
    let chat = ChatTok::Katgpt(Box::new(t));
    // the marker must be the single id 5, not BPE fragments of "<turn|>"
    let ids = chat.encode("<turn|>ab", false).expect("encode");
    assert_eq!(ids, vec![5, 3]);
}

#[test]
fn roundtrip_and_byte_level() {
    let t = from_bytes(&fake_tokenizer_json()).expect("build");
    let chat = ChatTok::Katgpt(Box::new(t));
    // '▁' is the space marker, so "a b" must decode back with a space.
    let ids = chat.encode("ab", false).expect("encode");
    assert_eq!(ids, vec![3]);
    let ids = chat.encode("a b", false).expect("encode");
    assert_eq!(chat.decode(&ids).expect("decode"), "a b");
}

#[test]
fn decode_skips_special_tokens() {
    let t = from_bytes(&fake_tokenizer_json()).expect("build");
    let chat = ChatTok::Katgpt(Box::new(t));
    // bos (id 4, added_tokens) must not leak into the decoded text
    let ids = chat.encode("ab", true).expect("encode");
    assert_eq!(ids, vec![4, 3]);
    assert_eq!(chat.decode(&ids).expect("decode"), "ab");
}

#[test]
fn bos_prepended_only_when_requested_and_defined() {
    let t = from_bytes(&fake_tokenizer_json()).expect("build");
    let chat = ChatTok::Katgpt(Box::new(t));
    assert_eq!(
        chat.encode("a", true).expect("encode")[0],
        4,
        "bos id from added_tokens"
    );
}

#[test]
fn tok_kind_parse() {
    assert_eq!(TokKind::parse(Some("katgpt")), KATGPT);
    assert_eq!(TokKind::parse(Some("normal")), NORMAL);
    assert_eq!(TokKind::parse(None), NORMAL);
    assert_eq!(KATGPT.as_str(), "katgpt");
}

#[test]
fn rejects_non_bpe() {
    let json = serde_json::json!({ "model": { "type": "Unigram" } });
    assert!(from_bytes(json.to_string().as_bytes()).is_err());
}
