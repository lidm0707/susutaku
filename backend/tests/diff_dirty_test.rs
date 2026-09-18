//! Dirty-diff detection: the manager's GIT DIFF renders an empty diff as
//! the literal "(no changes)" placeholder — that is NOT work.

use backend::app::card_run::diff_is_dirty;

#[test]
fn placeholder_is_clean() {
    assert!(!diff_is_dirty("(no changes)"));
    assert!(!diff_is_dirty("  (no changes)\n"));
}

#[test]
fn empty_is_clean() {
    assert!(!diff_is_dirty(""));
    assert!(!diff_is_dirty("  \n"));
}

#[test]
fn a_real_patch_is_dirty() {
    assert!(diff_is_dirty(
        "diff --git a/src/lib.rs b/src/lib.rs\n+fn main() {}\n"
    ));
}
