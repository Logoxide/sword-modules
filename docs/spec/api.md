## `src/cipher.rs`

```rust
```

## `src/conf.rs`

```rust
/// One `[Section]` of a conf file: a module name and its ordered key/value entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleConf { /* private fields */ }

impl ModuleConf {
    /// Parse the first module section in `text`.
    pub fn parse(text: &str) -> Result<ModuleConf>;

    /// Read and parse a `.conf` file. The bytes are decoded as UTF-8 when the file
    /// declares `Encoding=UTF-8` (or is valid UTF-8), otherwise as Latin-1/CP1252.
    pub fn from_file(path: impl AsRef<Path>) -> Result<ModuleConf>;

    /// Create a conf programmatically (handy for tests or synthetic modules).
    pub fn new(name: impl Into<String>, entries: Vec<(String, String)>) -> Self;

    /// The module name (the `[Section]` header), e.g. `KJV`.
    pub fn name(&self) -> &str;

    /// All entries in file order.
    pub fn entries(&self) -> &[(String, String)];

    /// First value for `key` (keys are matched case-sensitively, as SWORD does).
    pub fn get(&self, key: &str) -> Option<&str>;

    /// Every value for a repeatable key such as `GlobalOptionFilter` or `Feature`.
    pub fn get_all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a;

    /// Value for a localized key, e.g. `get_localized("Description", "de")` looks up
    /// `Description_de` first and falls back to `Description`.
    pub fn get_localized(&self, key: &str, lang: &str) -> Option<&str>;

    /// Set (replace) a value. Useful to supply a `CipherKey` for a locked module.
    pub fn set(&mut self, key: &str, value: impl Into<String>);

    /// `ModDrv`: the storage driver.
    pub fn mod_drv(&self) -> Result<ModDrv>;

    /// `DataPath` relative to the library root, with any leading `./` or `/` removed.
    pub fn data_path(&self) -> Option<&str>;

    /// `SourceType` — the markup of entries. SWORD defaults to GBF when absent.
    pub fn source_type(&self) -> Markup;

    /// `Encoding` of entry text. SWORD defaults to Latin-1 when absent.
    pub fn encoding(&self) -> TextEncoding;

    /// `CompressType` for z* drivers. SWORD defaults to LZSS when absent.
    pub fn compress_type(&self) -> CompressType;

    /// `BlockType` for zText/zCom. SWORD defaults to CHAPTER.
    pub fn block_type(&self) -> BlockType;

    /// `Versification` system name. SWORD defaults to `KJV`.
    pub fn versification(&self) -> &str;

    /// `CipherKey`. `Some("")` means the module is enciphered but locked.
    pub fn cipher_key(&self) -> Option<&str>;

    /// True when the module is enciphered and no key has been entered.
    pub fn is_locked(&self) -> bool;

    /// `Description`.
    pub fn description(&self) -> Option<&str>;

    /// `About`, converted from SWORD's RTF subset to plain text.
    pub fn about_text(&self) -> Option<String>;

    /// `Lang` (BCP-47-ish language code, e.g. `en`, `grc`, `he`).
    pub fn lang(&self) -> Option<&str>;

    /// `Version` of the module.
    pub fn version(&self) -> Option<&str>;

    /// `Direction`: text direction.
    pub fn direction(&self) -> Direction;

    /// `GlobalOptionFilter` values (e.g. `OSISStrongs`, `OSISFootnotes`).
    pub fn global_option_filters(&self) -> Vec<&str>;

    /// `Feature` values (e.g. `StrongsNumbers`, `GreekDef`, `HebrewDef`).
    pub fn features(&self) -> Vec<&str>;

    /// `Category` (e.g. `Biblical Texts`, `Commentaries`, `Lexicons / Dictionaries`,
    /// `Generic Books`, `Daily Devotional`, `Glossaries`, `Cults / Unorthodox / Questionable Material`).
    pub fn category(&self) -> Option<&str>;

    /// `CaseSensitiveKeys` for dictionaries (default false).
    pub fn case_sensitive_keys(&self) -> bool;

    /// `StrongsPadding` for dictionaries (default true).
    pub fn strongs_padding(&self) -> bool;

    /// `KeyType` for general books (default `TreeKey`).
    pub fn key_type(&self) -> &str;

    /// High-level module kind derived from `ModDrv`.
    pub fn module_kind(&self) -> Result<ModuleKind>;

/// Parse all `[Section]`s in a conf text (rules in `docs/spec/conf.md`).
pub fn parse_conf(text: &str) -> Result<Vec<ModuleConf>>;

/// Convert SWORD's RTF subset (used by `About` and similar entries) into plain text.
///
/// Handles `\par` / `\line` (newline), `\tab`, `\uN?` Unicode escapes, `\'hh` hex
/// escapes, escaped `\\ \{ \}`, drops other control words (`\pard`, `\qc`, `\b0`, …)
/// and braces.
pub fn rtf_to_text(s: &str) -> String;

/// Storage drivers (`ModDrv`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModDrv {
    RawText,
    RawText4,
    ZText,
    ZText4,
    RawCom,
    RawCom4,
    ZCom,
    ZCom4,
    HrefCom,
    RawFiles,
    RawLD,
    RawLD4,
    ZLD,
    RawGenBook,
    Other(String),
}

