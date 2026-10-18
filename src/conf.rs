//! Module `.conf` files (rules in `docs/spec/conf.md`) and the values read from them.

use std::path::Path;

use crate::encoding::{decode_cp1252, TextEncoding};
use crate::error::{Error, Result};

/// Markup used inside entries: the conf's `SourceType`. This crate only names it;
/// parsing and rendering entries is left to the caller, which reads
/// [`Entry::raw`](crate::Entry) with it.
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
    const NAMES: [(&'static str, Markup); 5] = [("OSIS", Markup::Osis), ("ThML", Markup::Thml), ("GBF", Markup::Gbf), ("TEI", Markup::Tei), ("Plaintext", Markup::Plaintext)];

    /// A markup by its `SourceType` name, ignoring case: `OSIS`, `ThML`, `GBF`, `TEI`,
    /// `Plaintext` or `Plain`. `None` for any other name.
    pub fn from_name(name: &str) -> Option<Markup> {
        if name.eq_ignore_ascii_case("plain") {
            return Some(Markup::Plaintext);
        }
        Self::NAMES.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, m)| m)
    }

    /// The `SourceType` name as confs spell it: `OSIS`, `ThML`, `GBF`, `TEI`, `Plaintext`.
    pub fn as_str(self) -> &'static str {
        Self::NAMES.iter().find(|&&(_, m)| m == self).map_or("Plaintext", |(n, _)| n)
    }
}

/// One `[Section]` of a conf file: a module name and its ordered key/value entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleConf {
    name: String,
    entries: Vec<(String, String)>,
}

/// Whitespace as the conf rules mean it: ASCII blanks and line ends.
fn is_blank(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{0B}' | '\u{0C}')
}

/// Join physical lines into logical lines (continuations with a trailing `\`).
fn logical_lines(text: &str) -> Vec<String> {
    let physical: Vec<&str> = text.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < physical.len() {
        let mut line = String::new();
        // Only the physical line just taken decides whether the logical line goes on.
        let mut current = physical[i].trim_start_matches(is_blank);
        i += 1;
        loop {
            current = current.trim_end_matches(is_blank);
            // A backslash that begins its physical line is ordinary text.
            let Some(body) = current.strip_suffix('\\').filter(|b| !b.is_empty()) else {
                line.push_str(current);
                break;
            };
            line.push_str(body);
            match physical.get(i) {
                Some(next) => {
                    current = next;
                    i += 1;
                }
                None => break,
            }
        }
        out.push(line);
    }
    out
}

/// Parse all `[Section]`s in a conf text (rules in `docs/spec/conf.md`).
pub fn parse_conf(text: &str) -> Result<Vec<ModuleConf>> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut sections: Vec<ModuleConf> = Vec::new();
    for line in logical_lines(text) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let name = rest.split(']').next().unwrap_or("");
            sections.push(ModuleConf { name: name.to_string(), entries: Vec::new() });
            continue;
        }
        let Some(section) = sections.last_mut() else { continue };
        match line.split_once('=') {
            Some(("", _)) => {}
            Some((key, value)) => section.entries.push((key.to_string(), value.trim_matches(is_blank).to_string())),
            None => section.entries.push((line.clone(), String::new())),
        }
    }
    Ok(sections)
}

/// Decode a conf file's bytes: UTF-8 when valid (a byte-order mark is dropped), otherwise
/// UTF-8 with replacements if the file declares `Encoding=UTF-8`, otherwise Windows-1252.
pub(crate) fn decode_conf_bytes(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let latin = decode_cp1252(bytes);
            let declares_utf8 = latin.lines().any(|l| {
                let l = l.trim();
                l.split_once('=').is_some_and(|(k, v)| k == "Encoding" && TextEncoding::parse(v) == TextEncoding::Utf8)
            });
            if declares_utf8 {
                String::from_utf8_lossy(bytes).into_owned()
            } else {
                latin
            }
        }
    }
}

fn is_true(v: &str) -> bool {
    v.trim().eq_ignore_ascii_case("true")
}

impl ModuleConf {
    /// Parse the first module section in `text`.
    pub fn parse(text: &str) -> Result<ModuleConf> {
        parse_conf(text)?.into_iter().next().ok_or_else(|| Error::Conf("no [Section] in conf text".into()))
    }

