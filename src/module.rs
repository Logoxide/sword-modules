//! Opened modules: Bibles and commentaries, dictionaries, and general books.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use topos_bible::data::books::BookId;
use topos_bible::segments::Passage;

use crate::conf::{Markup, ModDrv, ModuleConf, ModuleKind};
use crate::drivers::genbook::{TreeNode, TreeStore};
use crate::drivers::strkey::{link_target, DictStore};
use crate::drivers::verse::{Layout, VerseStore};
use crate::encoding::{decode_text, TextEncoding};
use crate::error::{Error, Result};
use crate::markup::{self, HtmlRenderer, Node, Strongs, StrongsLang, TextOptions};
use crate::normalize::{normalize_entry, strongs_pad};
use crate::versification::{IndexPos, Testament, VerseKey, Versification};

/// The text of one entry plus the markup needed to interpret it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Entry text after deciphering, whitespace normalisation (`docs/spec/entry-text.md`) and
    /// decoding to UTF-8 — markup still included.
    pub raw: String,
    pub markup: Markup,
}

impl Entry {
    /// Parse into a node tree.
    pub fn nodes(&self) -> Vec<Node> {
        markup::parse(self.markup, &self.raw)
    }

    /// Plain text with default [`TextOptions`] (no notes, titles or metadata).
    pub fn plain_text(&self) -> String {
        self.plain_text_with(&TextOptions::default())
    }

    pub fn plain_text_with(&self, opts: &TextOptions) -> String {
        markup::plain_text(&self.nodes(), opts)
    }

    /// Stand-alone HTML. Use one [`HtmlRenderer`] across a passage instead to
    /// carry red-letter state between verses.
    pub fn html(&self) -> String {
        HtmlRenderer::new().render(&self.nodes())
    }

    /// Strong's numbers in reading order.
    pub fn strongs(&self) -> Vec<Strongs> {
        markup::strongs_numbers(&self.nodes())
    }
}

/// Turns stored bytes into [`Entry`]s for one module.
#[derive(Debug, Clone)]
struct TextReader {
    encoding: TextEncoding,
    /// No `Encoding` in the conf: valid UTF-8 is read as UTF-8.
    sniff_utf8: bool,
    markup: Markup,
    /// No `SourceType` in the conf: entries with XML end tags are not GBF.
    sniff_markup: bool,
}

impl TextReader {
    fn new(conf: &ModuleConf) -> TextReader {
        TextReader { encoding: conf.encoding(), sniff_utf8: conf.get("Encoding").is_none(), markup: conf.source_type(), sniff_markup: conf.get("SourceType").is_none() }
    }

    fn decode(&self, bytes: &[u8]) -> String {
        if self.sniff_utf8 {
            if let Ok(s) = std::str::from_utf8(bytes) {
                return s.to_string();
            }
        }
        decode_text(bytes, self.encoding)
    }

    /// Normalise (unless `raw`), decode, and wrap; empty text is no entry.
    fn entry(&self, bytes: &[u8], raw: bool) -> Option<Entry> {
        let text = if raw {
            self.decode(bytes)
        } else if self.encoding == TextEncoding::Utf16 && !self.sniff_utf8 {
            // UTF-16 text is full of NUL bytes, so it is normalised after decoding.
            let decoded = self.decode(bytes);
            String::from_utf8_lossy(&normalize_entry(decoded.as_bytes())).into_owned()
        } else {
            self.decode(&normalize_entry(bytes))
        };
        if text.is_empty() {
            return None;
        }
        let markup = if self.sniff_markup && (text.contains("</") || text.contains("/>")) { guess_markup(&text) } else { self.markup };
        Some(Entry { raw: text, markup })
    }
}

/// The XML markup an entry most likely uses when the conf names none.
fn guess_markup(text: &str) -> Markup {
    const TEI: [&str; 4] = ["<entryFree", "<orth", "<def>", "<superentry"];
    const OSIS: [&str; 9] = ["osisID", "osisRef", "<w ", "<transChange", "<divineName", "<verse", "<note", "<q ", "<l>"];
    if TEI.iter().any(|t| text.contains(t)) {
        Markup::Tei
    } else if OSIS.iter().any(|t| text.contains(t)) {
        Markup::Osis
    } else {
        Markup::Thml
    }
}

