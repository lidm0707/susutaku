use crate::buffer::Position;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub start: Position,
    pub end: Position,
}

/// Find all occurrences of `needle` in the buffer text.
pub fn find_all(lines: &[String], needle: &str) -> Vec<SearchMatch> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    for (li, line) in lines.iter().enumerate() {
        let mut byte = 0;
        while let Some(found) = line[byte..].find(needle) {
            let start = byte + found;
            let end = start + needle.len();
            out.push(SearchMatch {
                start: Position::new(li, byte_to_col(line, start)),
                end: Position::new(li, byte_to_col(line, end)),
            });
            byte = end;
        }
    }
    out
}

fn byte_to_col(line: &str, byte: usize) -> usize {
    line[..byte].chars().count()
}
