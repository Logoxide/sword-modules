# Spec: LZSS block compression (`CompressType=LZSS`)

**Source of this spec:** the classic public-domain LZSS scheme (Haruhiko
Okumura, 1989) with the parameters SWORD uses; verified by decoding data that
libsword's own `LZSSCompress` encoder produced (`tools/differential/`).

## Format

The decoder keeps a 4096-byte ring buffer. Before decoding, bytes 0–4077 hold
spaces (0x20) and bytes 4078–4095 hold zeros; the write position starts at 4078.

The stream is a sequence of groups. Each group starts with a flag byte whose bits
are consumed least-significant first, one per item. For each item:

* flag bit **1** → one literal byte follows: output it.
* flag bit **0** → two bytes `b0 b1` follow: a back-reference with
  position `b0 | (b1 & 0xF0) << 4` (12 bits) and length `(b1 & 0x0F) + 3`
  (3–18 bytes). Copy that many bytes from the ring buffer starting at that
  position (wrapping at 4096), one at a time.

Every byte output (literal or copied) is also written at the write position,
which then advances (wrapping at 4096); copying byte by byte means a reference
may overlap the bytes it is producing.

Decoding stops when the input ends at an item boundary (a truncated item is
ignored).
