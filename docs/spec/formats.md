# Spec: module storage formats

**Sources:** the CrossWire wiki's module file-format documentation, inspection of
module files (hex dumps of published modules and of modules produced by
libsword's `osis2mod`, `imp2ld`, `imp2gbs`), and black-box comparison with
`mod2imp`. All integers are little-endian.

## Verse-keyed (Bibles, commentaries)

`DataPath` is a directory. Per testament (`ot`, `nt`; either may be absent):

* **RawText / RawCom:** `ot.vss` has 6-byte entries (`u32 offset, u16 length`)
  per index slot into the text file `ot`. **RawText4 / RawCom4:** 8-byte entries
  (`u32 offset, u32 length`).
* **zText / zCom:** with `X` = `b`, `c`, `v` for `BlockType` BOOK, CHAPTER, VERSE:
  `ot.Xzv` has 10-byte entries (`u32 block, u32 offset in block, u16 length`);
  `ot.Xzs` has 12-byte entries per block (`u32 offset in ot.Xzz, u32 stored
  length, u32 unpacked length`); `ot.Xzz` holds the blocks.
  **zText4 / zCom4:** 12-byte `.Xzv` entries (`u32, u32, u32`).
* A zero length means "no text". xz blocks may be followed by zero padding.
* **RawFiles (personal notes):** RawText layout whose entries are file names of
  files in the module directory that hold each note's text (unnormalised).
  Versification is always KJV.

## String-keyed (dictionaries)

`DataPath` is a file prefix.

* `.idx`: sorted entries (`u32 offset into .dat, u16 length` for RawLD;
  `u32, u32` for RawLD4 and zLD).
* `.dat` record: the key, terminated by CR, LF or `\`; then the rest of the line;
  then the body. A body beginning `@LINK ` names another key whose entry should be
  used instead.
* **zLD:** the body is `u32 block, u32 entry`. `.zdx` has 8-byte entries per
  block (`u32 offset in .zdt, u32 length`). An unpacked block begins with
  `u32 count`, then `count` pairs (`u32 offset, u32 length`) relative to the block
  start; entry text is NUL-terminated.

## Tree-keyed (general books)

`DataPath` is a file prefix. A node is identified by a byte offset into `.idx`
(root = 0); `.idx` holds a `u32` offset into `.dat` per node. A `.dat` node is
`i32 parent, i32 next sibling, i32 first child` (each an `.idx` offset, −1 for
none), NUL-terminated name, `u16` user-data length, user data. For general books
the user data is `u32 offset, u32 length` into `.bdt`.
