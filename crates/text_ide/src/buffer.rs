use std::fmt;

pub const TAB_WIDTH: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

impl Position {
    pub const fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line + 1, self.col + 1)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Buffer {
    lines: Vec<String>,
}

impl Buffer {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
        }
    }

    pub fn from_text(text: &str) -> Self {
        let mut b = Self::new();
        b.lines = text.split('\n').map(str::to_string).collect();
        if b.lines.is_empty() {
            b.lines.push(String::new());
        }
        b
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line(&self, idx: usize) -> Option<&str> {
        self.lines.get(idx).map(String::as_str)
    }

    pub fn line_len(&self, idx: usize) -> Option<usize> {
        self.lines.get(idx).map(String::len)
    }

    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty()
    }

    pub fn char_count(&self) -> usize {
        self.lines.iter().map(|l| l.chars().count()).sum::<usize>()
            + self.line_count().saturating_sub(1)
    }

    /// Insert a char at position; returns adjusted position (end of inserted char).
    pub fn insert_char(&mut self, pos: Position, c: char) -> Option<Position> {
        let line = self.lines.get_mut(pos.line)?;
        let col = pos.col.min(line.chars().count());
        let byte = char_to_byte(line, col);
        line.insert(byte, c);
        Some(Position::new(pos.line, col + 1))
    }

    /// Delete the char before position (backspace); returns new position.
    pub fn delete_before(&mut self, pos: Position) -> Option<Position> {
        if pos.line >= self.lines.len() {
            return None;
        }
        if pos.col == 0 {
            if pos.line == 0 {
                return None;
            }
            let prev_len = self.lines[pos.line - 1].chars().count();
            let cur = self.lines.remove(pos.line);
            self.lines[pos.line - 1].push_str(&cur);
            return Some(Position::new(pos.line - 1, prev_len));
        }
        let line = &mut self.lines[pos.line];
        let col = pos.col.min(line.chars().count());
        let byte = char_to_byte(line, col);
        let start = prev_char_boundary(line, byte);
        line.replace_range(start..byte, "");
        Some(Position::new(pos.line, col - 1))
    }

    /// Delete the char at position (delete key); returns same position.
    pub fn delete_at(&mut self, pos: Position) -> Option<Position> {
        if pos.line >= self.lines.len() {
            return None;
        }
        let line_len = self.lines[pos.line].chars().count();
        if pos.col >= line_len {
            if pos.line + 1 >= self.lines.len() {
                return None;
            }
            let next = self.lines.remove(pos.line + 1);
            self.lines[pos.line].push_str(&next);
            return Some(Position::new(pos.line, line_len));
        }
        let line = &mut self.lines[pos.line];
        let byte = char_to_byte(line, pos.col);
        let end = next_char_boundary(line, byte);
        line.replace_range(byte..end, "");
        Some(Position::new(pos.line, pos.col))
    }

    /// Split line at position (Enter key).
    pub fn split_line(&mut self, pos: Position) -> Option<Position> {
        let line = self.lines.get_mut(pos.line)?;
        let col = pos.col.min(line.chars().count());
        let byte = char_to_byte(line, col);
        let tail = line.split_off(byte);
        self.lines.insert(pos.line + 1, tail);
        Some(Position::new(pos.line + 1, 0))
    }

    /// Remove indentation of a line; returns the number of spaces removed.
    pub fn unindent_line(&mut self, idx: usize) -> usize {
        let Some(line) = self.lines.get_mut(idx) else {
            return 0;
        };
        let mut removed = 0;
        for _ in 0..TAB_WIDTH {
            if line.starts_with(' ') {
                line.remove(0);
                removed += 1;
            } else {
                break;
            }
        }
        removed
    }
}

fn char_to_byte(line: &str, char_col: usize) -> usize {
    line.char_indices()
        .nth(char_col)
        .map(|(b, _)| b)
        .unwrap_or(line.len())
}

fn prev_char_boundary(line: &str, byte: usize) -> usize {
    line[..byte]
        .char_indices()
        .next_back()
        .map(|(b, _)| b)
        .unwrap_or(byte)
}

fn next_char_boundary(line: &str, byte: usize) -> usize {
    line[byte..]
        .chars()
        .next()
        .map(|c| byte + c.len_utf8())
        .unwrap_or(byte)
}
