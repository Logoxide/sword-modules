# sword-modules

A pure-Rust reader for [CrossWire SWORD](https://www.crosswire.org/sword/) modules, the
format behind AndBible, BibleTime, Xiphos, Ezra Bible App and others. Like JSword
(which AndBible uses), it is a native reimplementation, not an FFI wrapper around
the C++ libsword, so it builds anywhere Rust does (Android, iOS, WASM).

It is verified against libsword itself: every entry of 15 real modules from
CrossWire (~207k entries) and of modules generated with libsword's own tools in
every storage format is byte-identical to libsword's `mod2imp` output (apart from
the deliberate deviations below).

## What's supported

| Area | Support |
|---|---|
| Bibles | `RawText`, `RawText4`, `zText`, `zText4` |
| Commentaries | `RawCom`, `RawCom4`, `zCom`, `zCom4`; personal notes (`RawFiles`, read-only) |
| Lexicons / dictionaries / devotionals | `RawLD`, `RawLD4`, `zLD` |
| General books | `RawGenBook` (`KeyType=TreeKey`) |
| Compression | ZIP, LZSS, BZIP2, XZ (each a cargo feature except LZSS) |
| Encryption | Sapphire II (`CipherKey`), including SWORD's obfuscated key format |
| Versification | All 18 built-in systems (KJV, KJVA, NRSV, NRSVA, Synodal, SynodalProt, Vulg, German, Luther, Catholic, Catholic2, LXX, Orthodox, MT, Leningrad, Calvin, DarbyFr, Segond) |
| Markup | Named, not parsed: each entry is its stored text with the conf's `SourceType` (OSIS, ThML, GBF, TEI, plain text), sniffed when the conf has none |
| Encodings | UTF-8, Latin-1 (as CP1252, like libsword), UTF-16LE |
| Installing | CrossWire's master repository list, any SWORD repository over HTTPS/HTTP/FTP/`file://`, a module zip at any URL, local zips; update status, uninstall |

Not supported: writing personal notes, `HREFCom`, `RawGenBook` with a `VerseKey`
key type, cross-versification mapping, search indexes, and SFTP repositories.

Verse references come from [topos](https://crates.io/crates/topos-bible) (re-exported
as `sword_modules::topos`). `bible_data()` parses references like `Jn 3:16-18; 5` or
`Sir 24:3` into a topos `Passage`, which `VerseModule::passage` reads using the module's own
versification. One position is a `VerseKey { book: BookId, chapter, verse }`; chapter 0 and
verse 0 are book and chapter introductions (`book_intro`, `chapter_intro`).

## Usage

```rust
use sword_modules::{bible_data, Markup, SwordLibrary, VerseKey};

let lib = SwordLibrary::open("/path/to/sword")?; // dir containing mods.d/ and modules/

// Bibles and commentaries
let kjv = lib.open_module("KJV")?.into_verse()?;
for (key, entry) in kjv.passage(&bible_data().parse("Jn 3:16-18; 5").expect("a reference"))? {
    println!("{key} {}", entry.raw);           // "John 3:16 <w …>For</w> …", as stored
}
let john = bible_data().books().search("John").expect("a book");
let e = kjv.verse(&VerseKey::new(john, 3, 16))?.expect("present");
assert_eq!(e.markup, Markup::Osis);            // the conf's SourceType; parse `e.raw` with it
let psalms = bible_data().books().search("Ps").expect("a book");
for (key, entry) in kjv.chapter(psalms, 23)? { /* verse 0 = heading/intro */ }
for item in kjv.iter() { let (key, entry) = item?; /* whole module */ }

// Lexicons (libsword key rules: upper-casing, Strong's zero-padding, @LINK aliases)
let greek = lib.open_module("StrongsGreek")?.into_dictionary()?;
let agapao = greek.get("G25")?;                 // also "25", "00025"
let agapao = greek.get_strongs("G25")?;         // tries 00025, then G0025

// General books
let book = lib.open_module("Pilgrim")?.into_genbook()?;
for (depth, node) in book.walk()? { println!("{}{}", "  ".repeat(depth), node.name); }
let text = book.get("/Part 1/Chapter 2")?;

// Locked modules
let m = lib.open_module_with_key("SomeLockedBible", "unlock-key-from-publisher")?;
```

Personal-notes commentaries (`ModDrv=RawFiles`, written by BibleTime/Xiphos) open
as ordinary verse modules; `is_personal_notes()` identifies them. As in libsword
they always use KJV versification. Each verse's index entry names a file
(`0000012`) in the module directory holding the note; file names that could point
outside that directory are ignored.