impl ModDrv {
    pub fn parse(s: &str) -> ModDrv;

    /// Which kind of module this driver stores.
    pub fn kind(&self) -> ModuleKind;

/// Broad module categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModuleKind {
    Bible,
    Commentary,
    Dictionary,
    GenBook,
    Unknown,
}

impl ModuleKind {
    pub fn as_str(self) -> &'static str;

/// Markup used inside entries: the conf's `SourceType`. This crate only names it;
/// parsing and rendering entries is left to the caller (logoxide's `logoxide-markup`, for
/// one), which reads [`Entry::raw`](crate::Entry) with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Markup {
    Osis,
    Thml,
    Gbf,
    Tei,
    /// `Plaintext`, and any `SourceType` this crate doesn't know.
    Plaintext,
}

impl Markup {
    /// A markup by its `SourceType` name, ignoring case: `OSIS`, `ThML`, `GBF`, `TEI`,
    /// `Plaintext` or `Plain`. `None` for any other name.
    pub fn from_name(name: &str) -> Option<Markup>;

    /// The `SourceType` name as confs spell it: `OSIS`, `ThML`, `GBF`, `TEI`, `Plaintext`.
    pub fn as_str(self) -> &'static str;

/// Compression used by z* drivers (`CompressType`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompressType {
    Zip,
    Lzss,
    Bzip2,
    Xz,
    Other(String),
}

impl CompressType {
    pub fn parse(s: &str) -> CompressType;

/// Granularity of compressed blocks (`BlockType`). Determines z* index file names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    Verse,
    Chapter,
    Book,
}

impl BlockType {
    pub fn parse(s: &str) -> BlockType;

    /// The letter used in file names: `ot.bzs` (BOOK), `ot.czs` (CHAPTER), `ot.vzs` (VERSE).
    pub fn file_letter(self) -> char;

/// Text direction (`Direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    LeftToRight,
    RightToLeft,
    Bidi,
}
```

## `src/drivers/genbook.rs`

```rust
/// A node of a general book's table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    /// Stable node id (the node's byte offset in the `.idx` file; root = 0).
    pub offset: u32,
    pub parent: Option<u32>,
    pub next_sibling: Option<u32>,
    pub first_child: Option<u32>,
    /// The node's name (one path segment, e.g. `Chapter 1`).
    pub name: String,
}

impl TreeNode {
    pub fn has_children(&self) -> bool;

    /// Whether the node has entry text of its own.
    pub fn has_text(&self) -> bool;

```

## `src/drivers/mod.rs`

```rust
```

## `src/drivers/strkey.rs`

```rust
```

## `src/drivers/verse.rs`

```rust
```

## `src/encoding.rs`

```rust
//! Character encodings used by SWORD module text (`Encoding=` in the conf).
/// The encoding of a module's entry text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextEncoding {
    Utf8,
    /// `Latin-1` (SWORD's default). Like libsword's `Latin1UTF8` filter, bytes
    /// 0x80–0x9F are interpreted as Windows-1252.
    Latin1,
    /// `UTF-16` (little-endian, as written by SWORD on all common platforms).
    Utf16,
    /// `SCSU` — rare; decoded as Latin-1 as a best effort.
    Scsu,
}

impl TextEncoding {
    pub fn parse(s: &str) -> TextEncoding;

/// Decode raw entry bytes to a `String`.
///
/// Invalid UTF-8 in a UTF-8 module is replaced with U+FFFD rather than failing,
/// since a handful of published modules contain stray bytes.
pub fn decode_text(bytes: &[u8], enc: TextEncoding) -> String;
```

## `src/error.rs`

```rust
/// Errors produced while reading SWORD modules.
#[derive(Debug)]
pub enum Error {
    /// Underlying I/O failure, with the file involved when known.
    Io { path: Option<PathBuf>, source: io::Error },
    /// A `.conf` file is malformed or lacks a required entry.
    Conf(String),
    /// The module uses a driver (`ModDrv`) this crate does not implement.
    UnsupportedDriver(String),
    /// The module's `CompressType` is unknown, or its cargo feature is disabled.
    UnsupportedCompression(String),
    /// The `Versification` named in the conf is not one of SWORD's built-in systems.
    UnknownVersification(String),
    /// The module is enciphered and no `CipherKey` was supplied (a "locked" module).
    Locked(String),
    /// Index or data files are inconsistent (truncated, out-of-range offsets, …).
    Corrupt(String),
    /// Decompression failed.
    Decompress(String),
    /// A module with that name was not found in the library.
    ModuleNotFound(String),
    /// The operation does not apply to this kind of module
    /// (e.g. verse lookup on a dictionary).
    WrongModuleType { expected: &'static str, found: &'static str },
    /// A remote file does not exist (HTTP 404, FTP 550, missing local file).
    NotFound(String),
    /// Any other network / transport failure.
    Network { url: String, message: String },
    /// Installing or removing a module failed (bad archive, unsafe paths, …).
    Install(String),
}

impl fmt::Display for Error { … }

impl std::error::Error for Error { … }

impl From<io::Error> for Error { … }


pub type Result<T> = std::result::Result<T, Error>;
```

## `src/files.rs`

```rust
//! Where a library's files are read from: a folder on disk ([`FsFiles`]), or anything else
//! that can hand over a file's bytes (memory, a host's files in WebAssembly), so the readers
//! never touch the file system themselves. Paths are `/`-separated and relative to the
//! library root (`mods.d/kjv.conf`, `modules/texts/ztext/kjv/ot.bzs`).

/// A library's files.
pub trait Files: Send + Sync {
    /// A whole file, or `None` when it does not exist.
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>>;

