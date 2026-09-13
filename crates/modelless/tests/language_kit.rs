use modelless::chain_gram::ChainGram;
use modelless::corpus;
use modelless::sentencepiece::{SentencePiece, UNK};

#[test]
fn sentencepiece_roundtrips_corpus_words() {
    let text = corpus::text();
    let sp = SentencePiece::train(&text, 4096);
    assert!(sp.vocab_size() > 1);
    assert_eq!(sp.piece(0), UNK);
    let mut misses = 0;
    for word in corpus::words() {
        let ids = sp.encode(word);
        assert!(!ids.is_empty());
        if sp.decode(&ids) != word {
            misses += 1;
        }
    }
    assert_eq!(misses, 0, "every corpus word must survive encode/decode");
}

#[test]
fn sentencepiece_unknown_char_maps_to_unk() {
    let sp = SentencePiece::train(corpus::text().as_str(), 512);
    let ids = sp.encode("\u{03A9}\u{03A9}\u{03A9}");
    assert!(ids.iter().all(|&id| sp.piece(id) == UNK));
    assert_eq!(sp.decode(&ids), UNK.repeat(3));
}

#[test]
fn chain_gram_predicts_and_generates() {
    let words: Vec<&str> = corpus::words().collect();
    let gram = ChainGram::train_seq(&words, 2);
    assert!(gram.context_count() > 0);
    let seed: Vec<&str> = words[..2].to_vec();
    let next = gram.next(&seed);
    assert!(next.is_some(), "context from corpus must continue");
    let out = gram.generate(&seed, 5);
    assert!(out.len() >= seed.len());
    assert!(out.len() <= seed.len() + 5);
}

#[test]
fn chain_gram_candidates_sorted_by_token() {
    let words: Vec<&str> = corpus::words().collect();
    let gram = ChainGram::train_seq(&words, 1);
    let rows = gram.candidates(&[words[0]]);
    assert!(rows.windows(2).all(|w| w[0].0 <= w[1].0));
}