The crate doesn't parse markup and depends on no markup crate. An `Entry` is `raw` (the
stored text, deciphered, normalised and decoded to UTF-8, markup included) and `markup` (a
`Markup`: the conf's `SourceType`, or for a conf without one, GBF unless the text has XML end
tags, then TEI, OSIS or ThML by telltale elements). Parse and render it with whichever markup
library you use (logoxide uses its `logoxide-markup`).

`examples/sword.rs` is a small CLI (`list`, `verse`, `chapter`, `dict`, `toc`, `book`,
`repos`, `remote`, `install`, `install-url`, `uninstall`).

### Without a file system

Every file is read through the `Files` trait (read a file, list a folder, and optionally open
one to read at offsets), so a library needn't be a folder: `SwordLibrary::open` uses `FsFiles`,
and `SwordLibrary::open_files` takes any other, such as `MemoryFiles` filled from a module zip.
That's how it runs in WebAssembly (build with `--no-default-features --features
zip,bzip2,xz`; the installer needs a file system):

```rust
use std::sync::Arc;
use sword_modules::{MemoryFiles, SwordLibrary};

let mut files = MemoryFiles::new();
for (path, bytes) in unzipped_module {          // e.g. packages/KJV.zip's entries
    files.insert(path, bytes);
}
let lib = SwordLibrary::open_files(Arc::new(files))?;
let kjv = lib.open_module("KJV")?.into_verse()?;
```

## Installing modules

```rust
use sword_modules::install::{self, Installer, ModuleStatus, Repository};

let installer = Installer::new();
let repos = installer.master_repo_list()?;               // CrossWire's registry
let crosswire = repos.iter().find(|r| r.name == "CrossWire").unwrap();
let catalog = installer.catalog(crosswire)?;              // mods.d.tar.gz
for m in catalog.of_kind(sword_modules::ModuleKind::Bible) {
    println!("{} — {}", m.name(), m.conf.description().unwrap_or(""));
}
installer.install(&catalog, "KJV", "/path/to/sword")?;    // install or update

// any other repository, a zip URL, or a local zip
let ebible = Repository::from_url("eBible.org", "https://ebible.org/sword")?;
installer.install_from_url("https://example.org/MyModule.zip", "/path/to/sword")?;
install::install_zip_file("Downloads/KJV.zip", "/path/to/sword")?;

// updates, removal, unlocking
let lib = sword_modules::SwordLibrary::open("/path/to/sword")?;
for (m, status) in install::module_status(&catalog, &lib) {
    if status == ModuleStatus::UpdateAvailable { installer.install(&catalog, m.name(), "/path/to/sword")?; }
}
install::uninstall("/path/to/sword", "KJV")?;
install::save_cipher_key("/path/to/sword", "SomeLockedBible", "key-from-publisher")?;
```

How it works (libsword's InstallMgr protocol): CrossWire publishes
`masterRepoList.conf`, listing each repository with one or more sources (HTTPS,
HTTP, FTP; `…PackagePreference` sources are tried first). Every repository root
has `mods.d.tar.gz` (all its confs), `packages/<Module>.zip`, and `modules/…`. The
installer uses the package zip when present and otherwise copies the module's data
directory file by file from HTTP directory listings or FTP `LIST`. Your own
libsword configuration can be reused with `install::parse_install_conf`.

Downloads are unpacked into a staging directory inside the library and moved into
place only after the module opens successfully (locked modules, and drivers this
crate cannot read, still install). Archive entries and server-supplied names are
validated, nothing outside `mods.d/` and `modules/` is written, unpacking is capped
at 4 GiB, and `uninstall` refuses a `DataPath` that is not a module directory.
Installed confs get `InstallSourceCaption`/`InstallSourceID`, as libsword does.

**Transports.** `DefaultTransport` handles `https://`/`http://` (via `ureq` with
rustls, feature `https`), `ftp://` (a small built-in passive-mode client, since some
repositories such as IBT and STEP Bible are FTP-only) and `file://`. To use your
platform's HTTP stack, a proxy, caching or progress reporting, implement the
two-method `install::Transport` trait and use `Installer::with_transport`; then
disable the `https` feature to drop `ureq`. CrossWire's community asks apps to
make internet access an explicit user choice; this crate never connects on its
own, only when you call these functions.

## The format, briefly

A *library* is a directory with `mods.d/*.conf` (one INI-style file per module) and
`modules/…` (data). Module zips from CrossWire repositories unzip into that layout.

