use modelless::corpus;
use modelless::engine::Engine;

#[test]
fn corpus_has_3000_words() {
    assert_eq!(corpus::WORD_COUNT, 3000);
    assert_eq!(corpus::words().count(), corpus::WORD_COUNT);
    assert_eq!(corpus::verbs().count(), corpus::VERB_COUNT);
}

#[test]
fn regular_inflection() {
    assert_eq!(
        corpus::Verb::regular("walk").form(corpus::Form::Past),
        "walked"
    );
    assert_eq!(
        corpus::Verb::regular("love").form(corpus::Form::Past),
        "loved"
    );
    assert_eq!(
        corpus::Verb::regular("carry").form(corpus::Form::Past),
        "carried"
    );
    assert_eq!(
        corpus::Verb::regular("stop").form(corpus::Form::Past),
        "stopped"
    );
    assert_eq!(
        corpus::Verb::regular("mix").form(corpus::Form::Past),
        "mixed"
    );
    assert_eq!(
        corpus::Verb::regular("walk").form(corpus::Form::PastParticiple),
        "walked"
    );
}

#[test]
fn irregular_lookup() {
    let go = corpus::verbs().find(|v| v.base == "go").unwrap();
    assert_eq!(go.form(corpus::Form::Past), "went");
    assert_eq!(go.form(corpus::Form::PastParticiple), "gone");
}

#[test]
fn engine_roundtrip() {
    let engine = Engine::train();
    let word = "walked";
    let ids = engine.encode(word);
    assert!(!ids.is_empty());
    assert_eq!(engine.decode(&ids), word);
    assert!(engine.roundtrip_hits() > corpus::WORD_COUNT / 2);
}
