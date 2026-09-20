//! Corpus lookup tests for the policy vocabulary.

use modelless::corpus::{WORD_COUNT, is_word, words};

#[test]
fn corpus_has_3000_forms() {
    assert_eq!(words().count(), WORD_COUNT);
    assert_eq!(WORD_COUNT, 3000);
}

#[test]
fn common_verbs_in_all_three_forms_are_words() {
    assert!(is_word("run"));
    assert!(is_word("ran"));
    assert!(is_word("run"));
    assert!(is_word("delete"));
    assert!(is_word("deleted"));
    assert!(is_word("apply"));
}

#[test]
fn regular_past_forms_are_words() {
    assert!(is_word("deleted"));
    assert!(is_word("created"));
    assert!(is_word("applied"), "apply -> applied (ied rule)");
}

#[test]
fn gibberish_and_non_verbs_are_not_words() {
    assert!(!is_word("shlel"));
    assert!(!is_word("deploy_prod"));
    assert!(!is_word(""));
    assert!(!is_word("cargo"));
}