**Conf files.** `[Name]` section, `Key=Value` lines, repeatable keys
(`GlobalOptionFilter`, `Feature`), `\` line continuation, RTF-ish `About`
(`\par`, `\u8212?`). Key entries: `DataPath`, `ModDrv`, `SourceType`
(default **GBF**), `Encoding` (default **Latin-1**), `CompressType` (default LZSS),
`BlockType` (default CHAPTER), `Versification` (default KJV), `CipherKey`.

**Verse-keyed modules** keep one index slot for *every possible* position in the
versification, per testament: slot 0 module heading, 1 testament heading, then for
each book a book-intro slot, and for each chapter a chapter-intro slot followed by
its verses (KJV: Gen 1:1 = slot 4). All integers are little-endian.

* Raw: `ot.vss` (`u32 offset, u16 size`; `u32 size` in the `4` variants) → `ot`.
* Compressed: `ot.Xzv` (`u32 block, u32 offset, u16|u32 size`) → `ot.Xzs`
  (`u32 offset, u32 compressed size, u32 size`) → `ot.Xzz` (blocks), where `X` is
  `b`/`c`/`v` for BlockType BOOK/CHAPTER/VERSE. Enciphering applies to each
  compressed block.

**String-keyed modules** (`DataPath` is a file prefix): sorted `.idx`
(`u32 offset, u16|u32 size`) → `.dat` records `KEY\r\n` + text or `@LINK OTHER`. zLD
adds `.zdx`/`.zdt` blocks, each holding a small table of NUL-terminated entries.

**General books**: `.idx` (node → `.dat` offset), `.dat` (parent/next/child links
*as `.idx` offsets*, name, `u32 offset, u32 size` into `.bdt`).

**Markup** is stored per verse/entry and is *not* well-formed XML: quotes and
paragraphs that span verses appear as `sID`/`eID` milestones or as a start tag in one
verse and its end tag in another, so a parser of entries has to be tolerant. GBF is a
non-XML token format (`<FR>…<Fr>`, `word<WG25>`); ThML and GBF Strong's tags follow the
word they tag.

## Deviations from libsword (deliberate)

* If a conf declares the wrong index width (e.g. Ubuntu's MHCC package says
  `zCom4` but has 10-byte `zCom` entries), the width that matches the
  versification is used. libsword returns misaligned text for that module.
* `@LINK` aliases stored *inside* compressed (zLD) entries are followed;
  libsword returns the literal `@LINK …` text.
* `get("G25")` falls back to `00025` when a lexicon's keys have no language prefix.
* When `Encoding`/`SourceType` are missing from the conf, entries that are valid
  non-ASCII UTF-8 are decoded as UTF-8, and entries with XML end tags are parsed as
  OSIS/TEI/ThML rather than GBF.

## Cargo features

`default = ["zip", "bzip2", "xz", "install", "https"]`. `zip`/`bzip2`/`xz` enable
those `CompressType`s (LZSS is always available); `install` adds the installer
(FTP and `file://` included); `https` adds the built-in HTTP(S) client. Minimum
Rust version: 1.88 (required by current `zip`).

## Testing

`cargo test` runs unit tests plus integration tests over `tests/fixtures.tar.gz`:
modules generated with `osis2mod`/`imp2ld`/`imp2gbs` in every driver, block type
and compression, an enciphered module, a non-KJV versification, a personal-notes
module written by libsword's RawFiles driver, and `mod2imp` reference dumps.
`tests/install.rs` serves those modules as a CrossWire-layout repository from an
in-process HTTP server and via `file://`, installs them both from package zips
and file by file, checks the results byte for byte, and feeds the installer
hostile archives. (The same repository was also checked with libsword's own
`installmgr`, and the installer was exercised against a real FTP server.) Against a real library:

```sh
SWORD_TEST_LIBRARY=~/.sword cargo test -- --ignored --nocapture
# byte-compare one module with libsword:
mod2imp KJV > kjv.imp && cargo run --example compare_imp -- ~/.sword KJV kjv.imp
```

`tools/differential/run.py` runs randomised black-box comparisons of the routines
in `docs/spec/` against libsword's compiled tools and Python bindings
(needs `libsword-utils` and `python3-sword` on Ubuntu, `sword` and `python-sword`
on Arch).

Versification data: `tools/extract_v11n.py` queries libsword's runtime API for
each system's books and verse counts into `data/versification.json`, and
`tools/v11n_to_rust.py` turns that into `src/versification/data.rs`.

## Specifications

`docs/spec/` describes the module formats and every compatibility rule the code
follows (entry text normalisation, dictionary keys, conf files, LZSS, the cipher,
versification and index layout). See `CLEANROOM.md` for how they were produced.

## License

CC0 1.0 Universal (public domain dedication); see `LICENSE`. To the extent
possible under law, the authors have waived all copyright and related rights to
this crate.

This crate contains no code from libsword (GPL-2.0). The parts that were first
ported from it were removed and reimplemented from behavioural specifications;
`CLEANROOM.md` documents the process, its evidence and its limits. Dependencies
keep their own (permissive) licenses: MIT, Apache-2.0, ISC, BSD-3-Clause, Zlib,
bzip2-1.0.6 and, via `webpki-roots` (feature `https`), CDLA-Permissive-2.0. A
binary that includes them must carry their notices.
