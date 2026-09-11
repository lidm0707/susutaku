//! Byte-level GPT-2 unicode map: each raw byte → one printable char.
//!
//! Same table HF byte-level BPE uses; katgpt vocab strings are built on it.

use std::collections::HashMap;
use std::sync::LazyLock;

pub const SPACE_CHAR: char = ' ';
pub const SPACE_BYTE: u8 = b' ';

fn build_byte_to_unicode() -> [char; 256] {
    let mut table = ['\0'; 256];
    let mut extra: u32 = 0;
    for (byte, slot) in table.iter_mut().enumerate() {
        let b = byte as u32;
        *slot = if (33..=126).contains(&b) || (161..=172).contains(&b) || (174..=255).contains(&b) {
            char::from_u32(b).expect("printable byte is a char")
        } else {
            let c = char::from_u32(256 + extra).expect("mapped codepoint is a char");
            extra += 1;
            c
        };
    }
    table
}

pub static BYTE_TO_UNICODE: LazyLock<[char; 256]> = LazyLock::new(build_byte_to_unicode);

pub static UNICODE_TO_BYTE: LazyLock<HashMap<char, u8>> = LazyLock::new(|| {
    BYTE_TO_UNICODE
        .iter()
        .enumerate()
        .map(|(byte, ch)| (*ch, byte as u8))
        .collect()
});
