//! Tokenizer selection: HF `tokenizers` (normal) or katgpt-tokenizer BPE.
//! Both wrap the same `tokenizer.json` vocab/merges, so token IDs stay
//! compatible; the katgpt path differs only in pretokenization strategy.

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use katgpt_tokenizer::{BpeTokenizer, BpeTokenizerImpl, MergeRule};

pub const TOKENIZER_FILE: &str = "tokenizer.json";
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
const SPACE_CHAR: char = ' ';
const SPACE_BYTE: u8 = b' ';

/// Which tokenizer implementation decodes/encodes for a generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TokKind {
    #[default]
    Normal,
    Katgpt,
}

impl TokKind {
    /// API string → kind; anything unknown falls back to normal.
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("katgpt") => Self::Katgpt,
            _ => Self::Normal,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Katgpt => "katgpt",
        }
    }
}

/// Generation-ready tokenizer: normal (HF) or katgpt BPE over the same vocab.
pub enum ChatTok {
    Normal(Box<tokenizers::Tokenizer>),
    Katgpt(Box<KatgptTok>),
}

/// katgpt BPE plus the tokenizer.json metadata it needs: which ids are
/// special (skipped on decode) — the BPE impl itself is metadata-blind.
pub struct KatgptTok {
    inner: BpeTokenizer,
    special: Vec<bool>,
    /// added_tokens (content, id), longest content first — atomic on encode
    added: Vec<(String, u32)>,
}

impl ChatTok {
    pub fn load(dir: &Path, kind: TokKind) -> Result<Self, String> {
        let file = dir.join(TOKENIZER_FILE);
        match kind {
            TokKind::Normal => tokenizers::Tokenizer::from_file(&file)
                .map_err(|e| format!("failed to read {}: {e}", file.display()))
                .map(|t| Self::Normal(Box::new(t))),
            TokKind::Katgpt => {
                let bytes = std::fs::read(&file)
                    .map_err(|e| format!("failed to read {}: {e}", file.display()))?;
                from_bytes(&bytes).map(|t| Self::Katgpt(Box::new(t)))
            }
        }
    }

    /// Encode `text`; `add_special` prepends BOS when the vocab defines one.
    pub fn encode(&self, text: &str, add_special: bool) -> Result<Vec<u32>, String> {
        match self {
            Self::Normal(t) => {
                let enc = t.encode(text, add_special).map_err(|e| e.to_string())?;
                Ok(enc.get_ids().to_vec())
            }
            Self::Katgpt(t) => {
                // added/special tokens are atomic: matched spans emit their
                // single id directly (the trained markers must not be BPE'd
                // into text fragments); everything else BPEs as before.
                let mut ids = Vec::new();
                let mut rest = text;
                while !rest.is_empty() {
                    if let Some((len, id)) = t.match_added(rest) {
                        ids.push(id);
                        rest = &rest[len..];
                        continue;
                    }
                    // segment = everything up to the earliest added-token
                    // occurrence, so BPE still merges freely inside it
                    let cut = t
                        .added
                        .iter()
                        .filter_map(|(s, _)| rest.find(s.as_str()))
                        .filter(|&p| p > 0)
                        .min()
                        .unwrap_or(rest.len());
                    let (seg, tail) = rest.split_at(cut);
                    ids.extend(t.encode_segment(seg)?);
                    rest = tail;
                }
                if add_special && t.inner.bos_id != t.inner.pad_id {
                    ids.insert(0, t.inner.bos_id as u32);
                }
                Ok(ids)
            }
        }
    }

    /// Look up one special-token id by content, when the vocab defines it.
    pub fn token_id(&self, token: &str) -> Option<u32> {
        match self {
            Self::Normal(t) => t.token_to_id(token),
            Self::Katgpt(t) => t.inner.vocab_to_id.get(token).map(|&id| id as u32),
        }
    }

    pub fn decode(&self, ids: &[u32]) -> Result<String, String> {
        match self {
            Self::Normal(t) => t.decode(ids, true).map_err(|e| e.to_string()),
            Self::Katgpt(t) => {
                let kept: Vec<usize> = ids
                    .iter()
                    .filter(|&&id| !t.is_special(id))
                    .map(|&id| id as usize)
                    .collect();
                // concat vocab strings directly — the crate's decode drops ▁
                let mapped: String = kept
                    .iter()
                    .filter_map(|&i| t.inner.id_to_vocab.get(i).map(String::as_str))
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
        }
    }
}

/// Build a katgpt `BpeTokenizer` from an HF `tokenizer.json` (BPE model only).
fn from_bytes(bytes: &[u8]) -> Result<KatgptTok, String> {
    let json: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("bad tokenizer.json: {e}"))?;
    let model = json
        .get("model")
        .ok_or("tokenizer.json has no model section")?;
    let ty = model.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if ty != BPE_MODEL_TYPE {
        return Err(format!("katgpt tokenizer supports BPE only, got `{ty}`"));
    }

    let vocab_json = model
        .get(VOCAB_FIELD)
        .and_then(|v| v.as_object())
        .ok_or("tokenizer.json model has no vocab object")?;
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
    Ok(KatgptTok {
        inner: tok,
        special,
        added,
    })
}

impl KatgptTok {
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
    fn encode_segment(&self, text: &str) -> Result<Vec<u32>, String> {
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
fn parse_merges(value: Option<&serde_json::Value>) -> Result<Vec<MergeRule>, String> {
    let items = value
        .and_then(|v| v.as_array())
        .ok_or("tokenizer.json model has no merges array")?;
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
            return Err("unparsable merge entry in tokenizer.json".to_string());
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

fn build_byte_to_unicode() -> [char; 256] {
    let mut table = ['\0'; 256];
    let mut extra: u32 = 0;
    for (byte, slot) in table.iter_mut().enumerate() {
        let b = byte as u32;
        *slot = if (33..=126).contains(&b) || (161..=172).contains(&b) || (174..=255).contains(&b) {
            char::from_u32(b).expect("printable byte is a char")
        } else {
            let c = char::from_u32(256 + extra).expect("mapped codepoint is a char");
            extra += 1;
            c
        };
    }
    table
}

static BYTE_TO_UNICODE: LazyLock<[char; 256]> = LazyLock::new(build_byte_to_unicode);

static UNICODE_TO_BYTE: LazyLock<HashMap<char, u8>> = LazyLock::new(|| {
    BYTE_TO_UNICODE
        .iter()
        .enumerate()
        .map(|(byte, ch)| (*ch, byte as u8))
        .collect()
});

#[cfg(test)]
mod tests {
    use super::*;

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
}
