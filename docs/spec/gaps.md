# Spec gaps found during the clean-room rewrite

The rewrite was done from the other files in `docs/spec/`, `README.md` and the
fixtures in `tests/fixtures.tar.gz`. Where those left a question open, the answer
below was worked out from the fixtures (hex dumps of the module files and the
`mod2imp` dumps in `ref/`), from public format documentation, or, where neither
says anything, chosen as a reasonable behaviour and marked **choice**. Nothing
here comes from libsword's code.

## Versification data

* **MT and Leningrad have no New Testament.** In `data/versification.json` their
  `nt` list repeats the `ot` list book for book (the extraction tool asked a
  system without a New Testament for one). A system whose New-Testament list equals
  its Old-Testament list is read as Old-Testament-only: its New-Testament index
  holds only the two heading slots.
* **Index sizes.** Every verse-keyed fixture's index has exactly
  `slot_count(testament)` entries as `versification.md` defines it (KJVA: 30,028 Old
  and 8,246 New; Synodal: 30,299 and 8,244; KJV personal notes: 24,115 and 8,246).

## Verse-keyed modules

* **Width of index entries when the conf is wrong** (from `README.md`): if the index
  length is not `slots × declared width` but is `slots × the other width`
  (6↔8 for raw, 10↔12 for compressed), the other width is used.
* **The block index's unpacked size** (third field of `.Xzs`) is not needed:
  entries are sliced from the decompressed block by offset and length. A slice
  outside the block is an error (`Corrupt`).
