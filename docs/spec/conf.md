# Spec: `.conf` files

**Source of this spec:** observed behaviour (conf files read through libsword's
module manager and `getConfigEntry`, including randomised layouts in
`tools/differential/`) and the conf format documentation on the CrossWire wiki.
The continuation and key rules were corrected after differential testing showed
the first version of this spec was incomplete.

* Text is UTF-8 (a leading byte-order mark is ignored), or Latin-1/CP1252 if not
  valid UTF-8.
* **Logical lines.** Physical lines are separated by LF. A logical line starts
  with a physical line minus its leading whitespace. Then, repeatedly: remove
  trailing whitespace (including CR); if the text now ends with `\` **and that
  backslash is not the first character of the physical line it came from**
  (after the leading-whitespace removal, for the first physical line), remove the
  backslash and, if another physical line follows, append it **verbatim**
  (leading whitespace included) and repeat. A backslash that is the first
  character of its physical line is ordinary text. A final backslash with no
  following line is still removed.
* A logical line starting with `#` is a comment; an empty one is ignored.
  (Comment detection happens after joining, so `A=1\` followed by `#B=2` gives
  `A` = `1#B=2`.)
* `[Name]` starts a section (the name is everything up to the first `]`).
* Otherwise the line is split at the first `=`:
  * if the line starts with `=`, it is ignored;
  * the **key** is the text before `=`, kept as is (trailing spaces included:
    `Key = v` has the key `"Key "`);
  * the **value** is the text after `=`, trimmed of whitespace on both sides;
  * a line with no `=` is an entry whose key is the whole line and whose value
    is empty.
* Keys may repeat; all values are kept in order.
* Defaults when absent: `SourceType` → GBF, `Encoding` → Latin-1,
  `CompressType` → LZSS, `BlockType` → CHAPTER, `Versification` → KJV,
  `CaseSensitiveKeys` → false, `StrongsPadding` → true.
* `About` and similar values use a small RTF subset: `\par` and `\line` are line
  breaks, `\tab` a tab, `\uN?` the Unicode character N (the `?` placeholder is
  skipped), `\'hh` a Latin-1 byte; other control words and braces are dropped.
