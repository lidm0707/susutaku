//! SentencePiece-style subword tokenizer: frequent-substring seed vocab with
//! unigram log-prob scores and Viterbi segmentation. Pure Rust, zero-copy
//! during encode.

use std::collections::HashMap;

pub const UNK: &str = "<unk>";
pub const MAX_PIECE_CHARS: usize = 12;
pub const MIN_PIECE_FREQ: usize = 3;
pub const UNK_LOG_PROB: f64 = -50.0;

pub struct SentencePiece {
    pieces: Vec<String>,
    index: HashMap<String, usize>,
    log_probs: Vec<f64>,
}

#[derive(Clone, Copy)]
struct Candidate<'a> {
    piece: &'a str,
    freq: usize,
}

impl SentencePiece {
    /// Train on whitespace-separated text: all distinct chars plus frequent
    /// substrings, ranked by frequency, capped at `vocab_size`.
    pub fn train(text: &str, vocab_size: usize) -> Self {
        let mut freqs: HashMap<String, usize> = HashMap::new();
        for word in text.split_whitespace() {
            let offs = char_offsets(word);
            for i in 0..offs.len() - 1 {
                for j in (i + 1)..offs.len() {
                    if j - i > MAX_PIECE_CHARS {
                        break;
                    }
                    let piece = &word[offs[i]..offs[j]];
                    *freqs.entry(piece.to_owned()).or_default() += 1;
                }
            }
        }

        let mut singles: Vec<Candidate> = freqs
            .iter()
            .filter(|(p, _)| p.chars().count() == 1)
            .map(|(p, f)| Candidate {
                piece: p.as_str(),
                freq: *f,
            })
            .collect();
        singles.sort();
        let head = singles.iter().map(|c| (c.piece.to_owned(), c.freq));

        let mut multi: Vec<Candidate> = freqs
            .iter()
            .filter(|(p, f)| p.chars().count() > 1 && **f >= MIN_PIECE_FREQ)
            .map(|(p, f)| Candidate {
                piece: p.as_str(),
                freq: *f,
            })
            .collect();
        multi.sort();

        let picked = head
            .chain(multi.into_iter().map(|c| (c.piece.to_owned(), c.freq)))
            .take(vocab_size);
        let kept: Vec<(String, usize)> = picked.collect();

        let mut pieces = vec![UNK.to_owned()];
        pieces.extend(kept.iter().map(|(piece, _)| piece.clone()));

        let freq_total: usize = kept.iter().map(|(_, freq)| *freq).sum();
        let mut log_probs = vec![UNK_LOG_PROB];
        for (_, freq) in &kept {
            log_probs.push(((*freq as f64) / (freq_total as f64)).ln());
        }

        let index = pieces
            .iter()
            .enumerate()
            .map(|(i, p)| (p.clone(), i))
            .collect();
        Self {
            pieces,
            index,
            log_probs,
        }
    }

    pub fn piece(&self, id: usize) -> &str {
        &self.pieces[id]
    }

    pub fn vocab_size(&self) -> usize {
        self.pieces.len()
    }

    /// Viterbi segmentation per whitespace-separated word; unknown chars map
    /// to the UNK id.
    pub fn encode(&self, text: &str) -> Vec<usize> {
        let mut ids = Vec::new();
        for word in text.split_whitespace() {
            ids.extend(self.encode_word(word));
        }
        ids
    }

    fn encode_word(&self, word: &str) -> Vec<usize> {
        let offs = char_offsets(word);
        let n = offs.len() - 1;
        let mut best = vec![f64::NEG_INFINITY; n + 1];
        let mut back = vec![0usize; n + 1];
        best[0] = 0.0;
        for i in 0..n {
            if !best[i].is_finite() {
                continue;
            }
            for j in (i + 1)..=n {
                if j - i > MAX_PIECE_CHARS {
                    break;
                }
                let Some(&id) = self.index.get(&word[offs[i]..offs[j]]) else {
                    continue;
                };
                let score = best[i] + self.log_probs[id];
                if score > best[j] {
                    best[j] = score;
                    back[j] = i;
                }
            }
        }
        if !best[n].is_finite() {
            return (0..n).map(|_| 0).collect();
        }
        let mut ids = Vec::new();
        let mut at = n;
        while at > 0 {
            let from = back[at];
            let piece = &word[offs[from]..offs[at]];
            ids.push(self.index[piece]);
            at = from;
        }
        ids.reverse();
        ids
    }

    pub fn decode(&self, ids: &[usize]) -> String {
        let mut out = String::new();
        for &id in ids {
            out.push_str(&self.pieces[id]);
        }
        out
    }
}

impl Ord for Candidate<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.freq.cmp(&self.freq).then_with(|| {
            other
                .piece
                .chars()
                .count()
                .cmp(&self.piece.chars().count())
                .then_with(|| self.piece.cmp(other.piece))
        })
    }
}

impl PartialOrd for Candidate<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for Candidate<'_> {}

impl PartialEq for Candidate<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

fn char_offsets(word: &str) -> Vec<usize> {
    let mut offs: Vec<usize> = word.char_indices().map(|(b, _)| b).collect();
    offs.push(word.len());
    offs
}
