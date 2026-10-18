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
* **A testament with an index but no data.** If a testament's index file exists
  but its data file (`ot`, or `ot.Xzz`) or, for compressed modules, its block
  index (`ot.Xzs`) does not, the module still opens and every entry of that
  testament reads as empty, whatever its index entry says; the other testament
  reads normally. (Observed with libsword 1.9.0 on osis2mod-made zText and
  RawText modules with those files deleted, read through the Python bindings'
  `getRawEntry` and with `mod2imp`; for non-zero index entries libsword also
  prints an error. Published examples: CrossWire's `TR` has an all-zero `ot.bzv`
  and no `ot.bzs`/`ot.bzz`; `WLC` has a one-entry, zero `nt.bzv` and no
  `nt.bzs`/`nt.bzz`.)
* **Linked entries.** Slots whose index entries are identical (same offset and
  length; for compressed modules the same block, offset and length) share one
  stored entry, typically a commentary note on a range of verses. Each such slot
  reads as that entry's text, and libsword's `isLinked` reports the slots as
  linked. Equal text in separate stored entries is not a link.
  **How `mod2imp` lists them** (observed, libsword 1.9.0, on modules written
  with `setEntry`/`linkEntry` through the Python bindings and on CrossWire's
  `MHC`): for a commentary it lists only slots with text, and leaves out a slot
  whose index entry is identical to the slot **immediately before** it; a slot
  sharing an entry with an earlier, non-adjacent slot is listed. For a Bible it
  lists every slot, linked or empty. So `MHC` (`zCom4`, 32,355 slots with text)
  is dumped as 5,506 entries.
  **How writers lay out links** (observed, libsword 1.9.0's `osis2mod` and `imp2vs`,
  RawCom and `osis2mod -z z -b 3`, index entries read with a hex dump and slot
  numbers from the Python bindings' `getTestamentIndex`): a range's later slots get
  index entries identical to its first slot's, so the shared entry is stored once,
  where the first slot's text went (for a compressed module, in the first slot's
  block, even when a later slot is in another chapter or book). `osis2mod` links
  only the verses an `osisID` names: for `John.3.36 John.4.1 John.4.2` the chapter
  introduction John 4:0 between them stays empty, and for `John.21.25 Acts.1.1` the
  introductions between them (Acts 0:0 and 1:0) stay empty. `imp2vs` given the
  range `$$$John 6:71-7:1` links every slot of it, John 7:0 included.
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