    /// The names of the files (not folders) directly in a folder, in any order; empty when
    /// the folder does not exist.
    fn list(&self, dir: &str) -> Result<Vec<String>>;

    /// A file to read in pieces (data files can be large), or `None` when it does not exist.
    /// The default reads the whole file with [`read`](Files::read).
    fn open(&self, path: &str) -> Result<Option<Box<dyn FileReader>>>;
}

/// A file opened for reading at offsets.
pub trait FileReader: Send + Sync {
    /// Its size in bytes.
    fn size(&self) -> u64;

    /// Fill `buf` from `offset`; an error when the file is shorter.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()>;
}

/// A file's bytes in memory.
impl FileReader for Vec<u8> { … }

/// A folder on disk: the library root.
#[derive(Debug, Clone)]
pub struct FsFiles { /* private fields */ }

impl FsFiles {
    pub fn new(root: impl Into<PathBuf>) -> Self;

    pub fn root(&self) -> &Path;

/// Reads files from disk, opening data files once and reading them at offsets.
impl Files for FsFiles { … }

/// Files kept in memory, by path (an unpacked module zip, a test).
#[derive(Debug, Clone, Default)]
pub struct MemoryFiles { /* private fields */ }

impl MemoryFiles {
    pub fn new() -> Self;

    /// Add (or replace) a file.
    pub fn insert(&mut self, path: impl Into<String>, bytes: Vec<u8>);

impl Files for MemoryFiles { … }
```

## `src/install/ftp.rs`

```rust
```

## `src/install/mod.rs`

```rust
pub mod transport;

pub use transport::{DefaultTransport, DirEntry, Transport};

/// Where CrossWire publishes the list of known repositories.
pub const MASTER_REPO_LIST_URL: &str = "https://crosswire.org/ftpmirror/pub/sword/masterRepoList.conf";
/// FTP fallback for the master list (used by libsword when HTTPS fails).
pub const MASTER_REPO_LIST_FTP_URL: &str = "ftp://ftp.crosswire.org/pub/sword/masterRepoList.conf";

/// Transfer protocol of a repository source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Protocol {
    Https,
    Http,
    Ftp,
    Sftp,
    File,
}

/// One way of reaching a repository (libsword's `InstallSource`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub protocol: Protocol,
    pub host: String,
    /// Repository root on the host, e.g. `/ftpmirror/pub/sword/raw`.
    pub directory: String,
    pub user: Option<String>,
    pub password: Option<String>,
    /// From a `…PackagePreference` entry: try this source first.
    pub package_preference: bool,
}

impl Source {
    /// Parse `Caption|host|directory|user|password|uid` (the value of a
    /// `HTTPSSource=` / `FTPSource=` / `…PackagePreference=` entry).
    /// Returns the caption, the uid (if given) and the source.
    pub fn parse_entry(kind: &str, value: &str) -> Option<(String, Option<String>, Source)>;

    /// Base URL of the repository root, ending in `/`.
    pub fn base_url(&self) -> String;

    /// Build a source from a URL such as `https://ebible.org/sword`.
    pub fn from_url(url: &str) -> Option<Source>;

/// A repository (e.g. "CrossWire", "eBible.org") with one or more sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    pub name: String,
    /// Stable id from the master list (e.g. `20081216195754`), if any.
    pub uid: Option<String>,
    /// Sources in the order they are tried.
    pub sources: Vec<Source>,
}

impl Repository {
    /// A repository reachable at one URL, e.g.
    /// `Repository::from_url("eBible.org", "https://ebible.org/sword")`.
    pub fn from_url(name: impl Into<String>, url: &str) -> Result<Repository>;

    /// CrossWire's main repository (HTTPS, falling back to FTP).
    pub fn crosswire() -> Repository;

/// Parse CrossWire's `masterRepoList.conf` (`[Repos]` with `uid=TypeSource=…` lines).
pub fn parse_master_repo_list(text: &str) -> Vec<Repository>;

/// Parse a libsword `InstallMgr.conf` (`[Sources]` with `HTTPSSource=…` etc.), e.g.
/// `~/.sword/InstallMgr/InstallMgr.conf`, to reuse a user's configured repositories.
pub fn parse_install_conf(text: &str) -> Vec<Repository>;

/// A module available in a repository.
#[derive(Debug, Clone)]
pub struct RemoteModule {
    pub conf: ModuleConf,
    /// File name in `mods.d/` (e.g. `kjv.conf`).
    pub conf_file: String,
    /// The conf file exactly as published.
    pub conf_bytes: Vec<u8>,
}

impl RemoteModule {
    pub fn name(&self) -> &str;

/// The modules a repository offers.
#[derive(Debug, Clone)]
pub struct Catalog {
    pub repository: Repository,
    pub modules: Vec<RemoteModule>,
}

impl Catalog {
    /// Find a module by name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<&RemoteModule>;