    /// Read and parse a `.conf` file. The bytes are decoded as UTF-8 when the file
    /// declares `Encoding=UTF-8` (or is valid UTF-8), otherwise as Latin-1/CP1252.
    pub fn from_file(path: impl AsRef<Path>) -> Result<ModuleConf> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
        ModuleConf::parse(&decode_conf_bytes(&bytes)).map_err(|e| match e {
            Error::Conf(m) => Error::Conf(format!("{}: {m}", path.display())),
            other => other,
        })
    }

    /// Create a conf programmatically (handy for tests or synthetic modules).
    pub fn new(name: impl Into<String>, entries: Vec<(String, String)>) -> Self {
        ModuleConf { name: name.into(), entries }
    }

    /// The module name (the `[Section]` header), e.g. `KJV`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// All entries in file order.
    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }

    /// First value for `key` (keys are matched case-sensitively, as SWORD does).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// Every value for a repeatable key such as `GlobalOptionFilter` or `Feature`.
    pub fn get_all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.entries.iter().filter(move |(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// Value for a localized key, e.g. `get_localized("Description", "de")` looks up
    /// `Description_de` first and falls back to `Description`.
    pub fn get_localized(&self, key: &str, lang: &str) -> Option<&str> {
        self.get(&format!("{key}_{lang}")).or_else(|| self.get(key))
    }

    /// Set (replace) a value. Useful to supply a `CipherKey` for a locked module.
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        match self.entries.iter().position(|(k, _)| k == key) {
            Some(first) => {
                self.entries[first].1 = value;
                let mut index = 0;
                self.entries.retain(|(k, _)| {
                    let keep = index <= first || k != key;
                    index += 1;
                    keep
                });
            }
            None => self.entries.push((key.to_string(), value)),
        }
    }

    /// `ModDrv`: the storage driver.
    pub fn mod_drv(&self) -> Result<ModDrv> {
        self.get("ModDrv").map(ModDrv::parse).ok_or_else(|| Error::Conf(format!("{}: no ModDrv", self.name)))
    }

    /// `DataPath` relative to the library root, with any leading `./` or `/` removed.
    pub fn data_path(&self) -> Option<&str> {
        let mut p = self.get("DataPath")?;
        loop {
            if let Some(rest) = p.strip_prefix("./") {
                p = rest;
            } else if let Some(rest) = p.strip_prefix('/') {
                p = rest;
            } else {
                return Some(p);
            }
        }
    }

    /// `SourceType` — the markup of entries. SWORD defaults to GBF when absent.
    pub fn source_type(&self) -> Markup {
        match self.get("SourceType") {
            None => Markup::Gbf,
            Some(v) => Markup::from_name(v.trim()).unwrap_or(Markup::Plaintext),
        }
    }

    /// `Encoding` of entry text. SWORD defaults to Latin-1 when absent.
    pub fn encoding(&self) -> TextEncoding {
        self.get("Encoding").map(TextEncoding::parse).unwrap_or(TextEncoding::Latin1)
    }

    /// `CompressType` for z* drivers. SWORD defaults to LZSS when absent.
    pub fn compress_type(&self) -> CompressType {
        self.get("CompressType").map(CompressType::parse).unwrap_or(CompressType::Lzss)
    }

    /// `BlockType` for zText/zCom. SWORD defaults to CHAPTER.
    pub fn block_type(&self) -> BlockType {
        self.get("BlockType").map(BlockType::parse).unwrap_or(BlockType::Chapter)
    }

    /// `Versification` system name. SWORD defaults to `KJV`.
    pub fn versification(&self) -> &str {
        self.get("Versification").map(str::trim).filter(|v| !v.is_empty()).unwrap_or("KJV")
    }

    /// `CipherKey`. `Some("")` means the module is enciphered but locked.
    pub fn cipher_key(&self) -> Option<&str> {
        self.get("CipherKey")
    }

    /// True when the module is enciphered and no key has been entered.
    pub fn is_locked(&self) -> bool {
        self.cipher_key().is_some_and(|k| k.trim().is_empty())
    }

    /// `Description`.
    pub fn description(&self) -> Option<&str> {
        self.get("Description")
    }

    /// `About`, converted from SWORD's RTF subset to plain text.
    pub fn about_text(&self) -> Option<String> {
        self.get("About").map(rtf_to_text)
    }

    /// `Lang` (BCP-47-ish language code, e.g. `en`, `grc`, `he`).
    pub fn lang(&self) -> Option<&str> {
        self.get("Lang")
    }

    /// `Version` of the module.
    pub fn version(&self) -> Option<&str> {
        self.get("Version")
    }

    /// `Direction`: text direction.
    pub fn direction(&self) -> Direction {
        match self.get("Direction").map(|d| d.trim().to_ascii_lowercase()) {
            Some(d) if d == "rtol" => Direction::RightToLeft,
            Some(d) if d == "bidi" => Direction::Bidi,
            _ => Direction::LeftToRight,
        }
    }

    /// `GlobalOptionFilter` values (e.g. `OSISStrongs`, `OSISFootnotes`).
    pub fn global_option_filters(&self) -> Vec<&str> {
        self.get_all("GlobalOptionFilter").collect()
    }

    /// `Feature` values (e.g. `StrongsNumbers`, `GreekDef`, `HebrewDef`).
    pub fn features(&self) -> Vec<&str> {
        self.get_all("Feature").collect()
    }

    /// `Category` (e.g. `Biblical Texts`, `Commentaries`, `Lexicons / Dictionaries`,
    /// `Generic Books`, `Daily Devotional`, `Glossaries`, `Cults / Unorthodox / Questionable Material`).
    pub fn category(&self) -> Option<&str> {
        self.get("Category")
    }

    /// `CaseSensitiveKeys` for dictionaries (default false).
    pub fn case_sensitive_keys(&self) -> bool {
        self.get("CaseSensitiveKeys").is_some_and(is_true)
    }

    /// `StrongsPadding` for dictionaries (default true).
    pub fn strongs_padding(&self) -> bool {
        self.get("StrongsPadding").is_none_or(|v| !v.trim().eq_ignore_ascii_case("false"))
    }

    /// `KeyType` for general books (default `TreeKey`).
    pub fn key_type(&self) -> &str {
        self.get("KeyType").map(str::trim).filter(|v| !v.is_empty()).unwrap_or("TreeKey")
    }

    /// High-level module kind derived from `ModDrv`.
    pub fn module_kind(&self) -> Result<ModuleKind> {
        Ok(self.mod_drv()?.kind())
    }
}

