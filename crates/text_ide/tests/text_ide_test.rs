#![allow(clippy::unwrap_used)] // tests: unwrap is the assertion tool

use text_ide::{Buffer, Editor, Position, Selection, find_all};

#[test]
fn buffer_new_and_from_text() {
    let b = Buffer::new();
    assert!(b.is_empty());
    assert_eq!(b.line_count(), 1);
    let b = Buffer::from_text("ab\ncd");
    assert_eq!(b.line_count(), 2);
    assert_eq!(b.line(1), Some("cd"));
    assert_eq!(b.text(), "ab\ncd");
}

#[test]
fn insert_chars_and_split() {
    let mut b = Buffer::new();
    let mut p = Position::default();
    for c in "héllo".chars() {
        p = b.insert_char(p, c).unwrap();
    }
    assert_eq!(b.text(), "héllo");
    assert_eq!(p, Position::new(0, 5));
    let p = b.split_line(Position::new(0, 2)).unwrap();
    assert_eq!(b.text(), "hé\nllo");
    assert_eq!(p, Position::new(1, 0));
}

#[test]
fn delete_before_joins_lines() {
    let mut b = Buffer::from_text("ab\ncd");
    let p = b.delete_before(Position::new(1, 0)).unwrap();
    assert_eq!(b.text(), "abcd");
    assert_eq!(p, Position::new(0, 2));
    assert_eq!(b.delete_before(Position::new(0, 0)), None);
}

#[test]
fn delete_at_merges_next_line() {
    let mut b = Buffer::from_text("ab\ncd");
    let p = b.delete_at(Position::new(0, 2)).unwrap();
    assert_eq!(b.text(), "abcd");
    assert_eq!(p, Position::new(0, 2));
    assert_eq!(b.delete_at(Position::new(0, 4)), None);
}

#[test]
fn utf8_safety() {
    let mut b = Buffer::from_text("héllo");
    let p = b.delete_before(Position::new(0, 2)).unwrap();
    assert_eq!(b.text(), "hllo");
    assert_eq!(p, Position::new(0, 1));
}

#[test]
fn unindent_respects_width() {
    let mut b = Buffer::from_text("    x");
    assert_eq!(b.unindent_line(0), 4);
    assert_eq!(b.line(0), Some("x"));
    assert_eq!(b.unindent_line(0), 0);
    assert_eq!(b.line(0), Some("x"));
    assert_eq!(b.unindent_line(99), 0);
}

#[test]
fn cursor_movement() {
    let mut c = text_ide::Cursor::at(0, 0);
    c.right(3);
    c.right(3);
    c.right(3);
    assert_eq!(c.pos, Position::new(0, 3));
    c.right(3);
    assert_eq!(c.pos, Position::new(1, 0));
    c.left(4);
    assert_eq!(c.pos, Position::new(0, 4));
    c.down(2);
    c.up(2);
    assert_eq!(c.pos, Position::new(0, 2)); // clamped to line 0 length
    c.line_start();
    assert_eq!(c.pos, Position::new(0, 0));
    c.line_end(7);
    assert_eq!(c.pos, Position::new(0, 7));
}

#[test]
fn selection_range_orders_anchors() {
    let s = Selection {
        anchor: Position::new(1, 4),
        head: Position::new(0, 2),
    };
    assert!(!s.is_empty());
    assert_eq!(s.range(), (Position::new(0, 2), Position::new(1, 4)));
    let e = Selection::new(Position::new(3, 3));
    assert!(e.is_empty());
}

#[test]
fn find_all_matches() {
    let lines = Buffer::from_text("ab ab\nbab").lines().to_vec();
    let m = find_all(&lines, "ab");
    assert_eq!(m.len(), 3);
    assert_eq!(m[0].start, Position::new(0, 0));
    assert_eq!(m[1].start, Position::new(0, 3));
    assert_eq!(m[2].start, Position::new(1, 1));
    assert!(find_all(&lines, "").is_empty());
    assert!(find_all(&lines, "zz").is_empty());
}

#[test]
fn editor_typing_and_history() {
    let mut e = Editor::new();
    for c in "hi!".chars() {
        e.insert_char(c);
    }
    assert_eq!(e.buffer.text(), "hi!");
    e.backspace();
    assert_eq!(e.buffer.text(), "hi");
    assert!(e.can_undo());
    e.undo();
    assert_eq!(e.buffer.text(), "hi!");
    assert!(e.can_redo());
    e.redo();
    assert_eq!(e.buffer.text(), "hi");
}

#[test]
fn editor_newline_undo() {
    let mut e = Editor::with_text("ab");
    e.cursor.pos = Position::new(0, 1);
    e.newline();
    assert_eq!(e.buffer.text(), "a\nb");
    e.undo();
    assert_eq!(e.buffer.text(), "ab");
}

#[test]
fn editor_backspace_across_lines_roundtrip() {
    let mut e = Editor::with_text("ab\ncd");
    e.cursor.pos = Position::new(1, 0);
    e.backspace();
    assert_eq!(e.buffer.text(), "abcd");
    e.undo();
    assert_eq!(e.buffer.text(), "ab\ncd");
    assert_eq!(e.cursor.pos, Position::new(0, 2));
    e.redo();
    assert_eq!(e.buffer.text(), "abcd");
}

#[test]
fn editor_delete_join_roundtrip() {
    let mut e = Editor::with_text("ab\ncd");
    e.cursor.pos = Position::new(0, 2);
    e.delete();
    assert_eq!(e.buffer.text(), "abcd");
    e.undo();
    assert_eq!(e.buffer.text(), "ab\ncd");
}

#[test]
fn editor_unindent_undo() {
    let mut e = Editor::with_text("    x");
    e.unindent_current();
    assert_eq!(e.buffer.text(), "x");
    e.undo();
    assert_eq!(e.buffer.text(), "    x");
}

#[test]
fn redo_cleared_on_new_edit() {
    let mut e = Editor::new();
    e.insert_char('a');
    e.undo();
    assert!(e.can_redo());
    e.insert_char('b');
    assert!(!e.can_redo());
}

#[test]
fn empty_editor_ops_are_noops() {
    let mut e = Editor::new();
    e.backspace();
    e.delete();
    e.undo();
    e.redo();
    assert_eq!(e.buffer.text(), "");
    assert_eq!(e.buffer.char_count(), 0);
}
