use crate::buffer::Position;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    pub pos: Position,
}

impl Cursor {
    pub fn at(line: usize, col: usize) -> Self {
        Self {
            pos: Position::new(line, col),
        }
    }

    pub fn left(&mut self, line_len: usize) {
        if self.pos.col > 0 {
            self.pos.col -= 1;
        } else if self.pos.line > 0 {
            self.pos.line -= 1;
            self.pos.col = line_len;
        }
    }

    pub fn right(&mut self, line_len: usize) {
        if self.pos.col < line_len {
            self.pos.col += 1;
        } else {
            self.pos.line += 1;
            self.pos.col = 0;
        }
    }

    pub fn up(&mut self, target_len: usize) {
        if self.pos.line > 0 {
            self.pos.line -= 1;
            self.pos.col = self.pos.col.min(target_len);
        }
    }

    pub fn down(&mut self, target_len: usize) {
        self.pos.line += 1;
        self.pos.col = self.pos.col.min(target_len);
    }

    pub fn line_start(&mut self) {
        self.pos.col = 0;
    }

    pub fn line_end(&mut self, line_len: usize) {
        self.pos.col = line_len;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: Position,
    pub head: Position,
}

impl Selection {
    pub fn new(anchor: Position) -> Self {
        Self {
            anchor,
            head: anchor,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn range(&self) -> (Position, Position) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}
