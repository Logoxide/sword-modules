//! A pure-Rust reader and installer for CrossWire SWORD modules: Bibles, commentaries,
//! dictionaries and general books, as AndBible, BibleTime and Xiphos use them.
//!
//! Open a library (a directory with `mods.d/` and `modules/`) with [`SwordLibrary`], then
//! a module by name. The formats and every compatibility rule are described in
//! `docs/spec/`.
//!
//! Entries come as their stored text with the markup the conf names ([`Entry`]: `raw` and
//! [`Markup`]); the crate parses no markup, so callers use whichever markup parser they like.

mod cipher;
mod compress;
pub mod conf;
mod drivers;
pub mod encoding;
mod error;
mod files;
#[cfg(feature = "install")]
pub mod install;
mod library;
mod lzss;
mod module;
mod normalize;
pub mod versification;

pub use conf::{BlockType, CompressType, Markup, ModDrv, ModuleConf, ModuleKind};

pub use drivers::genbook::TreeNode;

pub use normalize::{normalize_entry, strongs_pad};

pub use error::{Error, Result};

pub use library::SwordLibrary;

pub use files::{FileReader, Files, FsFiles, MemoryFiles};

pub use module::{DictionaryModule, Entry, EntryLocation, GenBookModule, Module, VerseModule, VerseSpan};

pub use versification::{bible_data, IndexPos, Testament, VerseKey, Versification};

/// The verse reference library: book data, passages and their parsing.
pub use topos_bible as topos;

pub use topos_bible::data::books::BookId;

pub use topos_bible::segments::Passage;

/// Not part of the API: exposes the LZSS decoder to the differential test harness.
#[doc(hidden)]
pub fn __lzss_decode_for_tests(data: &[u8]) -> Vec<u8> {
    lzss::decode(data)
}
