//! katgpt BPE backend for [`TokenGate`], built from an HF `tokenizer.json`.
//!
//! The `BpeTokenizer` impl is metadata-blind, so this module carries the
//! tokenizer.json metadata it needs: which ids are special (skipped on
//! decode) and the added_tokens matched atomically on encode.

use std::collections::HashMap;

use katgpt_tokenizer::{BpeTokenizer, BpeTokenizerImpl, MergeRule};

use crate::byte_map::{BYTE_TO_UNICODE, SPACE_BYTE, SPACE_CHAR, UNICODE_TO_BYTE};
use crate::{GateKind, GateResult, TokenGate};

const BOS_TOKEN: &str = "<bos>";
const EOS_TOKEN: &str = "<eos>";
const PAD_TOKEN: &str = "<pad>";
const BPE_MODEL_TYPE: &str = "BPE";
const MERGES_FIELD: &str = "merges";
const VOCAB_FIELD: &str = "vocab";
const ADDED_TOKENS_FIELD: &str = "added_tokens";
const CONTENT_FIELD: &str = "content";
const ID_FIELD: &str = "id";
const SP: &str = "\u{2581}";

/// katgpt BPE plus the tokenizer.json metadata the raw BPE impl lacks.
pub struct KatgptGate {
    inner: BpeTokenizer,
    special: Vec<bool>,
    /// added_tokens (content, id), longest content first — atomic on encode
    added: Vec<(String, u32)>,
}

impl KatgptGate {
    /// Build from an HF `tokenizer.json` (BPE model only).
    pub fn from_bytes(bytes: &[u8]) -> GateResult<Self> {
        let json: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|e| crate::GateError(format!("bad tokenizer.json: {e}")))?;
        let model = json
            .get("model")
            .ok_or_else(|| crate::GateError("tokenizer.json has no model section".into()))?;
        let ty = model.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ty != BPE_MODEL_TYPE {
            return Err(crate::GateError(format!(
                "katgpt tokenizer supports BPE only, got `{ty}`"
            )));
        }

        let vocab_json = model
            .get(VOCAB_FIELD)
            .and_then(|v| v.as_object())
            .ok_or_else(|| crate::GateError("tokenizer.json model has no vocab object".into()))?;
        let mut vocab_to_id: HashMap<String, usize> = HashMap::with_capacity(vocab_json.len());
        let mut id_to_vocab: Vec<String> = vec![String::new(); vocab_json.len()];
        for (token, id) in vocab_json {
            if let Some(id) = id.as_u64() {
                let id = id as usize;
                vocab_to_id.insert(token.clone(), id);
                if id >= id_to_vocab.len() {
                    id_to_vocab.resize(id + 1, String::new());
                }
                id_to_vocab[id] = token.clone();
            }
        }

        let merges = parse_merges(model.get(MERGES_FIELD))?;
        let special = special_table(&json, id_to_vocab.len());

        let mut tok = BpeTokenizer {
            vocab_to_id,
            id_to_vocab,
            merges,
            merge_ranks: HashMap::new(),
            merge_ranks_id: HashMap::new(),
            merge_target_id: Vec::new(),
            bos_id: special_id(&json, BOS_TOKEN),
            eos_id: special_id(&json, EOS_TOKEN),
            pad_id: special_id(&json, PAD_TOKEN),
        };
        tok.rebuild_ranks();
        let mut added: Vec<(String, u32)> = json
            .get(ADDED_TOKENS_FIELD)
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|t| {
                        let content = t.get(CONTENT_FIELD)?.as_str()?.to_string();
                        let id = t.get(ID_FIELD)?.as_u64()? as u32;
                        Some((content, id))
                    })
                    .collect()
            })
            .unwrap_or_default();
        added.sort_by_key(|(s, _)| std::cmp::Reverse(s.len()));
        Ok(Self {
            inner: tok,
            special,
            added,
        })
    }

    fn is_special(&self, id: u32) -> bool {
        self.special.get(id as usize).copied().unwrap_or(false)
    }

    /// Longest added-token match at the start of `text` → (byte len, id).
    fn match_added(&self, text: &str) -> Option<(usize, u32)> {
        self.added
            .iter()
            .find(|(s, _)| text.starts_with(s.as_str()))
            .map(|(s, id)| (s.len(), *id))
    }

    /// The original char-map + BPE path for one added-token-free segment.
    fn encode_segment(&self, text: &str) -> GateResult<Vec<u32>> {
        let mapped: String = text
            .chars()
            .flat_map(|c| {
                if c == SPACE_CHAR {
                    SP.chars().collect::<Vec<char>>()
                } else {
                    let mut buf = [0u8; 4];
                    c.encode_utf8(&mut buf)
                        .bytes()
                        .map(|b| BYTE_TO_UNICODE[b as usize])
                        .collect::<Vec<char>>()
                }
            })
            .collect();
        Ok(BpeTokenizerImpl::encode(&self.inner, &mapped)
            .into_iter()
            .map(|id| id as u32)
            .collect())
    }
}