/// Convert SWORD's RTF subset (used by `About` and similar entries) into plain text.
///
/// Handles `\par` / `\line` (newline), `\tab`, `\uN?` Unicode escapes, `\'hh` hex
/// escapes, escaped `\\ \{ \}`, drops other control words (`\pard`, `\qc`, `\b0`, …)
/// and braces.
pub fn rtf_to_text(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    // After `\uN`, the next character (or `\'hh`) is the fallback for readers without Unicode.
    let mut skip_fallback = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '{' || c == '}' {
            i += 1;
            continue;
        }
        if c != '\\' {
            if skip_fallback {
                skip_fallback = false;
            } else {
                out.push(c);
            }
            i += 1;
            continue;
        }
        // A backslash: a control symbol or a control word.
        let Some(&next) = chars.get(i + 1) else {
            i += 1;
            continue;
        };
        if next == '\'' {
            let hex: String = chars.iter().skip(i + 2).take(2).collect();
            i += 2 + hex.chars().count();
            if skip_fallback {
                skip_fallback = false;
            } else if let Ok(b) = u8::from_str_radix(&hex, 16) {
                out.push(char::from(b));
            }
            continue;
        }
        if !next.is_ascii_alphabetic() {
            // Control symbols: escaped characters, or others (`\~`, `\-`) that are dropped.
            if matches!(next, '\\' | '{' | '}') {
                out.push(next);
            }
            skip_fallback = false;
            i += 2;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len() && chars[j].is_ascii_alphabetic() {
            j += 1;
        }
        let word: String = chars[i + 1..j].iter().collect();
        let num_start = j;
        if j < chars.len() && chars[j] == '-' {
            j += 1;
        }
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
        let param: String = chars[num_start..j].iter().collect();
        // A single space ends a control word and belongs to it.
        if j < chars.len() && chars[j] == ' ' {
            j += 1;
        }
        i = j;
        skip_fallback = false;
        match word.as_str() {
            "par" | "line" => out.push('\n'),
            "tab" => out.push('\t'),
            "u" => {
                if let Ok(n) = param.parse::<i32>() {
                    let n = if n < 0 { n + 65536 } else { n };
                    if let Some(ch) = u32::try_from(n).ok().and_then(char::from_u32) {
                        out.push(ch);
                    }
                    skip_fallback = true;
                }
            }
            _ => {}
        }
    }
    out
}

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
    pub fn parse(s: &str) -> ModDrv {
        match s.trim().to_ascii_lowercase().as_str() {
            "rawtext" => ModDrv::RawText,
            "rawtext4" => ModDrv::RawText4,
            "ztext" => ModDrv::ZText,
            "ztext4" => ModDrv::ZText4,
            "rawcom" => ModDrv::RawCom,
            "rawcom4" => ModDrv::RawCom4,
            "zcom" => ModDrv::ZCom,
            "zcom4" => ModDrv::ZCom4,
            "hrefcom" => ModDrv::HrefCom,
            "rawfiles" => ModDrv::RawFiles,
            "rawld" => ModDrv::RawLD,
            "rawld4" => ModDrv::RawLD4,
            "zld" => ModDrv::ZLD,
            "rawgenbook" => ModDrv::RawGenBook,
            _ => ModDrv::Other(s.trim().to_string()),
        }
    }

    /// Which kind of module this driver stores.
    pub fn kind(&self) -> ModuleKind {
        match self {
            ModDrv::RawText | ModDrv::RawText4 | ModDrv::ZText | ModDrv::ZText4 => ModuleKind::Bible,
            ModDrv::RawCom | ModDrv::RawCom4 | ModDrv::ZCom | ModDrv::ZCom4 | ModDrv::HrefCom | ModDrv::RawFiles => ModuleKind::Commentary,
            ModDrv::RawLD | ModDrv::RawLD4 | ModDrv::ZLD => ModuleKind::Dictionary,
            ModDrv::RawGenBook => ModuleKind::GenBook,
            ModDrv::Other(_) => ModuleKind::Unknown,
        }
    }
}

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
    pub fn as_str(self) -> &'static str {
        match self {
            ModuleKind::Bible => "Bible",
            ModuleKind::Commentary => "Commentary",
            ModuleKind::Dictionary => "Dictionary",
            ModuleKind::GenBook => "GenBook",
            ModuleKind::Unknown => "Unknown",
        }
    }
}

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
    pub fn parse(s: &str) -> CompressType {
        match s.trim().to_ascii_uppercase().as_str() {
            "ZIP" => CompressType::Zip,
            "LZSS" => CompressType::Lzss,
            "BZIP2" | "BZIP" => CompressType::Bzip2,
            "XZ" => CompressType::Xz,
            _ => CompressType::Other(s.trim().to_string()),
        }
    }
}

