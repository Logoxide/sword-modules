//! A pure-Rust reader and installer for CrossWire SWORD modules: Bibles, commentaries,
//! dictionaries and general books, as AndBible, BibleTime and Xiphos use them.
//!
//! Open a library (a directory with `mods.d/` and `modules/`) with [`SwordLibrary`], then
//! a module by name. The formats and every compatibility rule are described in
//! `docs/spec/`.

mod cipher;
mod compress;
pub mod conf;
mod drivers;
pub mod encoding;
mod error;
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

/// Entry markup: one tree for OSIS, ThML, GBF and TEI, with plain text, HTML, filters and search.
pub use topos_bible_markup as markup;

pub use markup::{Filter, HtmlRenderer, Kind, Node, Strongs, TextOptions};

pub use module::{DictionaryModule, Entry, GenBookModule, Module, VerseModule};

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
