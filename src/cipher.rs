//! Enciphered modules (`CipherKey`, `docs/spec/cipher.md`): key preparation and the
//! Sapphire II stream cipher.

/// The alphabet of personalised keys; a character's value is its position.
const ALPHABET: &[u8; 62] = b"bceafgijklhmpqBrHostTuwxyAdCDzEFIJKGLNO7PQMRSUVWXY9012Z364n8v5";

fn value(c: u8) -> usize {
    ALPHABET.iter().position(|&a| a == c).unwrap_or(0)
}

/// The key the cipher is keyed with: a personalised key (`s0-s1-s2-s3-check` with a valid
/// check segment) is decoded to `s1-s2-s3`; anything else is used as written.
pub(crate) fn prepare_key(key: &str) -> Vec<u8> {
    let segments: Vec<&[u8]> = key.as_bytes().split(|&b| b == b'-').take(5).collect();
    if segments.len() < 5 {
        return key.as_bytes().to_vec();
    }
    let first = segments[0];
    let mut check = Vec::with_capacity(4);
    let mut decoded: Vec<Vec<u8>> = Vec::with_capacity(3);
    for (i, segment) in segments[..4].iter().enumerate() {
        let shared = segment.len().min(first.len());
        let sum: usize = segment[..shared].iter().map(|&c| value(c)).sum();
        check.push(ALPHABET[sum % 62]);
        if i >= 1 {
            let mut plain = segment.to_vec();
            for (j, c) in plain.iter_mut().enumerate().take(shared) {
                *c = ALPHABET[(value(*c) + value(first[j])) % 62];
            }
            decoded.push(plain);
        }
    }
    if segments[4] == check.as_slice() {
        decoded.join(&b'-')
    } else {
        key.as_bytes().to_vec()
    }
}

/// Sapphire II state.
struct Sapphire {
    cards: [u8; 256],
    rotor: u8,
    ratchet: u8,
    avalanche: u8,
    last_plain: u8,
    last_cipher: u8,
}

impl Sapphire {
    fn new(key: &[u8]) -> Sapphire {
        let key = &key[..key.len().min(255)];
        if key.is_empty() {
            let mut cards = [0u8; 256];
            for (i, c) in cards.iter_mut().enumerate() {
                *c = 255 - i as u8;
            }
            return Sapphire { cards, rotor: 1, ratchet: 3, avalanche: 5, last_plain: 7, last_cipher: 11 };
        }
        let mut cards = [0u8; 256];
        for (i, c) in cards.iter_mut().enumerate() {
            *c = i as u8;
        }
        let mut schedule = KeySchedule { key, pos: 0, rsum: 0 };
        for i in (0..=255u8).rev() {
            let t = schedule.rand(&cards, i);
            cards.swap(usize::from(i), usize::from(t));
        }
        Sapphire { rotor: cards[1], ratchet: cards[3], avalanche: cards[5], last_plain: cards[7], last_cipher: cards[usize::from(schedule.rsum)], cards }
    }

    fn card(&self, i: u8) -> u8 {
        self.cards[usize::from(i)]
    }

    /// The next keystream byte; the caller then records the plain and cipher bytes.
    fn keystream(&mut self) -> u8 {
        self.ratchet = self.ratchet.wrapping_add(self.card(self.rotor));
        self.rotor = self.rotor.wrapping_add(1);
        let x = self.card(self.last_cipher);
        self.cards[usize::from(self.last_cipher)] = self.card(self.ratchet);
        self.cards[usize::from(self.ratchet)] = self.card(self.last_plain);
        self.cards[usize::from(self.last_plain)] = self.card(self.rotor);
        self.cards[usize::from(self.rotor)] = x;
        self.avalanche = self.avalanche.wrapping_add(self.card(x));
        let a = self.card(self.card(self.ratchet).wrapping_add(self.card(self.rotor)));
        let inner = self.card(self.last_plain).wrapping_add(self.card(self.last_cipher)).wrapping_add(self.card(self.avalanche));
        a ^ self.card(self.card(inner))
    }

    fn decipher(&mut self, b: u8) -> u8 {
        let p = b ^ self.keystream();
        self.last_plain = p;
        self.last_cipher = b;
        p
    }

    #[cfg(test)]
    fn encipher(&mut self, p: u8) -> u8 {
        let c = p ^ self.keystream();
        self.last_plain = p;
        self.last_cipher = c;
        c
    }
}

/// The key schedule's pseudo-random source.
struct KeySchedule<'a> {
    key: &'a [u8],
    pos: usize,
    rsum: u8,
}

impl KeySchedule<'_> {
    fn rand(&mut self, cards: &[u8; 256], limit: u8) -> u8 {
        if limit == 0 {
            return 0;
        }
        let mut mask: u8 = 1;
        while mask < limit {
            mask = (mask << 1) | 1;
        }
        let mut attempts = 0;
        loop {
            self.rsum = cards[usize::from(self.rsum)].wrapping_add(self.key[self.pos]);
            self.pos += 1;
            if self.pos >= self.key.len() {
                self.pos = 0;
                // At most 255, so the length fits a byte.
                self.rsum = self.rsum.wrapping_add(self.key.len() as u8);
            }
            let mut u = self.rsum & mask;
            attempts += 1;
            if attempts > 11 {
                u %= limit;
            }
            if u <= limit {
                return u;
            }
        }
    }
}

/// Decipher one block or entry; the cipher starts afresh from the key each time.
pub(crate) fn decipher(key: &[u8], data: &mut [u8]) {
    let mut s = Sapphire::new(key);
    for b in data.iter_mut() {
        *b = s.decipher(*b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn personalised(segments: [&str; 4]) -> String {
        let first = segments[0].as_bytes();
        let check: String = segments
            .iter()
            .map(|s| {
                let n = s.len().min(first.len());
                char::from(ALPHABET[s.as_bytes()[..n].iter().map(|&c| value(c)).sum::<usize>() % 62])
            })
            .collect();
        format!("{}-{check}", segments.join("-"))
    }

    #[test]
    fn plain_keys_are_used_as_written() {
        assert_eq!(prepare_key("s3cr3t-Key"), b"s3cr3t-Key");
        assert_eq!(prepare_key("a-b-c-d-wrong"), b"a-b-c-d-wrong");
    }

    #[test]
    fn personalised_keys_are_decoded() {
        let key = personalised(["bce", "bcea", "ccc", "Z"]);
        // s0 = "bce" has values 0, 1, 2: in s1 "bcea", b+b, c+c and e+e give b, e, f; "a" is kept.
        let decoded = String::from_utf8(prepare_key(&key)).unwrap();
        assert_eq!(decoded, "befa-cea-Z");
        // Text after a fifth `-` is ignored.
        assert_eq!(prepare_key(&format!("{key}-extra")), decoded.as_bytes());
    }

    #[test]
    fn round_trip() {
        for key in [&b"s3cr3t-Key"[..], b"", b"k", &[7u8; 300][..]] {
            let plain = b"In the beginning God created the heaven and the earth.".to_vec();
            let mut enc = Sapphire::new(key);
            let mut data: Vec<u8> = plain.iter().map(|&p| enc.encipher(p)).collect();
            assert_ne!(data, plain);
            decipher(key, &mut data);
            assert_eq!(data, plain);
        }
    }
}