fn wrong_type(expected: &'static str, found: ModuleKind) -> Error {
    Error::WrongModuleType { expected, found: found.as_str() }
}

/// Where a module's data is: `DataPath` under the library root.
fn data_location(library_root: &Path, conf: &ModuleConf) -> Result<PathBuf> {
    let path = conf.data_path().filter(|p| !p.is_empty()).ok_or_else(|| Error::Conf(format!("{}: no DataPath", conf.name())))?;
    Ok(library_root.join(path.trim_end_matches('/')))
}

/// The prepared cipher key, if the module is enciphered (and unlocked).
fn cipher_key(conf: &ModuleConf) -> Result<Option<Vec<u8>>> {
    if conf.is_locked() {
        return Err(Error::Locked(conf.name().to_string()));
    }
    Ok(conf.cipher_key().map(|k| crate::cipher::prepare_key(k.trim())))
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
    pub fn open(library_root: impl AsRef<Path>, conf: ModuleConf) -> Result<Module> {
        let root = library_root.as_ref();
        let drv = conf.mod_drv()?;
        match drv {
            ModDrv::HrefCom | ModDrv::Other(_) => Err(Error::UnsupportedDriver(conf.get("ModDrv").unwrap_or_default().to_string())),
            ModDrv::RawLD | ModDrv::RawLD4 | ModDrv::ZLD => DictionaryModule::open(root, conf, &drv).map(Module::Dictionary),
            ModDrv::RawGenBook => GenBookModule::open(root, conf).map(Module::GenBook),
            _ => VerseModule::open(root, conf, &drv).map(Module::Verse),
        }
    }

    pub fn conf(&self) -> &ModuleConf {
        match self {
            Module::Verse(m) => m.conf(),
            Module::Dictionary(m) => m.conf(),
            Module::GenBook(m) => m.conf(),
        }
    }

    pub fn name(&self) -> &str {
        self.conf().name()
    }

    pub fn kind(&self) -> ModuleKind {
        self.conf().module_kind().unwrap_or(ModuleKind::Unknown)
    }

    pub fn markup(&self) -> Markup {
        self.conf().source_type()
    }

    pub fn as_verse(&self) -> Option<&VerseModule> {
        match self {
            Module::Verse(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_dictionary(&self) -> Option<&DictionaryModule> {
        match self {
            Module::Dictionary(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_genbook(&self) -> Option<&GenBookModule> {
        match self {
            Module::GenBook(m) => Some(m),
            _ => None,
        }
    }

    /// Convert into a verse module, or fail with [`Error::WrongModuleType`].
    pub fn into_verse(self) -> Result<VerseModule> {
        match self {
            Module::Verse(m) => Ok(m),
            other => Err(wrong_type("Bible or Commentary", other.kind())),
        }
    }

    pub fn into_dictionary(self) -> Result<DictionaryModule> {
        match self {
            Module::Dictionary(m) => Ok(m),
            other => Err(wrong_type("Dictionary", other.kind())),
        }
    }

    pub fn into_genbook(self) -> Result<GenBookModule> {
        match self {
            Module::GenBook(m) => Ok(m),
            other => Err(wrong_type("GenBook", other.kind())),
        }
    }
}

/// A Bible or commentary, keyed by verse.
pub struct VerseModule {
    conf: ModuleConf,
    reader: TextReader,
    v11n: &'static Versification,
    store: VerseStore,
    personal: bool,
}

impl VerseModule {
    fn open(root: &Path, conf: ModuleConf, drv: &ModDrv) -> Result<VerseModule> {
        let dir = data_location(root, &conf)?;
        let personal = *drv == ModDrv::RawFiles;
        let v11n = if personal {
            // Personal notes always use the KJV versification.
            Versification::kjv()
        } else {
            Versification::get(conf.versification()).ok_or_else(|| Error::UnknownVersification(conf.versification().to_string()))?
        };
        let layout = match drv {
            ModDrv::RawText | ModDrv::RawCom => Layout::Raw { wide: false },
            ModDrv::RawText4 | ModDrv::RawCom4 => Layout::Raw { wide: true },
            ModDrv::ZText | ModDrv::ZCom | ModDrv::ZText4 | ModDrv::ZCom4 => {
                let compress = conf.compress_type();
                crate::compress::check_supported(&compress)?;
                Layout::Compressed { wide: matches!(drv, ModDrv::ZText4 | ModDrv::ZCom4), block: conf.block_type(), compress }
            }
            ModDrv::RawFiles => Layout::Files,
            other => return Err(Error::UnsupportedDriver(format!("{other:?}"))),
        };
        let cipher = if personal { None } else { cipher_key(&conf)? };
        let store = VerseStore::open(&dir, layout, v11n, cipher)?;
        Ok(VerseModule { reader: TextReader::new(&conf), conf, v11n, store, personal })
    }

    /// Whether this is a `RawFiles` personal-notes commentary (read-only here).
    pub fn is_personal_notes(&self) -> bool {
        self.personal
    }

    pub fn conf(&self) -> &ModuleConf {
        &self.conf
    }

    pub fn name(&self) -> &str {
        self.conf.name()
    }

    pub fn markup(&self) -> Markup {
        self.conf.source_type()
    }

    /// The module's versification system.
    pub fn versification(&self) -> &'static Versification {
        self.v11n
    }

    /// Whether the module has data for this testament (many are NT- or OT-only).
    pub fn has_testament(&self, t: Testament) -> bool {
        self.store.has_testament(t)
    }

    /// Text at a verse (chapter 0 / verse 0 address introductions).
    /// `Ok(None)` if the reference is not in the versification or has no text.
    pub fn verse(&self, key: &VerseKey) -> Result<Option<Entry>> {
        match self.v11n.index_of(key) {
            Some(pos) => self.at(pos),
            None => Ok(None),
        }
    }

    /// Text at a raw index position.
    pub fn at(&self, pos: IndexPos) -> Result<Option<Entry>> {
        Ok(self.store.entry(pos)?.and_then(|bytes| self.reader.entry(&bytes, self.personal)))
    }

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
    ///     println!("{key}: {}", entry.plain_text());
    /// }
    /// # Ok::<(), sword_modules::Error>(())
    /// ```
    pub fn passage(&self, passage: &Passage) -> Result<Vec<(VerseKey, Entry)>> {
        let Some(counts) = self.v11n.chapter_verses(passage.book) else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        for cv in passage.verses(Some(&counts)) {
            let key = VerseKey::new(passage.book, cv.chapter, cv.verse);
            if let Some(entry) = self.verse(&key)? {
                out.push((key, entry));
            }
        }
        Ok(out)
    }

    /// The book's introduction (chapter 0, verse 0), if the module has one.
    pub fn book_intro(&self, book: BookId) -> Result<Option<Entry>> {
        self.verse(&VerseKey::new(book, 0, 0))
    }

    /// A chapter's introduction (verse 0), if the module has one.
    pub fn chapter_intro(&self, book: BookId, chapter: u8) -> Result<Option<Entry>> {
        if chapter == 0 {
            return Ok(None);
        }
        self.verse(&VerseKey::new(book, chapter, 0))
    }

    /// All verses of a chapter that have text (verse 0 = chapter introduction,
    /// included when present), in order.
    pub fn chapter(&self, book: BookId, chapter: u8) -> Result<Vec<(VerseKey, Entry)>> {
        let Some(count) = self.v11n.verse_count(book, chapter) else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        for verse in 0..=count {
            let key = VerseKey::new(book, chapter, verse);
            if let Some(entry) = self.verse(&key)? {
                out.push((key, entry));
            }
        }
        Ok(out)
    }

    /// Whether any verse of the book has text.
    pub fn has_book(&self, book: BookId) -> Result<bool> {
        let Some((testament, slots)) = self.v11n.book_slot_range(book) else { return Ok(false) };
        for slot in slots {
            let pos = IndexPos { testament, slot };
            if self.store.stored_len(pos) > 0 && self.at(pos)?.is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Iterate every non-empty entry (book/chapter intros and verses) in canonical
    /// order. Module and testament headings are skipped; read them with
    /// [`at`](Self::at) at slots 0 and 1 if needed.
    pub fn iter(&self) -> impl Iterator<Item = Result<(VerseKey, Entry)>> + '_ {
        [Testament::Old, Testament::New].into_iter().filter(|&t| self.has_testament(t)).flat_map(move |testament| {
            (2..self.v11n.slot_count(testament)).filter_map(move |slot| {
                let pos = IndexPos { testament, slot };
                if self.store.stored_len(pos) == 0 {
                    return None;
                }
                let key = self.v11n.key_at(pos)?;
                match self.at(pos) {
                    Ok(Some(entry)) => Some(Ok((key, entry))),
                    Ok(None) => None,
                    Err(e) => Some(Err(e)),
                }
            })
        })
    }
}

/// How many `@LINK`s are followed before giving up (links can form loops).
const MAX_LINKS: usize = 8;

/// A lexicon / dictionary / glossary / daily devotional, keyed by strings.
pub struct DictionaryModule {
    conf: ModuleConf,
    reader: TextReader,
    store: DictStore,
    case_sensitive: bool,
    padding: bool,
}

impl DictionaryModule {
    fn open(root: &Path, conf: ModuleConf, drv: &ModDrv) -> Result<DictionaryModule> {
        let prefix = data_location(root, &conf)?;
        let compress = match drv {
            ModDrv::ZLD => {
                let c = conf.compress_type();
                crate::compress::check_supported(&c)?;
                Some(c)
            }
            _ => None,
        };
        let cipher = cipher_key(&conf)?;
        let store = DictStore::open(&prefix, *drv == ModDrv::RawLD4, compress, cipher)?;
        Ok(DictionaryModule { reader: TextReader::new(&conf), case_sensitive: conf.case_sensitive_keys(), padding: conf.strongs_padding(), conf, store })
    }

    pub fn conf(&self) -> &ModuleConf {
        &self.conf
    }

    pub fn name(&self) -> &str {
        self.conf.name()
    }

    pub fn markup(&self) -> Markup {
        self.conf.source_type()
    }

    /// Number of keys.
    pub fn len(&self) -> usize {
        self.store.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Key at position `i` (sorted order), as stored.
    pub fn key_at(&self, i: usize) -> Result<String> {
        Ok(self.reader.decode(&self.store.key(i)?))
    }

    /// All keys in sorted order.
    pub fn keys(&self) -> Result<Vec<String>> {
        (0..self.len()).map(|i| self.key_at(i)).collect()
    }

    /// A key as it is compared: padded and case-folded per the conf.
    fn fold(&self, key: &str) -> String {
        let key = if self.padding { strongs_pad(key) } else { key.to_string() };
        if self.case_sensitive {
            key
        } else {
            key.to_uppercase()
        }
    }

    /// A stored key as it is compared.
    fn fold_stored(&self, key: &str) -> String {
        if self.case_sensitive {
            key.to_string()
        } else {
            key.to_uppercase()
        }
    }

    /// The first position whose key sorts at or after the folded `key`.
    fn lower_bound(&self, folded: &str) -> Result<usize> {
        let (mut lo, mut hi) = (0, self.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.fold_stored(&self.key_at(mid)?).as_bytes() < folded.as_bytes() {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        Ok(lo)
    }

    /// The position of an exact key.
    fn find(&self, key: &str) -> Result<Option<usize>> {
        let folded = self.fold(key);
        let i = self.lower_bound(&folded)?;
        if i < self.len() && self.fold_stored(&self.key_at(i)?) == folded {
            return Ok(Some(i));
        }
        Ok(None)
    }

    /// Entry at position `i`, following `@LINK`s.
    fn resolve(&self, mut i: usize) -> Result<Option<Entry>> {
        let mut seen = HashSet::new();
        for _ in 0..MAX_LINKS {
            if !seen.insert(i) {
                return Ok(None);
            }
            let record = self.store.record(i)?;
            let text = self.store.text(record.body)?;
            let normalized = normalize_entry(&text);
            match link_target(&normalized) {
                Some(target) => {
                    let target = self.reader.decode(target);
                    match self.find(&target)? {
                        Some(next) => i = next,
                        None => return Ok(None),
                    }
                }
                None => return Ok(self.reader.entry(&text, false)),
            }
        }
        Ok(None)
    }

    /// Entry for an exact key. Keys are matched like libsword: upper-cased unless
    /// `CaseSensitiveKeys=true`, and Strong's-style keys are zero-padded
    /// (`"25"` → `"00025"`, `"G25"` → `"G0025"`). `@LINK` aliases are followed.
    ///
    /// Many Strong's lexicons store keys without the language prefix (`00025`),
    /// so when a prefixed key like `G25` misses, the bare number is tried too.
    pub fn get(&self, key: &str) -> Result<Option<Entry>> {
        if let Some(i) = self.find(key)? {
            return self.resolve(i);
        }
        if let Some(bare) = key.strip_prefix(['G', 'g', 'H', 'h']) {
            if bare.starts_with(|c: char| c.is_ascii_digit()) {
                if let Some(i) = self.find(bare)? {
                    return self.resolve(i);
                }
            }
        }
        Ok(None)
    }

    /// Look up a Strong's number in a StrongsGreek/StrongsHebrew-style module,
    /// trying both the plain padded form (`00025`) and the prefixed one (`G0025`).
    pub fn get_strongs(&self, s: &Strongs) -> Result<Option<Entry>> {
        // The keys are written out already padded, so they match whether or not the module
        // pads keys itself (`StrongsPadding`), in the order Strong's lexicons most often use:
        // the bare five-digit key (`00025`, as StrongsGreek and StrongsHebrew store them), then
        // the prefixed four-digit key (`G0025`); a number with a suffix (`H0430a`) is tried with
        // it first and then, since most lexicons have one entry per number, without it
        let letter = match s.lang {
            StrongsLang::Greek => 'G',
            StrongsLang::Hebrew => 'H',
        };
        let bare = s.dictionary_key();
        let prefixed = format!("{letter}{:04}", s.number);
        let mut keys = Vec::new();
        if let Some(suffix) = s.suffix.as_deref().filter(|x| !x.is_empty()) {
            keys.push(format!("{bare}{suffix}"));
            keys.push(format!("{prefixed}{suffix}"));
        }
        keys.push(bare);
        keys.push(prefixed);
        for key in &keys {
            if let Some(i) = self.find(key)? {
                if let Some(entry) = self.resolve(i)? {
                    return Ok(Some(entry));
                }
            }
        }
        Ok(None)
    }

    /// The first entry whose key sorts at or after `key` (for "type-ahead"
    /// navigation). Returns the key found and its entry.
    pub fn get_nearest(&self, key: &str) -> Result<Option<(String, Entry)>> {
        let mut i = self.lower_bound(&self.fold(key))?;
        while i < self.len() {
            if let Some(entry) = self.resolve(i)? {
                return Ok(Some((self.key_at(i)?, entry)));
            }
            i += 1;
        }
        Ok(None)
    }

    /// Entry at sorted position `i`.
    pub fn entry_at(&self, i: usize) -> Result<Option<Entry>> {
        if i >= self.len() {
            return Ok(None);
        }
        self.resolve(i)
    }

    /// Iterate `(key, entry)` pairs in key order.
    pub fn iter(&self) -> impl Iterator<Item = Result<(String, Entry)>> + '_ {
        (0..self.len()).filter_map(move |i| match (self.key_at(i), self.entry_at(i)) {
            (Ok(key), Ok(Some(entry))) => Some(Ok((key, entry))),
            (_, Ok(None)) => None,
            (Err(e), _) | (_, Err(e)) => Some(Err(e)),
        })
    }
}

/// A general book with a tree-shaped table of contents.
pub struct GenBookModule {
    conf: ModuleConf,
    reader: TextReader,
    store: TreeStore,
}

impl GenBookModule {
    fn open(root: &Path, conf: ModuleConf) -> Result<GenBookModule> {
        if !conf.key_type().eq_ignore_ascii_case("TreeKey") {
            return Err(Error::UnsupportedDriver(format!("RawGenBook with KeyType={}", conf.key_type())));
        }
        let prefix = data_location(root, &conf)?;
        let store = TreeStore::open(&prefix, cipher_key(&conf)?)?;
        Ok(GenBookModule { reader: TextReader::new(&conf), conf, store })
    }

    pub fn conf(&self) -> &ModuleConf {
        &self.conf
    }

    pub fn name(&self) -> &str {
        self.conf.name()
    }

    pub fn markup(&self) -> Markup {
        self.conf.source_type()
    }

    /// The root node (usually unnamed and without text).
    pub fn root(&self) -> Result<TreeNode> {
        self.node(0)
    }

    pub fn children(&self, node: &TreeNode) -> Result<Vec<TreeNode>> {
        let mut out = Vec::new();
        let mut next = node.first_child;
        let mut seen = HashSet::new();
        while let Some(offset) = next {
            if !seen.insert(offset) || out.len() > self.store.node_count() {
                return Err(Error::Corrupt(format!("{}: sibling links loop", self.name())));
            }
            let child = self.node(offset)?;
            next = child.next_sibling;
            out.push(child);
        }
        Ok(out)
    }

    pub fn parent(&self, node: &TreeNode) -> Result<Option<TreeNode>> {
        node.parent.map(|p| self.node(p)).transpose()
    }

    /// Node by id (its `offset`).
    pub fn node(&self, offset: u32) -> Result<TreeNode> {
        let (mut node, name) = self.store.node_raw(offset)?;
        node.name = self.reader.decode(&name);
        Ok(node)
    }

    /// Find a node by `/`-separated path such as `/Book 1/Chapter 3`.
    /// Segment matching is exact first, then case-insensitive.
    pub fn find(&self, path: &str) -> Result<Option<TreeNode>> {
        let mut node = self.root()?;
        for segment in path.split('/').filter(|s| !s.is_empty()) {
            let children = self.children(&node)?;
            let found = match children.iter().position(|c| c.name == segment) {
                Some(i) => Some(i),
                None => {
                    let lower = segment.to_lowercase();
                    children.iter().position(|c| c.name.to_lowercase() == lower)
                }
            };
            match found {
                Some(i) => node = children.into_iter().nth(i).ok_or_else(|| Error::Corrupt("child vanished".into()))?,
                None => return Ok(None),
            }
        }
        Ok(Some(node))
    }

    /// Full `/`-separated path of a node.
    pub fn path(&self, node: &TreeNode) -> Result<String> {
        let mut names = Vec::new();
        let mut current = node.clone();
        let mut seen = HashSet::new();
        while let Some(parent) = current.parent {
            if !seen.insert(current.offset) {
                return Err(Error::Corrupt(format!("{}: parent links loop", self.name())));
            }
            names.push(current.name.clone());
            current = self.node(parent)?;
        }
        names.reverse();
        Ok(format!("/{}", names.join("/")))
    }

    /// Text of a node.
    pub fn text(&self, node: &TreeNode) -> Result<Option<Entry>> {
        Ok(self.store.text(node)?.and_then(|bytes| self.reader.entry(&bytes, false)))
    }

    /// Text by path.
    pub fn get(&self, path: &str) -> Result<Option<Entry>> {
        match self.find(path)? {
            Some(node) => self.text(&node),
            None => Ok(None),
        }
    }

    /// All nodes in document order (depth-first, pre-order), with their depth.
    pub fn walk(&self) -> Result<Vec<(usize, TreeNode)>> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut stack = vec![(0usize, self.root()?)];
        while let Some((depth, node)) = stack.pop() {
            if !seen.insert(node.offset) {
                return Err(Error::Corrupt(format!("{}: the tree has a loop", self.name())));
            }
            let children = self.children(&node)?;
            stack.extend(children.into_iter().rev().map(|c| (depth + 1, c)));
            out.push((depth, node));
        }
        Ok(out)
    }
}
