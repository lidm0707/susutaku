use crate::buffer::{Buffer, Position};
use crate::cursor::Cursor;

const MAX_HISTORY: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Edit {
    Insert { pos: Position, ch: char },
    DeleteBefore { pos: Position, ch: char },
    DeleteAt { pos: Position, ch: char },
    SplitLine { pos: Position },
    JoinLines { pos: Position },
    BackspaceJoin { line: usize, split_col: usize },
    Unindent { line: usize, spaces: usize },
}

pub struct Editor {
    pub buffer: Buffer,
    pub cursor: Cursor,
    undo_stack: Vec<Edit>,
    redo_stack: Vec<Edit>,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            buffer: Buffer::new(),
            cursor: Cursor::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn with_text(text: &str) -> Self {
        Self {
            buffer: Buffer::from_text(text),
            cursor: Cursor::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn insert_char(&mut self, c: char) {
        let Some(end) = self.buffer.insert_char(self.cursor.pos, c) else {
            return;
        };
        self.push_undo(Edit::Insert {
            pos: self.cursor.pos,
            ch: c,
        });
        self.cursor.pos = end;
    }

    pub fn backspace(&mut self) {
        let joining = self.cursor.pos.col == 0 && self.cursor.pos.line > 0;
        if joining {
            let split_col = self.buffer.line_len(self.cursor.pos.line - 1).unwrap_or(0);
            let line = self.cursor.pos.line;
            if self.buffer.delete_before(self.cursor.pos).is_none() {
                return;
            }
            self.push_undo(Edit::BackspaceJoin { line, split_col });
            self.cursor.pos = Position::new(line - 1, split_col);
            return;
        }
        if self.cursor.pos.col == 0 {
            return;
        }
        let deleted = self.buffer.line(self.cursor.pos.line).and_then(|l| {
            self.cursor
                .pos
                .col
                .checked_sub(1)
                .and_then(|c| l.chars().nth(c))
        });
        let Some(prev) = self.buffer.delete_before(self.cursor.pos) else {
            return;
        };
        self.push_undo(Edit::DeleteBefore {
            pos: self.cursor.pos,
            ch: deleted.unwrap_or('\0'),
        });
        self.cursor.pos = prev;
    }

    pub fn delete(&mut self) {
        let line_len = self.buffer.line_len(self.cursor.pos.line).unwrap_or(0);
        let joining = self.cursor.pos.col >= line_len;
        let pos = Position::new(self.cursor.pos.line, line_len);
        let Some(after) = self.buffer.delete_at(self.cursor.pos) else {
            return;
        };
        if joining {
            self.push_undo(Edit::JoinLines { pos });
        } else {
            let ch = self.char_at(after);
            self.push_undo(Edit::DeleteAt {
                pos: self.cursor.pos,
                ch,
            });
        }
    }

    pub fn newline(&mut self) {
        let Some(next) = self.buffer.split_line(self.cursor.pos) else {
            return;
        };
        self.push_undo(Edit::SplitLine {
            pos: self.cursor.pos,
        });
        self.cursor.pos = next;
    }

    pub fn join_with_next(&mut self) {
        let at = self.buffer.line_len(self.cursor.pos.line).unwrap_or(0);
        let pos = Position::new(self.cursor.pos.line, at);
        let Some(merged) = self.buffer.delete_at(pos) else {
            return;
        };
        self.push_undo(Edit::JoinLines { pos });
        self.cursor.pos = merged;
    }

    pub fn unindent_current(&mut self) {
        let line = self.cursor.pos.line;
        let spaces = self.buffer.unindent_line(line);
        if spaces > 0 {
            self.push_undo(Edit::Unindent { line, spaces });
            self.cursor.pos.col = self.cursor.pos.col.saturating_sub(spaces);
        }
    }

    pub fn undo(&mut self) -> bool {
        let Some(edit) = self.undo_stack.pop() else {
            return false;
        };
        match edit.clone() {
            Edit::Insert { pos, .. } => {
                self.buffer.delete_before(end_of(pos, 1));
            }
            Edit::BackspaceJoin { line, split_col } => {
                self.buffer.split_line(Position::new(line - 1, split_col));
            }
            Edit::DeleteBefore { pos, ch } => {
                self.buffer.insert_char(pos, ch);
            }
            Edit::DeleteAt { pos, ch } => {
                self.buffer.insert_char(pos, ch);
            }
            Edit::SplitLine { pos } => {
                let end = self.buffer.line_len(pos.line).unwrap_or(0);
                self.buffer.delete_at(Position::new(pos.line, end));
            }
            Edit::JoinLines { pos } => {
                self.buffer.split_line(pos);
            }
            Edit::Unindent { line, spaces } => {
                for _ in 0..spaces {
                    self.buffer.insert_char(Position::new(line, 0), ' ');
                }
            }
        }
        self.redo_stack.push(edit);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(edit) = self.redo_stack.pop() else {
            return false;
        };
        match edit.clone() {
            Edit::Insert { pos, ch } => {
                self.buffer.insert_char(pos, ch);
            }
            Edit::DeleteBefore { pos, .. } => {
                self.buffer.delete_before(pos);
            }
            Edit::DeleteAt { pos, .. } => {
                self.buffer.delete_at(pos);
            }
            Edit::SplitLine { pos } => {
                self.buffer.split_line(pos);
            }
            Edit::BackspaceJoin { line, .. } => {
                self.buffer.delete_before(Position::new(line, 0));
            }
            Edit::JoinLines { pos } => {
                self.buffer.delete_at(pos);
            }
            Edit::Unindent { line, .. } => {
                self.buffer.unindent_line(line);
            }
        }
        self.undo_stack.push(edit);
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    fn push_undo(&mut self, edit: Edit) {
        if self.undo_stack.len() >= MAX_HISTORY {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(edit);
        self.redo_stack.clear();
    }

    fn char_at(&self, pos: Position) -> char {
        self.buffer
            .line(pos.line)
            .and_then(|l| l.chars().nth(pos.col))
            .unwrap_or('\0')
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

fn end_of(pos: Position, n: usize) -> Position {
    Position::new(pos.line, pos.col + n)
}
