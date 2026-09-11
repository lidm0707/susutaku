use std::io::{BufRead, Error, ErrorKind, Write};

pub const HEADER_LEN: &str = "Content-Length:";
pub const ERR_CLOSED: &str = "lsp: server stream closed";
pub const ERR_NO_LEN: &str = "lsp: missing Content-Length header";

/// Read one framed LSP message body into `body` (cleared first, zero copy on reuse).
pub fn read_message(r: &mut impl BufRead, body: &mut Vec<u8>) -> Result<(), Error> {
    body.clear();
    let mut len = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 {
            return Err(Error::new(ErrorKind::UnexpectedEof, ERR_CLOSED));
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix(HEADER_LEN) {
            len = v.trim().parse::<usize>().ok();
        }
    }
    let len = len.ok_or_else(|| Error::new(ErrorKind::InvalidData, ERR_NO_LEN))?;
    body.resize(len, 0);
    r.read_exact(body)
}

/// Write one framed LSP message.
pub fn write_message(w: &mut impl Write, body: &[u8]) -> Result<(), Error> {
    write!(w, "{HEADER_LEN} {}\r\n\r\n", body.len())?;
    w.write_all(body)?;
    w.flush()
}
