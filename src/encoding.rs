//! Character encodings used by SWORD module text (`Encoding=` in the conf).

/// The encoding of a module's entry text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextEncoding {
    Utf8,
    /// `Latin-1` (SWORD's default). Like libsword's `Latin1UTF8` filter, bytes
    /// 0x80–0x9F are interpreted as Windows-1252.
    Latin1,
    /// `UTF-16` (little-endian, as written by SWORD on all common platforms).
    Utf16,
    /// `SCSU` — rare; decoded as Latin-1 as a best effort.
    Scsu,
}

impl TextEncoding {
    pub fn parse(s: &str) -> TextEncoding {
        let name: String = s.trim().chars().filter(|c| *c != '-' && *c != '_').collect::<String>().to_ascii_uppercase();
        match name.as_str() {
            "UTF8" => TextEncoding::Utf8,
            "UTF16" | "UTF16LE" => TextEncoding::Utf16,
            "SCSU" => TextEncoding::Scsu,
            _ => TextEncoding::Latin1,
        }
    }
}

/// Windows-1252's assignments for 0x80–0x9F, from the published code page. The five
/// unassigned positions (0x81, 0x8D, 0x8F, 0x90, 0x9D) keep their Latin-1 control code.
const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

/// One byte of Windows-1252 text.
pub(crate) fn cp1252_char(b: u8) -> char {
    match b {
        0x80..=0x9F => CP1252_HIGH[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}

/// Decode Windows-1252 bytes.
pub(crate) fn decode_cp1252(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| cp1252_char(b)).collect()
}

/// Decode UTF-16 (little-endian unless a big-endian byte-order mark says otherwise).
fn decode_utf16(bytes: &[u8]) -> String {
    let big_endian = bytes.starts_with(&[0xFE, 0xFF]);
    let body = if bytes.starts_with(&[0xFF, 0xFE]) || big_endian { &bytes[2..] } else { bytes };
    let units = body.chunks_exact(2).map(|p| if big_endian { u16::from_be_bytes([p[0], p[1]]) } else { u16::from_le_bytes([p[0], p[1]]) });
    char::decode_utf16(units).map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
}

/// Decode raw entry bytes to a `String`.
///
/// Invalid UTF-8 in a UTF-8 module is replaced with U+FFFD rather than failing,
/// since a handful of published modules contain stray bytes.
pub fn decode_text(bytes: &[u8], enc: TextEncoding) -> String {
    match enc {
        TextEncoding::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        TextEncoding::Latin1 | TextEncoding::Scsu => decode_cp1252(bytes),
        TextEncoding::Utf16 => decode_utf16(bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodings() {
        assert_eq!(TextEncoding::parse("UTF-8"), TextEncoding::Utf8);
        assert_eq!(TextEncoding::parse("utf8"), TextEncoding::Utf8);
        assert_eq!(TextEncoding::parse("UTF-16"), TextEncoding::Utf16);
        assert_eq!(TextEncoding::parse("Latin-1"), TextEncoding::Latin1);
        assert_eq!(TextEncoding::parse("SCSU"), TextEncoding::Scsu);
        assert_eq!(decode_text(&[0x93, b'a', 0x94, 0xE9, 0x81], TextEncoding::Latin1), "\u{201C}a\u{201D}é\u{81}");
        assert_eq!(decode_text(&[b'a', 0, 0x3A, 0x26], TextEncoding::Utf16), "a\u{263A}");
        assert_eq!(decode_text(b"ok\xFF", TextEncoding::Utf8), "ok\u{FFFD}");
    }
}