* **XZ padding.** Observed in `TXz`: the stored size of a block covers the xz
  stream plus zero bytes. Trailing zero bytes are removed before decoding (an xz
  stream's footer ends in `YZ`, so this never cuts the stream).
* **BZIP2 and ZIP** blocks are a single bzip2 stream and a zlib stream (`78 9C`
  header) respectively (observed).
* **RawFiles.** Observed in `Personal`: the data file `ot`/`nt` holds 7-digit file
  names, each followed by LF; index lengths cover the name only; a deleted note has
  a zero index entry. The note file is read as is (decoded, not normalised); an
  empty or missing file is no entry. **Choice:** names must be plain (letters,
  digits, `-`, `_`, `.`, not starting with `.`), so an index entry cannot point
  outside the module directory.

## Dictionaries

* **`.dat` records** (observed in all `L*` fixtures): `KEY CR LF body`; the index
  length covers the key line and body but not the LF (RawLD) or CR LF (zLD) that
  separates records. The body starts after the first LF following the key.
* **zLD bodies** are exactly 8 bytes (`u32 block, u32 entry`). `@LINK` aliases are in
  the compressed entry text, not in `.dat` (the `ALIAS` record of `LZip`); they are
  followed in both places (as `README.md` says). Up to 8 links are followed and a
  loop gives no entry.
* **zLD blocks** are compressed with the conf's `CompressType` and enciphered like
  zText blocks; the `.zdx` entries' second field is the stored (compressed) size.
* **Sort order.** Keys are stored upper-cased (case-insensitive modules) and sorted
  by bytes (`00001` … `05600`, `AARON` … `WORD99`, `ZION`, then `ÉBÈNE` and `ΩMEGA`). Lookups
  binary-search the index comparing the folded key's bytes with the stored key's.
* **get_strongs rewritten by a second clean implementer.** The first implementer had read parts
  of the old topos-bible-markup source, which influenced which keys `get_strongs` tried and in what
  order, so the function was deleted unread and written again by an implementer who has seen
  neither that crate nor any SWORD code, from `api.md`'s doc comments, `strongs-keys.md`, and the
  dictionary fixtures. (`get` itself, per its doc comment, tries the key as given and then, for a
  prefixed key like `G25` that misses, the bare number.) `get_strongs` tries, in order, and returns
  the first that is found and resolves to an entry:
  1. with a suffix (`H0430a`): the bare padded key with it (`00430a`), then the prefixed one
     (`H0430a`);
  2. the bare key, zero-padded to five digits (`00025`);
  3. the prefixed key, the language letter and the number zero-padded to four digits (`G0025`).

  Reasoning: the doc comment names exactly the plain padded form and the prefixed one.
  `strongs-keys.md` gives their stored shapes (five digits bare, four after a prefix), and writing
  them out already padded makes them match in modules with `StrongsPadding=false` too (lookups
  still fold case unless `CaseSensitiveKeys=true`). The bare form goes first because every
  dictionary fixture, like the StrongsGreek and StrongsHebrew modules the doc comment names,
  stores bare five-digit keys (`00025`, `05600`). A suffix is tried first because it names a more
  specific entry, then dropped because these lexicons have one entry per number. A key that is
  found but whose `@LINK` leads nowhere doesn't stop the search. Checked on all five dictionary
  formats in the fixtures (`G25`, `H0430a`, `g5600`, and `H1` found; `G26` and `H123456` not).
  Since the markup dependency went (October 2026) the number is a string, not a `Strongs`: the
  letter `G` or `H` in either case (it gives the prefix, always written upper-case), digits (read
  as a number, so `G0025` is `G25`; more than fits a `u32` finds nothing), and an optional suffix
  of ASCII letters, with surrounding whitespace ignored; any other string finds nothing. The keys
  tried and their order are unchanged.
* **Empty entries.** An entry whose normalised text is empty is no entry (as for
  verse modules); `iter()` skips it.
* **RawLD with a cipher key** (no fixture; `osis2mod` never makes one): only the
  record body is deciphered, since the key must stay readable for sorting
  (**choice**).

## General books

* **Node records** (observed in `GBook`): `.dat` may contain stale records that no
  `.idx` entry points to (left when a node is rewritten); only `.idx` offsets are
  used. The root has an empty name and no user data. A node has text when its user
  data is at least 8 bytes and the size is not zero.
* **Paths** are `/` followed by the names from the root's child down (`/Part 2/Chapter 3`);
  the root's path is `/`. Empty path segments are ignored when finding a node.
* **Loops** in parent, sibling or child links are reported as `Corrupt` rather than
  followed forever.
* `KeyType` other than `TreeKey` is refused as `UnsupportedDriver` (`README.md`).

## Text

* **UTF-16 modules and normalisation.** `entry-text.md` normalises bytes before
  decoding and cuts at the first NUL, which would cut UTF-16 text after one
  character. UTF-16 entries are decoded first and the UTF-8 result normalised
  (**choice**; no fixture).
* **Missing `Encoding`.** From `README.md`/`examples/compare_imp.rs`: entries that are
  valid UTF-8 are read as UTF-8, others as Latin-1/CP1252.
* **Missing `SourceType`.** From `README.md`: an entry containing `</` or `/>` is
  not GBF. **Choice** of which XML markup: TEI if it contains `<entryFree`, `<orth`,
  `<def>` or `<superentry`; OSIS if it contains `osisID`, `osisRef`, `<w `,
  `<transChange`, `<divineName`, `<verse`, `<note`, `<q ` or `<l>`; ThML otherwise.
* **Unknown `SourceType` values** are read as plain text; unknown `Encoding` values
  as Latin-1; unknown `BlockType` values as CHAPTER (**choice**).

## Conf files and RTF

* **Whitespace** in the line rules means ASCII space, tab, CR, LF, VT and FF.
* **Lines before the first `[Section]`** are ignored. `ModuleConf::parse` with no
  section is an error (`Conf`).
* **`ModDrv`, `CompressType`, `BlockType`, `Direction` values** are matched ignoring
  case. `Direction` is `RtoL` or `BiDi`, anything else left to right.
  `CaseSensitiveKeys=true` and `StrongsPadding=false` are matched ignoring case.
* **`CipherKey`** that is empty or only whitespace means locked.
* **`set`** replaces the first value and removes any repeats of the key.
* **RTF** (the RTF 1.x specification): a control word is a backslash, letters, an
  optional signed number, and an optional single space that belongs to it. `\uN`
  with a negative N means N + 65536. After `\uN`, one fallback character (or one
  `\'hh`) is skipped, not only `?`. Control symbols other than `\\ \{ \}` (`\~`,
  `\-`, …) are dropped. `\'hh` is a Latin-1 byte.

## Installer

`README.md` describes the protocol; these details are not in any spec.

* **masterRepoList.conf** (the published file): each line is
  `uid=TypeSource=Caption|host|directory`, with a different uid on each line, and a
  repository usually has an `HTTPSPackagePreference` line and an `FTPSource` line
  with the same caption. Lines are grouped into repositories by caption;
  package-preference sources come first; a repository keeps the uid of its first
  ordinary `…Source` line (so CrossWire is `20081216195754`, the uid the API
  documentation shows).
* **Source kinds**: `HTTPS`, `HTTP`, `FTP`, `SFTP`, and `DIR`/`FILE`/`LOCAL` (local
  directories), each followed by `Source` or `PackagePreference`.
* **Package zips** are fetched as `packages/<Name>.zip`, then with the name
  lower-cased; a missing package (not found) falls back to copying the module's
  data directory file by file. Other errors move on to the next source.
* **Catalogue** without `mods.d.tar.gz` (not found): `mods.d/` is listed and each
  `*.conf` fetched. Catalogue entries are sorted by conf file name.
* **Installed confs** get `InstallSourceCaption` (the repository's name) and, when
  the repository has a uid, `InstallSourceID`, added after the section header or
  replacing earlier values. Conf files from a repository are otherwise written as
  published.
* **What may still install**: a module whose staged copy fails to open only because
  it is locked, uses an unsupported driver or compression, or names an unknown
  versification. Any other failure rejects the installation.
* **Data directories**: `module_data_dir` needs `modules/` plus at least two more
  components, and refuses a three-component path whose second component is
  `texts`, `comments`, `lexdict` or `genbook` (those directories are shared by many
  modules). Zip entries outside `mods.d/` and `modules/` are ignored; unsafe names
  (absolute, `..`, drive letters, control characters) reject the whole zip.
* **Reinstalling** removes the old data directory first, and removes other conf
  files that declare the same module.
* **HTML listings**: every `href` naming an entry inside the listed directory
  (relative, or absolute under the listed path); query links, parent links and links
  elsewhere are skipped; a trailing `/` marks a directory. Sizes are not read.
* **FTP**: passive mode (`PASV`, then `EPSV`), binary type, anonymous login unless the
  URL has credentials; the data connection goes to the control connection's peer
  address (the address in a `227` reply is often private). Listings are `CWD` then
  `LIST`, in Unix or MS-DOS style. Reply 550/450 is `NotFound`.
