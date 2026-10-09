//! LZSS block decompression (`CompressType=LZSS`, `docs/spec/lzss.md`).

const RING: usize = 4096;
const START: usize = RING - 18;

/// The decoder's history: a ring of the last 4096 bytes written.
struct History {
    ring: [u8; RING],
    pos: usize,
}

impl History {
    fn new() -> History {
        let mut ring = [b' '; RING];
        ring[START..].fill(0);
        History { ring, pos: START }
    }

    fn emit(&mut self, b: u8, out: &mut Vec<u8>) {
        out.push(b);
        self.ring[self.pos] = b;
        self.pos = (self.pos + 1) % RING;
    }
}

/// Decode an LZSS stream; a truncated final item is ignored.
pub(crate) fn decode(data: &[u8]) -> Vec<u8> {
    let mut history = History::new();
    let mut out = Vec::with_capacity(data.len() * 2);
    let mut input = data;
    while let Some((&flags, rest)) = input.split_first() {
        input = rest;
        for bit in 0..8 {
            if flags & (1 << bit) != 0 {
                let Some((&b, rest)) = input.split_first() else { return out };
                input = rest;
                history.emit(b, &mut out);
            } else {
                let [b0, b1, rest @ ..] = input else { return out };
                input = rest;
                let start = usize::from(*b0) | (usize::from(*b1 & 0xF0) << 4);
                let len = usize::from(*b1 & 0x0F) + 3;
                for k in 0..len {
                    let b = history.ring[(start + k) % RING];
                    history.emit(b, &mut out);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn literals_and_references() {
        // flags 0b0000_0111: three literals, then a reference to the first of them
        // (position 4078, length 3), which also overlaps into its own output.
        let data = [0b0000_0111, b'a', b'b', b'c', 0xEE, 0xF3];
        assert_eq!(decode(&data), b"abcabcabc");
        // A reference into the initial spaces.
        assert_eq!(decode(&[0b0000_0000, 0x00, 0x00]), b"   ");
        // Truncated items are ignored.
        assert_eq!(decode(&[0b0000_0001, b'x', 0x00]), b"x");
        assert_eq!(decode(&[]), b"");
    }
}
