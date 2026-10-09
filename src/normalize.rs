//! Entry text normalisation (`docs/spec/entry-text.md`) and Strong's key padding
//! (`docs/spec/strongs-keys.md`).

const CR: u8 = b'\r';
const LF: u8 = b'\n';

/// Normalise raw entry bytes as SWORD applications see them: line breaks are
/// folded (a single line break becomes a space, further ones become newlines),
/// leading line breaks and trailing spaces/newlines are removed, and the text ends
/// at the first NUL. See `docs/spec/entry-text.md` for the exact rules.
pub fn normalize_entry(input: &[u8]) -> Vec<u8> {
    let text = match input.iter().position(|&b| b == 0) {
        Some(nul) => &input[..nul],
        None => input,
    };
    let start = text.iter().position(|&b| b != CR && b != LF).unwrap_or(text.len());
    let mut out = Vec::with_capacity(text.len() - start);
    let mut pending_space = false;
    let mut cr_seen = false;
    let mut newline_run = 0u32;
    for &b in &text[start..] {
        match b {
            CR => {
                out.push(LF);
                cr_seen = true;
                pending_space = false;
            }
            LF => {
                newline_run += 1;
                pending_space = !cr_seen;
                cr_seen = false;
                if newline_run >= 2 {
                    out.push(LF);
                }
            }
            other => {
                newline_run = 0;
                if pending_space {
                    pending_space = false;
                    if other != b' ' {
                        out.push(b' ');
                    }
                }
                out.push(other);
            }
        }
    }
    while out.len() > 1 && matches!(out.last(), Some(&LF) | Some(&b' ')) {
        out.pop();
    }
    out
}

/// Zero-pad Strong's-number dictionary keys the way SWORD stores them:
/// `"25"` → `"00025"`, `"G25"` → `"G0025"`, `"h430a"` → `"h0430A"`,
/// `"3056!b"` → `"03056!B"`. Keys that do not look like Strong's numbers are
/// returned unchanged. See `docs/spec/strongs-keys.md`.
pub fn strongs_pad(key: &str) -> String {
    let bytes = key.as_bytes();
    if bytes.is_empty() || bytes.len() > 8 {
        return key.to_string();
    }
    let prefix_len = usize::from(matches!(bytes[0], b'G' | b'g' | b'H' | b'h'));
    let digits = bytes[prefix_len..].iter().take_while(|b| b.is_ascii_digit()).count();
    let rest = &bytes[prefix_len + digits..];
    if digits == 0 || rest.len() > 2 {
        return key.to_string();
    }
    // At most 8 ASCII digits, so the value always fits.
    let value: u64 = bytes[prefix_len..prefix_len + digits].iter().fold(0, |n, d| n * 10 + u64::from(d - b'0'));
    let mut out = String::with_capacity(10);
    out.push_str(&key[..prefix_len]);
    if prefix_len == 1 {
        out.push_str(&format!("{value:04}"));
    } else {
        out.push_str(&format!("{value:05}"));
    }
    match rest {
        [letter, ..] if letter.is_ascii_alphabetic() => out.push(char::from(letter.to_ascii_uppercase())),
        [b'!', letter] if letter.is_ascii_alphabetic() => {
            out.push('!');
            out.push(char::from(letter.to_ascii_uppercase()));
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(s: &str) -> String {
        String::from_utf8(normalize_entry(s.as_bytes())).unwrap()
    }

    #[test]
    fn normalisation_examples() {
        assert_eq!(norm("\r\nHello\r\nworld  \n\n"), "Hello\nworld");
        assert_eq!(norm("a\nb"), "a b");
        assert_eq!(norm("a\n\nb"), "a\n b");
        assert_eq!(norm("a\n\n\nb"), "a\n\n b");
        assert_eq!(norm("a\r\n\r\nb"), "a\n\n\nb");
        assert_eq!(norm("\rA\nB"), "A B");
        assert_eq!(norm("a \n b"), "a  b");
        assert_eq!(norm("  a  "), "  a");
        assert_eq!(norm("a\0b"), "a");
        assert_eq!(norm("\n"), "");
        assert_eq!(norm(" "), " ");
    }

    #[test]
    fn padding_examples() {
        for (key, stored) in [
            ("25", "00025"),
            ("G25", "G0025"),
            ("h430a", "h0430A"),
            ("3056!b", "03056!B"),
            ("123.", "00123"),
            ("25ab", "00025A"),
            ("12a3", "00012A"),
            ("1-2", "00001"),
            ("1!", "00001"),
            ("H0", "H0000"),
            ("12345678", "12345678"),
            ("123456789", "123456789"),
            ("+5", "+5"),
            ("G", "G"),
            ("AARON", "AARON"),
            ("", ""),
            ("1é", "00001"),
        ] {
            assert_eq!(strongs_pad(key), stored, "{key}");
        }
    }
}