impl TokenGate for KatgptGate {
    fn kind(&self) -> GateKind {
        GateKind::Katgpt
    }

    fn encode(&self, text: &str, add_special: bool) -> GateResult<Vec<u32>> {
        // added/special tokens are atomic: matched spans emit their single id
        // directly (the trained markers must not be BPE'd into text
        // fragments); everything else BPEs as before.
        let mut ids = Vec::new();
        let mut rest = text;
        while !rest.is_empty() {
            if let Some((len, id)) = self.match_added(rest) {
                ids.push(id);
                rest = &rest[len..];
                continue;
            }
            // segment = everything up to the earliest added-token occurrence,
            // so BPE still merges freely inside it
            let cut = self
                .added
                .iter()
                .filter_map(|(s, _)| rest.find(s.as_str()))
                .filter(|&p| p > 0)
                .min()
                .unwrap_or(rest.len());
            let (seg, tail) = rest.split_at(cut);
            ids.extend(self.encode_segment(seg)?);
            rest = tail;
        }
        if add_special && self.inner.bos_id != self.inner.pad_id {
            ids.insert(0, self.inner.bos_id as u32);
        }
        Ok(ids)
    }

    fn decode(&self, ids: &[u32]) -> GateResult<String> {
        let kept: Vec<usize> = ids
            .iter()
            .filter(|&&id| !self.is_special(id))
            .map(|&id| id as usize)
            .collect();
        // concat vocab strings directly — the crate's decode drops ▁
        let mapped: String = kept
            .iter()
            .filter_map(|&i| self.inner.id_to_vocab.get(i).map(String::as_str))
            .collect();
        // undo the ▁ space marker, then the byte-level unicode map
        let bytes: Vec<u8> = mapped
            .replace(SP, " ")
            .chars()
            .filter_map(|c| {
                if c == SPACE_CHAR {
                    Some(SPACE_BYTE)
                } else {
                    UNICODE_TO_BYTE.get(&c).copied()
                }
            })
            .collect();
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn token_id(&self, token: &str) -> Option<u32> {
        self.inner.vocab_to_id.get(token).map(|&id| id as u32)
    }
}

/// True for every id declared in `added_tokens` (special / control tokens).
fn special_table(json: &serde_json::Value, vocab_len: usize) -> Vec<bool> {
    let mut table = vec![false; vocab_len];
    if let Some(items) = json.get(ADDED_TOKENS_FIELD).and_then(|v| v.as_array()) {
        for t in items {
            if let Some(id) = t.get(ID_FIELD).and_then(|v| v.as_u64()) {
                let id = id as usize;
                if id >= table.len() {
                    table.resize(id + 1, false);
                }
                table[id] = true;
            }
        }
    }
    table
}

/// HF merges entries are either `["a", "b"]` pairs or `"a b"` strings.
fn parse_merges(value: Option<&serde_json::Value>) -> GateResult<Vec<MergeRule>> {
    let items = value
        .and_then(|v| v.as_array())
        .ok_or_else(|| crate::GateError("tokenizer.json model has no merges array".into()))?;
    let mut merges = Vec::with_capacity(items.len());
    for item in items {
        let (left, right): (Option<&str>, Option<&str>) = if let Some(pair) = item.as_array() {
            (
                pair.first().and_then(|v| v.as_str()),
                pair.get(1).and_then(|v| v.as_str()),
            )
        } else {
            item.as_str()
                .and_then(|s| s.split_once(' '))
                .map(|(l, r)| (Some(l), Some(r)))
                .unwrap_or((None, None))
        };
        let (Some(left), Some(right)) = (left, right) else {
            return Err(crate::GateError(
                "unparsable merge entry in tokenizer.json".into(),
            ));
        };
        merges.push(MergeRule {
            left: left.to_string(),
            right: right.to_string(),
            merged: format!("{left}{right}"),
        });
    }
    Ok(merges)
}

fn special_id(json: &serde_json::Value, content: &str) -> usize {
    json.get(ADDED_TOKENS_FIELD)
        .and_then(|v| v.as_array())
        .and_then(|items| {
            items
                .iter()
                .find(|t| t.get(CONTENT_FIELD).and_then(|c| c.as_str()) == Some(content))
                .and_then(|t| t.get(ID_FIELD))
                .and_then(|id| id.as_u64())
        })
        .unwrap_or(0) as usize
}