    /// Modules of one kind (Bibles, commentaries, …).
    pub fn of_kind(&self, kind: ModuleKind) -> impl Iterator<Item = &RemoteModule>;

/// How a remote module relates to what is installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleStatus {
    NotInstalled,
    /// Remote `Version` is newer.
    UpdateAvailable,
    SameVersion,
    /// Installed copy is newer than the repository's.
    InstalledIsNewer,
}

/// Compare SWORD version strings (`1.2.10` > `1.2.9`; missing parts count as 0).
pub fn compare_versions(a: &str, b: &str) -> Ordering;

/// Status of every module in `catalog` relative to `installed`.
pub fn module_status<'a>(catalog: &'a Catalog, installed: &SwordLibrary) -> Vec<(&'a RemoteModule, ModuleStatus)>;

/// Downloads catalogues and installs modules.
pub struct Installer { /* private fields */ }

impl Default for Installer { … }

impl Installer {
    /// Installer using [`DefaultTransport`].
    pub fn new() -> Self;

    /// Installer using your own [`Transport`].
    pub fn with_transport(transport: impl Transport + 'static) -> Self;

    pub fn transport(&self) -> &dyn Transport;

    /// Fetch and parse CrossWire's list of known repositories.
    pub fn master_repo_list(&self) -> Result<Vec<Repository>>;

    /// Fetch a repository's catalogue (`mods.d.tar.gz`, or `mods.d/*.conf`).
    pub fn catalog(&self, repo: &Repository) -> Result<Catalog>;

    /// Install (or update) a module from a catalogue into the library rooted at
    /// `library_root` (created if needed). Returns the installed module names.
    pub fn install(&self, catalog: &Catalog, module: &str, library_root: impl AsRef<Path>) -> Result<Vec<String>>;

    /// Download a module zip from any URL (`https://…/KJV.zip`, `ftp://…`,
    /// `file://…`) and install every module it contains.
    pub fn install_from_url(&self, url: &str, library_root: impl AsRef<Path>) -> Result<Vec<String>>;

/// Install every module contained in a SWORD module zip (as distributed by
/// CrossWire, AndBible, eBible.org, …). Returns the installed module names.
pub fn install_zip(zip_bytes: &[u8], library_root: impl AsRef<Path>) -> Result<Vec<String>>;

/// Install from a local `.zip` file.
pub fn install_zip_file(zip_path: impl AsRef<Path>, library_root: impl AsRef<Path>) -> Result<Vec<String>>;

/// The directory holding a module's data, relative to the library root. For
/// dictionaries and general books `DataPath` is a file prefix, so its parent is used.
/// Refuses anything that is not safely inside `modules/`.
pub fn module_data_dir(conf: &ModuleConf) -> Result<PathBuf>;

/// Remove an installed module: its data directory and its `.conf` file.
pub fn uninstall(library_root: impl AsRef<Path>, module: &str) -> Result<()>;

/// Store a `CipherKey` for an installed module in its `.conf` (as libsword's
/// installers do), so it opens without supplying the key each time.
pub fn save_cipher_key(library_root: impl AsRef<Path>, module: &str, key: &str) -> Result<()>;

```

## `src/install/transport.rs`

```rust
/// One entry of a remote directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    /// Size in bytes if the listing reports it.
    pub size: Option<u64>,
}

/// How the installer talks to the outside world.
pub trait Transport: Send + Sync {
    /// Download a whole file. Must return [`Error::NotFound`] for missing files
    /// (HTTP 404, FTP 550) so the installer can fall back to other locations.
    fn get(&self, url: &str) -> Result<Vec<u8>>;

    /// List a directory (`url` ends with `/`). Only needed for repositories that
    /// lack `packages/<Module>.zip`. The default parses an HTML index page (the
    /// way libsword does for HTTP repositories).
    fn list(&self, url: &str) -> Result<Vec<DirEntry>> {
        let html = self.get(url)?;
        Ok(parse_html_listing(&String::from_utf8_lossy(&html), url))
    }
}

/// Built-in transport: HTTP(S) via `ureq` (feature `https`), FTP, `file://`.
pub struct DefaultTransport { /* private fields */ }

impl Default for DefaultTransport { … }

impl DefaultTransport {
    pub fn new() -> Self;

    /// Per-connection timeout for connecting and reading.
    pub fn with_timeout(timeout: std::time::Duration) -> Self;

impl Transport for DefaultTransport { … }
```

## `src/lib.rs`

```rust
pub mod conf;

pub mod encoding;

#[cfg(feature = "install")]
pub mod install;

pub mod versification;

pub use conf::{BlockType, CompressType, Markup, ModDrv, ModuleConf, ModuleKind};

pub use drivers::genbook::TreeNode;

pub use normalize::{normalize_entry, strongs_pad};

pub use error::{Error, Result};

pub use library::SwordLibrary;

pub use files::{FileReader, Files, FsFiles, MemoryFiles};

pub use module::{DictionaryModule, Entry, EntryLocation, GenBookModule, Module, VerseModule, VerseSpan};

pub use versification::{bible_data, IndexPos, Testament, VerseKey, Versification};

// No markup: the crate depends on no markup crate and parses no entry markup. An entry is its
// `raw` text and its `Markup` (the conf's `SourceType`); callers parse it themselves. Removed
// with the markup dependency (October 2026): the `markup` re-export of topos-bible-markup and
// its `Filter`, `HtmlRenderer`, `Kind`, `Node`, `Strongs`, `StrongsLang`, `TextOptions`;
// `Entry::nodes`, `plain_text`, `plain_text_with`, `html` and `strongs`; and
// `DictionaryModule::strongs_language` and `strongs_at` (a caller reads the language from
// `conf().features()` and the key with `key_at`, and parses it). `get_strongs` takes the number
// as a string (`G25`). Kept, since they use only topos-bible (`Passage`, `BookId`), which isn't
// markup: `VerseModule::iter_spans` and `location`, `EntryLocation`, `VerseSpan` with
// `passage` and `osis_ref`, and `DictionaryModule::link_at`.

/// The verse reference library: book data, passages and their parsing.
pub use topos_bible as topos;

pub use topos_bible::data::books::BookId;

pub use topos_bible::segments::Passage;

/// Not part of the API: exposes the LZSS decoder to the differential test harness.
#[doc(hidden)]
pub fn __lzss_decode_for_tests(data: &[u8]) -> Vec<u8>;
```

## `src/library.rs`

```rust
/// The modules found under one or more library roots.
#[derive(Debug, Clone, Default)]
pub struct SwordLibrary { /* private fields */ }

impl SwordLibrary {
    /// Scan `root/mods.d/*.conf`. Sections without `ModDrv` (e.g. `[Globals]`) are ignored.
    pub fn open(root: impl AsRef<Path>) -> Result<SwordLibrary>;

    /// Add another root (later roots do not replace modules already found).
    pub fn add_root(&mut self, root: impl AsRef<Path>) -> Result<()>;

    /// Register a module conf whose data lives under `root`.
    pub fn add_module(&mut self, root: impl Into<PathBuf>, conf: ModuleConf);

    /// Scan `mods.d/*.conf` in `files` (as [`open`](Self::open) does a folder), e.g. a
    /// module zip unpacked into [`MemoryFiles`].
    pub fn open_files(files: Arc<dyn Files>) -> Result<SwordLibrary>;

    /// Add another library's files (later ones do not replace modules already found).
    pub fn add_files(&mut self, files: Arc<dyn Files>) -> Result<()>;

    /// Register a module conf whose data is in `files`.
    pub fn add_module_files(&mut self, files: Arc<dyn Files>, conf: ModuleConf);

    /// All module confs.
    pub fn modules(&self) -> impl Iterator<Item = &ModuleConf>;

    /// Module confs of one kind.
    pub fn modules_of_kind(&self, kind: ModuleKind) -> impl Iterator<Item = &ModuleConf>;

    /// Conf by module name (case-insensitive).
    pub fn conf(&self, name: &str) -> Option<&ModuleConf>;

    /// Mutable conf by name — e.g. to set a `CipherKey` before opening.
    pub fn conf_mut(&mut self, name: &str) -> Option<&mut ModuleConf>;

    /// Open a module by name.
    pub fn open_module(&self, name: &str) -> Result<Module>;

    /// Open an enciphered module with a key (without storing the key).
    pub fn open_module_with_key(&self, name: &str, cipher_key: &str) -> Result<Module>;
```

## `src/lzss.rs`

```rust
```

## `src/module.rs`

```rust
/// The text of one entry plus the markup needed to interpret it. The crate doesn't parse
/// markup: callers parse `raw` as `markup` says (no methods; plain data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Entry text after deciphering, whitespace normalisation (`docs/spec/entry-text.md`) and
    /// decoding to UTF-8 — markup still included.
    pub raw: String,
    /// The conf's `SourceType`, or for a conf without one, the markup the text looks like
    /// (`docs/spec/gaps.md`).
    pub markup: Markup,
}

/// Any opened module.
pub enum Module {
    Verse(VerseModule),
    Dictionary(DictionaryModule),
    GenBook(GenBookModule),
}

impl Module {
    /// Open a module whose `DataPath` is relative to `library_root` (the directory
    /// containing `mods.d/` and `modules/`).
    pub fn open(library_root: impl AsRef<Path>, conf: ModuleConf) -> Result<Module>;

    /// Open a module whose `DataPath` is relative to the root of `files`.
    pub fn open_files(files: Arc<dyn Files>, conf: ModuleConf) -> Result<Module>;

    pub fn conf(&self) -> &ModuleConf;

    pub fn name(&self) -> &str;

    pub fn kind(&self) -> ModuleKind;

    pub fn markup(&self) -> Markup;

    pub fn as_verse(&self) -> Option<&VerseModule>;

    pub fn as_dictionary(&self) -> Option<&DictionaryModule>;

    pub fn as_genbook(&self) -> Option<&GenBookModule>;

    /// Convert into a verse module, or fail with [`Error::WrongModuleType`].
    pub fn into_verse(self) -> Result<VerseModule>;

    pub fn into_dictionary(self) -> Result<DictionaryModule>;

    pub fn into_genbook(self) -> Result<GenBookModule>;

/// A Bible or commentary, keyed by verse.
pub struct VerseModule { /* private fields */ }

impl VerseModule {
    /// Whether this is a `RawFiles` personal-notes commentary (read-only here).
    pub fn is_personal_notes(&self) -> bool;

    pub fn conf(&self) -> &ModuleConf;

    pub fn name(&self) -> &str;

    pub fn markup(&self) -> Markup;

    /// The module's versification system.
    pub fn versification(&self) -> &'static Versification;

    /// Whether the module has data for this testament (many are NT- or OT-only).
    pub fn has_testament(&self, t: Testament) -> bool;

    /// Text at a verse (chapter 0 / verse 0 address introductions).
    /// `Ok(None)` if the reference is not in the versification or has no text.
    pub fn verse(&self, key: &VerseKey) -> Result<Option<Entry>>;

    /// Text at a raw index position.
    pub fn at(&self, pos: IndexPos) -> Result<Option<Entry>>;

    /// Every verse of a passage that has text, in the passage's order, using this
    /// module's versification for whole chapters (`Ps 9` is longer in the LXX than in
    /// the KJV). Introductions are only included when named (`John 3:0`); see
    /// [`chapter_intro`](Self::chapter_intro) and [`book_intro`](Self::book_intro).
    ///
    /// ```no_run
    /// # let lib = sword_modules::SwordLibrary::open("/usr/share/sword")?;
    /// let kjv = lib.open_module("KJV")?.into_verse()?;
    /// let passage = sword_modules::bible_data().parse("Jn 3:16-18; 5").expect("a reference");
    /// for (key, entry) in kjv.passage(&passage)? {
    ///     println!("{key}: {}", entry.raw);
    /// }
    /// # Ok::<(), sword_modules::Error>(())
    /// ```
    pub fn passage(&self, passage: &Passage) -> Result<Vec<(VerseKey, Entry)>>;

    /// The book's introduction (chapter 0, verse 0), if the module has one.
    pub fn book_intro(&self, book: BookId) -> Result<Option<Entry>>;

    /// A chapter's introduction (verse 0), if the module has one.
    pub fn chapter_intro(&self, book: BookId, chapter: u8) -> Result<Option<Entry>>;

    /// All verses of a chapter that have text (verse 0 = chapter introduction,
    /// included when present), in order.
    pub fn chapter(&self, book: BookId, chapter: u8) -> Result<Vec<(VerseKey, Entry)>>;

    /// Whether any verse of the book has text.
    pub fn has_book(&self, book: BookId) -> Result<bool>;

    /// Iterate every non-empty entry (book/chapter intros and verses) in canonical
    /// order. Module and testament headings are skipped; read them with
    /// [`at`](Self::at) at slots 0 and 1 if needed.
    pub fn iter(&self) -> impl Iterator<Item = Result<(VerseKey, Entry)>> + '_;

    /// Where the entry at a verse is stored, or `None` when the verse is not in the
    /// versification or its index entry is empty (a zero length). Two verses share one
    /// stored entry (are linked, `docs/spec/formats.md`) exactly when their locations
    /// are equal.
    pub fn location(&self, key: &VerseKey) -> Option<EntryLocation>;

    /// Like [`iter`](Self::iter), but each run of linked verses is one item: a
    /// [`VerseSpan`] from its first verse to its last, read once. A run is made of
    /// slots in index order whose locations are equal, with these rules:
    ///
    /// * It never leaves a book: a link from John 21:25 to Acts 1:1 gives two spans
    ///   with the same location.
    /// * Within a book, empty slots that are chapter introductions (verse 0, a zero
    ///   length) are passed over, so `osis2mod`'s John 3:36 + 4:1-2 (with an empty
    ///   John 4:0) is one span, John 3:36 to John 4:2, as is `imp2vs`'s John 6:71-7:1
    ///   (whose John 7:0 is linked too). A chapter introduction with other text, or
    ///   any verse (verse 1 or more) with other text or none, ends the run.
    /// * Slots that share an entry but are not in one run (John 3:16 and 3:18 with
    ///   3:17 between them holding other text or none) are separate spans with the
    ///   same location; callers that want to treat them as one entry compare
    ///   locations.
    /// * A span can start at an introduction (John 7:0 linked to 7:1 is the span
    ///   John 7:0 to John 7:1); slots 0 and 1 (module and testament headings) are
    ///   skipped, as by [`iter`](Self::iter).
    /// * A span whose entry reads as empty (after normalisation) is skipped, as
    ///   [`iter`](Self::iter) skips such a verse.
    ///
    /// Bibles are read the same way (some link verses a translation joins).
    /// Personal notes compare the index entries that name note files.
    ///
    /// ```no_run
    /// # let lib = sword_modules::SwordLibrary::open("/usr/share/sword")?;
    /// let mhc = lib.open_module("MHC")?.into_verse()?;
    /// for span in mhc.iter_spans() {
    ///     let span = span?;
    ///     println!("{}: {}", span.osis_ref().unwrap_or_default(), span.entry.raw);
    /// }
    /// # Ok::<(), sword_modules::Error>(())
    /// ```
    pub fn iter_spans(&self) -> impl Iterator<Item = Result<VerseSpan>> + '_;

/// Where a verse module's entry is stored: its testament and its index entry (offset
/// and length; for compressed modules block, offset in the block and length). Opaque,
/// for comparing: equal locations are one stored entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryLocation { /* private fields */ }

/// A run of linked verses sharing one entry ([`VerseModule::iter_spans`]): from
/// `first` to `last` (equal for a verse that is not linked), always in one book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerseSpan {
    pub first: VerseKey,
    pub last: VerseKey,
    /// Where the entry is stored; other spans with the same location share it.
    pub location: EntryLocation,
    pub entry: Entry,
}

impl VerseSpan {
    /// Whether the span covers more than one slot.
    pub fn is_linked(&self) -> bool;

    /// The span as a topos passage (`John 3:36-4:2`); a span starting at an
    /// introduction starts at verse 0.
    pub fn passage(&self) -> Passage;

    /// OSIS reference: `John.3.16` for one verse, `John.3.36-John.4.2` for a span.
    pub fn osis_ref(&self) -> Option<String>;

/// A lexicon / dictionary / glossary / daily devotional, keyed by strings.
pub struct DictionaryModule { /* private fields */ }

impl DictionaryModule {
    pub fn conf(&self) -> &ModuleConf;

    pub fn name(&self) -> &str;

    pub fn markup(&self) -> Markup;

    /// Number of keys.
    pub fn len(&self) -> usize;

    pub fn is_empty(&self) -> bool;

    /// Key at position `i` (sorted order), as stored.
    pub fn key_at(&self, i: usize) -> Result<String>;

    /// All keys in sorted order.
    pub fn keys(&self) -> Result<Vec<String>>;

    /// Entry for an exact key. Keys are matched like libsword: upper-cased unless
    /// `CaseSensitiveKeys=true`, and Strong's-style keys are zero-padded
    /// (`"25"` → `"00025"`, `"G25"` → `"G0025"`). `@LINK` aliases are followed.
    ///
    /// Many Strong's lexicons store keys without the language prefix (`00025`),
    /// so when a prefixed key like `G25` misses, the bare number is tried too.
    pub fn get(&self, key: &str) -> Result<Option<Entry>>;

    /// Look up a Strong's number in a StrongsGreek/StrongsHebrew-style module,
    /// trying both the plain padded form (`00025`) and the prefixed one (`G0025`).
    /// The number is a string: `G` or `H` (either case), digits, and an optional suffix
    /// of letters (`G25`, `H0430a`); leading and trailing whitespace is ignored. Any
    /// other string is `Ok(None)`. The keys tried, and their order, are in
    /// `docs/spec/gaps.md`.
    pub fn get_strongs(&self, number: &str) -> Result<Option<Entry>>;

    /// The first entry whose key sorts at or after `key` (for "type-ahead"
    /// navigation). Returns the key found and its entry.
    pub fn get_nearest(&self, key: &str) -> Result<Option<(String, Entry)>>;

    /// Entry at sorted position `i`.
    pub fn entry_at(&self, i: usize) -> Result<Option<Entry>>;

    /// Iterate `(key, entry)` pairs in key order.
    pub fn iter(&self) -> impl Iterator<Item = Result<(String, Entry)>> + '_;

    /// The key the entry at sorted position `i` names with `@LINK` (the first link
    /// only, as stored: in `.dat`, or for zLD in the entry text), or `None` when the
    /// entry is not a link. [`iter`](Self::iter) and [`entry_at`](Self::entry_at)
    /// follow links, so an alias reads as its target's text; this tells an alias
    /// from an entry of its own.
    pub fn link_at(&self, i: usize) -> Result<Option<String>>;

/// A general book with a tree-shaped table of contents.
pub struct GenBookModule { /* private fields */ }

impl GenBookModule {
    pub fn conf(&self) -> &ModuleConf;

    pub fn name(&self) -> &str;

    pub fn markup(&self) -> Markup;

    /// The root node (usually unnamed and without text).
    pub fn root(&self) -> Result<TreeNode>;

    pub fn children(&self, node: &TreeNode) -> Result<Vec<TreeNode>>;

    pub fn parent(&self, node: &TreeNode) -> Result<Option<TreeNode>>;

    /// Node by id (its `offset`).
    pub fn node(&self, offset: u32) -> Result<TreeNode>;

    /// Find a node by `/`-separated path such as `/Book 1/Chapter 3`.
    /// Segment matching is exact first, then case-insensitive.
    pub fn find(&self, path: &str) -> Result<Option<TreeNode>>;

    /// Full `/`-separated path of a node.
    pub fn path(&self, node: &TreeNode) -> Result<String>;

    /// Text of a node.
    pub fn text(&self, node: &TreeNode) -> Result<Option<Entry>>;

    /// Text by path.
    pub fn get(&self, path: &str) -> Result<Option<Entry>>;

    /// All nodes in document order (depth-first, pre-order), with their depth.
    pub fn walk(&self) -> Result<Vec<(usize, TreeNode)>>;
```

## `src/normalize.rs`

```rust
/// Normalise raw entry bytes as SWORD applications see them: line breaks are
/// folded (a single line break becomes a space, further ones become newlines),
/// leading line breaks and trailing spaces/newlines are removed, and the text ends
/// at the first NUL. See `docs/spec/entry-text.md` for the exact rules.
pub fn normalize_entry(input: &[u8]) -> Vec<u8>;

/// Zero-pad Strong's-number dictionary keys the way SWORD stores them:
/// `"25"` → `"00025"`, `"G25"` → `"G0025"`, `"h430a"` → `"h0430A"`,
/// `"3056!b"` → `"03056!B"`. Keys that do not look like Strong's numbers are
/// returned unchanged. See `docs/spec/strongs-keys.md`.
pub fn strongs_pad(key: &str) -> String;
```

## `src/versification/mod.rs`

```rust
/// The topos book data this crate names books with: English names and abbreviations, OSIS
/// ids, and the deuterocanon ([`BibleData::with_deuterocanon`]). Parse references with it:
///
/// ```
/// let passage = sword_modules::bible_data().parse("Sir 24:3-5").unwrap();
/// assert_eq!(passage.to_osis(sword_modules::bible_data().books()).unwrap(), "Sir.24.3-Sir.24.5");
/// ```
pub fn bible_data() -> &'static BibleData;

/// Static description of one book in a versification system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookDef {
    /// English name as SWORD reports it (e.g. `I Samuel`).
    pub name: &'static str,
    /// OSIS book id (e.g. `1Sam`).
    pub osis: &'static str,
    /// Preferred abbreviation.
    pub abbrev: &'static str,
    /// The book's id in [`bible_data`].
    pub id: BookId,
    /// Number of chapters.
    pub chapters: u8,
}

/// Old or New Testament (selects the `ot*` / `nt*` data files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Testament {
    Old,
    New,
}

/// One position in a verse-keyed module: a book, chapter and verse.
///
/// Chapter 0 addresses the book introduction and verse 0 the chapter introduction
/// (both used by commentaries and by Bibles for headings). Parse references with
/// [`bible_data`] into a topos [`Passage`] and read it with
/// [`VerseModule::passage`](crate::VerseModule::passage), or build keys directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VerseKey {
    /// The book, numbered as in [`bible_data`].
    pub book: BookId,
    pub chapter: u8,
    pub verse: u8,
}

impl VerseKey {
    pub fn new(book: BookId, chapter: u8, verse: u8) -> Self;

    /// A key for an OSIS book id (`John`, `1Sam`, `Tob`; any case), or `None` for an
    /// unknown book.
    pub fn from_osis(book: &str, chapter: u8, verse: u8) -> Option<Self>;

    /// The OSIS book id, e.g. `John`.
    pub fn osis_book(&self) -> Option<&'static str>;

    /// OSIS reference string, e.g. `John.3.16`.
    pub fn osis_ref(&self) -> Option<String>;

    pub fn chapter_verse(&self) -> ChapterVerse;

    /// This one verse as a topos passage.
    pub fn passage(&self) -> Passage;

impl fmt::Display for VerseKey { … }

/// A position in a module's index: testament plus slot number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IndexPos {
    pub testament: Testament,
    pub slot: u32,
}

/// A versification system (KJV, NRSVA, Synodal, …).
#[derive(Debug)]
pub struct Versification { /* private fields */ }

impl Versification {
    /// All built-in systems.
    pub fn all() -> &'static [Versification];

    /// Look up a system by name (case-insensitive), e.g. `"KJV"`, `"NRSVA"`.
    pub fn get(name: &str) -> Option<&'static Versification>;

    /// The KJV system (SWORD's default).
    pub fn kjv() -> &'static Versification;

    pub fn name(&self) -> &'static str;

    /// Books in canonical order (OT then NT).
    pub fn books(&self) -> &[BookDef];

    /// Index of a book in [`books`](Self::books).
    pub fn book_index(&self, book: BookId) -> Option<usize>;

    /// Book definition by id.
    pub fn book(&self, book: BookId) -> Option<&BookDef>;

    /// Book definition by OSIS id (case-insensitive).
    pub fn book_by_osis(&self, osis: &str) -> Option<&BookDef>;

    pub fn testament_of(&self, book_index: usize) -> Testament;

    /// Number of chapters in a book.
    pub fn chapter_count(&self, book: BookId) -> Option<u8>;

    /// Number of verses in a chapter (1-based chapter).
    pub fn verse_count(&self, book: BookId, chapter: u8) -> Option<u8>;

    /// The book's verse counts as topos versification, for expanding passages
    /// ([`Passage::verses`]) and comparing them.
    pub fn chapter_verses(&self, book: BookId) -> Option<ChapterVerses>;

    /// Does this system contain the reference (allowing chapter 0 / verse 0 intros)?
    pub fn contains(&self, key: &VerseKey) -> bool;

    /// Map a reference to its slot in the module index.
    pub fn index_of(&self, key: &VerseKey) -> Option<IndexPos>;

    /// Reverse of [`index_of`](Self::index_of). Slots 0 and 1 (module and testament
    /// headings) have no `VerseKey` and return `None`.
    pub fn key_at(&self, pos: IndexPos) -> Option<VerseKey>;

    /// Number of slots in a testament's index.
    pub fn slot_count(&self, t: Testament) -> u32;

    /// Iterate all verses (verse ≥ 1) in canonical order.
    pub fn verses(&self) -> impl Iterator<Item = VerseKey> + '_;
```