/// Granularity of compressed blocks (`BlockType`). Determines z* index file names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    Verse,
    Chapter,
    Book,
}

impl BlockType {
    pub fn parse(s: &str) -> BlockType {
        match s.trim().to_ascii_uppercase().as_str() {
            "VERSE" => BlockType::Verse,
            "BOOK" => BlockType::Book,
            _ => BlockType::Chapter,
        }
    }

    /// The letter used in file names: `ot.bzs` (BOOK), `ot.czs` (CHAPTER), `ot.vzs` (VERSE).
    pub fn file_letter(self) -> char {
        match self {
            BlockType::Verse => 'v',
            BlockType::Chapter => 'c',
            BlockType::Book => 'b',
        }
    }
}

/// Text direction (`Direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    LeftToRight,
    RightToLeft,
    Bidi,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn about(text: &str) -> Option<String> {
        ModuleConf::parse(text).ok()?.get("About").map(str::to_string)
    }

    #[test]
    fn sections_keys_and_values() {
        let confs = parse_conf("\u{FEFF}# comment\n[KJV]\nModDrv=zText\nKey = v \nFeature=A\nFeature=B\n=ignored\nbare\n[Other]\nX=1\n").unwrap();
        assert_eq!(confs.len(), 2);
        let c = &confs[0];
        assert_eq!(c.name(), "KJV");
        assert_eq!(c.get("Key "), Some("v"));
        assert_eq!(c.get("Key"), None);
        assert_eq!(c.features(), ["A", "B"]);
        assert_eq!(c.get("bare"), Some(""));
        assert_eq!(c.mod_drv().unwrap(), ModDrv::ZText);
        assert_eq!(confs[1].get("X"), Some("1"));
    }

    #[test]
    fn continuation_lines() {
        assert_eq!(about("[A]\nAbout=one\\\n two\n").as_deref(), Some("one two"));
        assert_eq!(about("[A]\nAbout=one \\  \r\n\ttwo\\\nthree\n").as_deref(), Some("one \ttwothree"));
        // A backslash at the start of a physical line is ordinary text.
        assert_eq!(about("[A]\nAbout=x\\\n\\\nY=2\n").as_deref(), Some("x\\"));
        // Comment detection happens after joining.
        assert_eq!(ModuleConf::parse("[A]\nA=1\\\n#B=2\n").unwrap().get("A"), Some("1#B=2"));
        // A final backslash is removed.
        assert_eq!(about("[A]\nAbout=end\\").as_deref(), Some("end"));
        assert_eq!(about("[A]\n  About=\\par\\\nnext").as_deref(), Some("\\parnext"));
    }

    #[test]
    fn blank_continuation_line_ends_the_line() {
        // The observed examples in docs/spec/conf.md: (text after `About=`, About, Version).
        let cases: [(&str, Option<&str>, Option<&str>); 8] = [
            ("one\\\n two\n", Some("one two"), Some("1")),
            ("a\\\\\n\n", Some("a\\"), Some("1")),
            ("a\\\\\n   \n", Some("a\\"), Some("1")),
            ("a\\\n \\\\\n\n", Some("a \\"), Some("1")),
            ("a\\\n\t\nb\n", Some("a"), Some("1")),
            ("a \\\n\\\n", Some("a \\"), Some("1")),
            ("a\\\n  \\\nb\n", Some("a  b"), Some("1")),
            ("a\\\n  \\\n", Some("a  Version=1"), None),
        ];
        for (text, about, version) in cases {
            let c = ModuleConf::parse(&format!("[A]\nAbout={text}Version=1\n")).unwrap();
            assert_eq!((c.get("About"), c.get("Version")), (about, version), "{text:?}");
        }
        let c = ModuleConf::parse("[A]\nAbout=a\\\n\t\nb\nVersion=1\n").unwrap();
        assert_eq!(c.get("b"), Some(""));
    }

    #[test]
    fn defaults() {
        let c = ModuleConf::new("X", vec![("ModDrv".into(), "RawLD4".into())]);
        assert_eq!(c.source_type(), Markup::Gbf);
        assert_eq!(c.encoding(), TextEncoding::Latin1);
        assert_eq!(c.compress_type(), CompressType::Lzss);
        assert_eq!(c.block_type(), BlockType::Chapter);
        assert_eq!(c.versification(), "KJV");
        assert!(!c.case_sensitive_keys());
        assert!(c.strongs_padding());
        assert_eq!(c.key_type(), "TreeKey");
        assert_eq!(c.module_kind().unwrap(), ModuleKind::Dictionary);
        assert!(!c.is_locked());
    }

    #[test]
    fn source_types() {
        let source = |v: &str| ModuleConf::new("X", vec![("SourceType".into(), v.into())]).source_type();
        assert_eq!(source("OSIS"), Markup::Osis);
        assert_eq!(source(" thml "), Markup::Thml);
        assert_eq!(source("GBF"), Markup::Gbf);
        assert_eq!(source("TEI"), Markup::Tei);
        assert_eq!(source("Plain"), Markup::Plaintext);
        assert_eq!(source("RTF"), Markup::Plaintext);
        for m in [Markup::Osis, Markup::Thml, Markup::Gbf, Markup::Tei, Markup::Plaintext] {
            assert_eq!(Markup::from_name(m.as_str()), Some(m));
        }
        assert_eq!(Markup::Thml.as_str(), "ThML");
        assert_eq!(Markup::from_name("usx"), None);
    }

    #[test]
    fn set_and_localized() {
        let mut c = ModuleConf::parse("[A]\nCipherKey=\nDescription=Hi\nDescription_de=Hallo\nCipherKey=x\n").unwrap();
        assert!(c.is_locked());
        c.set("CipherKey", "k");
        assert_eq!(c.get_all("CipherKey").collect::<Vec<_>>(), ["k"]);
        assert_eq!(c.get_localized("Description", "de"), Some("Hallo"));
        assert_eq!(c.get_localized("Description", "fr"), Some("Hi"));
        assert_eq!(ModuleConf::parse("[A]\nDataPath=./modules/texts/x/").unwrap().data_path(), Some("modules/texts/x/"));
    }

    #[test]
    fn rtf() {
        assert_eq!(rtf_to_text(r"Line one\par Line two\line three\tab x"), "Line one\nLine two\nthree\tx");
        assert_eq!(rtf_to_text(r"{\b bold}\b0  \qc centred"), "bold centred");
        assert_eq!(rtf_to_text(r"dash \u8212? here \'e9t\'e9"), "dash — here été");
        assert_eq!(rtf_to_text(r"a\\b\{c\}"), "a\\b{c}");
        assert_eq!(rtf_to_text(r"\u-3913?x"), "\u{F0B7}x");
    }

    #[test]
    fn latin1_conf_bytes() {
        assert_eq!(decode_conf_bytes(b"[A]\nAbout=caf\xe9\n"), "[A]\nAbout=café\n");
        assert_eq!(decode_conf_bytes(b"\xEF\xBB\xBF[A]\n"), "[A]\n");
        assert_eq!(decode_conf_bytes(b"[A]\nEncoding=UTF-8\nAbout=caf\xe9\n"), "[A]\nEncoding=UTF-8\nAbout=caf\u{FFFD}\n");
    }
}
