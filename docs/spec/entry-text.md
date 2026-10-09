# Spec: entry text normalisation

**Applies to:** text read from Bible, commentary, dictionary and general-book
entries (not personal-notes files). Applied after deciphering, before decoding to
Unicode. Operates on bytes.

**Source of this spec:** observed behaviour of libsword 1.9.0 (entries written
with `setEntry` and read back with `getRawEntry`, plus 200,000 randomised
differential cases; see `tools/differential/`).

## Behaviour

1. Input ends at the first NUL byte, if any.
2. CR (0x0D) and LF (0x0A) bytes that occur before the first byte of any other
   value are discarded.
3. Afterwards, scanning left to right, with two flags *pending-space* and
   *cr-seen* (both initially off) and a counter *newline-run* (initially 0):
   * **CR:** emit LF; set *cr-seen*; clear *pending-space*. (Does not change
     *newline-run*.)
   * **LF:** increment *newline-run*; set *pending-space* to the negation of
     *cr-seen*; then clear *cr-seen*. If *newline-run* is now 2 or more, emit LF.
   * **Any other byte:** reset *newline-run* to 0. If *pending-space* is set,
     clear it and, unless this byte is a space (0x20), emit a space. Then emit
     the byte.
     (*cr-seen* is **not** cleared by other bytes.)
4. Finally, while the output is longer than one byte and ends with LF or space,
   remove the last byte.

## Examples (observed)

| input | output |
|---|---|
| `\r\nHello\r\nworld··\n\n` | `Hello\nworld` |
| `a\nb` | `a b` |
| `a\n\nb` | `a\n b` |
| `a\n\n\nb` | `a\n\n b` |
| `a\r\n\r\nb` | `a\n\n\nb` |
| `\rA\nB` | `A B` (leading CR discarded, so it does not suppress the space) |
| `a·\n·b` | `a··b` |
| `··a··` | `··a` |
| `a\0b` | `a` |
| `\n` | (empty) |
| `·` | `·` (single byte is never trimmed) |

(`·` = space)
