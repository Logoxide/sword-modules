# Spec: enciphered modules (`CipherKey`)

**Sources of this spec:** the Sapphire II stream cipher, published and placed in
the public domain by Michael Paul Johnson (1995); SWORD's key handling as
observed by enciphering test modules with libsword's `osis2mod -c` and deciphering
them (`tools/differential/`).

## Where the cipher applies

* zText/zCom/zLD: each compressed block is deciphered before decompression.
* RawText/RawCom/RawLD/RawGenBook: each entry's bytes are deciphered after
  reading. (libsword's `osis2mod` only enciphers compressed modules.)
* The cipher is re-initialised from the key for every block / entry.

## Key preparation ("personalised" keys)

A key is split on `-` into at most five segments (`s0-s1-s2-s3-check`; text after
a fifth `-` is ignored). Characters map to values through this 62-character
alphabet (position = value):

    bceafgijklhmpqBrHostTuwxyAdCDzEFIJKGLNO7PQMRSUVWXY9012Z364n8v5

Characters not in the alphabet have value 0.

For each segment *i* in 0–3, consider positions *j* from 0 up to the shorter of
`len(s_i)` and `len(s0)`:

* the *checksum digit* *c_i* is `ALPHABET[(Σ_j value(s_i[j])) mod 62]`;
* for *i* ≥ 1, the *decoded character* at *j* is
  `ALPHABET[(value(s_i[j]) + value(s0[j])) mod 62]`; characters beyond that
  range are kept unchanged.

If the fifth segment equals the four checksum digits `c_0 c_1 c_2 c_3` exactly,
the key used is the decoded `s1-s2-s3` (joined with `-`). Otherwise the key is
used exactly as written.

## Sapphire II

State: a permutation `cards[0..256]` and bytes `rotor`, `ratchet`, `avalanche`,
`last_plain`, `last_cipher`. All arithmetic is modulo 256.

**Key schedule** (key of 1–255 bytes; longer keys use the first 255):

* `cards[i] = i` for all *i*; `rsum = 0`; `keypos = 0`.
* For *i* from 255 down to 0: pick `t = keyrand(i)` and swap `cards[i]` and
  `cards[t]`, where `keyrand(limit)`:
  * returns 0 if `limit` is 0;
  * otherwise let `mask` be the smallest value of the form 2ᵏ−1 that is
    ≥ `limit`; repeat: `rsum = cards[rsum] + key[keypos]`; advance `keypos`,
    and when it reaches the key length reset it to 0 and add the key length to
    `rsum`; `u = rsum & mask`; after more than 11 attempts use `u mod limit`;
    until `u ≤ limit`. Return `u`.
* Then `rotor = cards[1]`, `ratchet = cards[3]`, `avalanche = cards[5]`,
  `last_plain = cards[7]`, `last_cipher = cards[rsum]`.

An empty key instead sets `rotor=1, ratchet=3, avalanche=5, last_plain=7,
last_cipher=11` and `cards[i] = 255 − i`.

**Per byte** (decipher input byte *b*):

1. `ratchet += cards[rotor]`; `rotor += 1`.
2. Rotate four cards: let `x = cards[last_cipher]`; then
   `cards[last_cipher] = cards[ratchet]`, `cards[ratchet] = cards[last_plain]`,
   `cards[last_plain] = cards[rotor]`, `cards[rotor] = x`.
3. `avalanche += cards[x]`.
4. keystream `k = cards[cards[ratchet] + cards[rotor]] ^
   cards[cards[cards[last_plain] + cards[last_cipher] + cards[avalanche]]]`.
5. plaintext `p = b ^ k`; then `last_plain = p`, `last_cipher = b`.

(Enciphering is the same with the roles swapped: `c = p ^ k`,
`last_plain = p`, `last_cipher = c`.)
