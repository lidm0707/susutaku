//! Evidence tail: failed-run logs must keep the END of the transcript (the
//! last reply / where the loop stopped), not the beginning.

use backend::app::card_run::tail;

#[test]
fn short_string_passes_through() {
    assert_eq!(tail("abc", 10), "abc");
}

#[test]
fn long_string_keeps_the_last_chars() {
    let s = "abcdef";
    assert_eq!(tail(s, 2), "ef");
}

#[test]
fn max_zero_gives_empty() {
    assert_eq!(tail("abc", 0), "");
}
