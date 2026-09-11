use std::path::PathBuf;

use token_gate_adapter::{DraftConfig, DraftGate, GateKind, TokenGate};

const MIN_WORD_LEN: &str = r#"{
  "version": "1.0",
  "model": {
    "type": "BPE",
    "vocab": {
      "<bos>": 0,
      "hel": 1,
      "lo": 2,
      "▁world": 3,
      "▁": 4,
      "h": 5,
      "▁h": 6
    },
    "merges": [["▁", "h"]]
  },
  "added_tokens": [
    { "id": 0, "content": "<bos>", "special": true, "single_word": false, "lstrip": false, "rstrip": false, "normalized": false }
  ]
}"#;

fn gate_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join("tga_test_gate");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(token_gate_adapter::TOKENIZER_FILE), MIN_WORD_LEN).unwrap();
        dir
    })
    .clone()
}

#[test]
fn gate_kind_parse_roundtrip() {
    assert_eq!(GateKind::parse(Some("katgpt")), GateKind::Katgpt);
    assert_eq!(GateKind::parse(Some("normal")), GateKind::Normal);
    assert_eq!(GateKind::parse(None), GateKind::Normal);
    assert_eq!(GateKind::parse(Some("bogus")), GateKind::Normal);
    assert_eq!(GateKind::Katgpt.as_str(), "katgpt");
    assert_eq!(GateKind::Normal.as_str(), "normal");
}

#[test]
fn katgpt_gate_encode_token_id_decode() {
    let dir = gate_dir();
    let gate = token_gate_adapter::load(&dir, GateKind::Katgpt).unwrap();
    assert_eq!(gate.kind(), GateKind::Katgpt);

    assert_eq!(gate.token_id("<bos>"), Some(0));
    assert_eq!(gate.token_id("missing"), None);

    let ids = gate.encode("hello", false).unwrap();
    assert!(!ids.is_empty());
    // every id stays in vocab
    for id in &ids {
        assert!(*id < 7);
    }
    // add_special prepends <bos> (bos != pad here: pad defaults to 0 = bos?
    // no — both resolve to id 0 via special_id fallback, so BOS is skipped)
    let with_special = gate.encode("hello", true).unwrap();
    assert_eq!(with_special.len(), ids.len());
}

#[test]
fn load_normal_reads_same_file() {
    let dir = gate_dir();
    let gate = token_gate_adapter::load(&dir, GateKind::Normal).unwrap();
    assert_eq!(gate.kind(), GateKind::Normal);
    let ids = gate.encode("hello", false).unwrap();
    assert!(!ids.is_empty());
}

#[test]
fn bigram_draft_window_respects_config() {
    let hist: Vec<u32> = vec![1, 2, 1, 2, 1, 2, 3, 9, 9, 9];
    let mut draft_gate = token_gate_adapter::BigramDraft::build(&hist, 16, DraftConfig::default());
    let out = draft_gate.draft(&hist, 1);
    assert!(out.len() <= DraftConfig::default().steps);
}

// verify trait objects work as the abstract layer's public surface
#[test]
fn gates_are_object_safe() {
    fn assert_gate(_g: &dyn TokenGate) {}
    fn assert_draft(_d: &mut dyn DraftGate) {}
    let dir = gate_dir();
    assert_gate(
        token_gate_adapter::load(&dir, GateKind::Katgpt)
            .unwrap()
            .as_ref(),
    );
    assert_draft(&mut token_gate_adapter::BigramDraft::build(
        &[1, 2, 3],
        8,
        DraftConfig::default(),
    ));
}
