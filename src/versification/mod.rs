//! Versification systems and the verse index layout (`docs/spec/versification.md`).

#[rustfmt::skip]
mod data;

use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

use topos_bible::data::books::BookId;
use topos_bible::data::chapter_verses::ChapterVerses;
use topos_bible::data::BibleData;
use topos_bible::segments::units::chapter_verse::ChapterVerse;
use topos_bible::segments::{Passage, Segment};

/// The topos book data this crate names books with: English names and abbreviations, OSIS
/// ids, and the deuterocanon ([`BibleData::with_deuterocanon`]). Parse references with it:
///
/// ```
/// let passage = sword_modules::bible_data().parse("Sir 24:3-5").unwrap();
/// assert_eq!(passage.to_osis(sword_modules::bible_data().books()).unwrap(), "Sir.24.3-Sir.24.5");
/// ```
pub fn bible_data() -> &'static BibleData {
    static DATA: OnceLock<BibleData> = OnceLock::new();
    DATA.get_or_init(BibleData::with_deuterocanon)
}

/// Book ids by lower-cased OSIS id.
fn osis_ids() -> &'static HashMap<String, BookId> {
    static IDS: OnceLock<HashMap<String, BookId>> = OnceLock::new();
    IDS.get_or_init(|| {
        let books = bible_data().books();
        books.ids().filter_map(|id| books.get_osis(id).map(|osis| (osis.to_ascii_lowercase(), id))).collect()
    })
}

fn book_id_for_osis(osis: &str) -> Option<BookId> {
    osis_ids().get(&osis.to_ascii_lowercase()).copied()
}

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

impl Testament {
    pub(crate) fn file_prefix(self) -> &'static str {
        match self {
            Testament::Old => "ot",
            Testament::New => "nt",
        }
    }

    fn index(self) -> usize {
        match self {
            Testament::Old => 0,
            Testament::New => 1,
        }
    }
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
    pub fn new(book: BookId, chapter: u8, verse: u8) -> Self {
        VerseKey { book, chapter, verse }
    }

    /// A key for an OSIS book id (`John`, `1Sam`, `Tob`; any case), or `None` for an
    /// unknown book.
    pub fn from_osis(book: &str, chapter: u8, verse: u8) -> Option<Self> {
        book_id_for_osis(book).map(|id| VerseKey::new(id, chapter, verse))
    }

    /// The OSIS book id, e.g. `John`.
    pub fn osis_book(&self) -> Option<&'static str> {
        bible_data().books().get_osis(self.book).map(String::as_str)
    }

    /// OSIS reference string, e.g. `John.3.16`.
    pub fn osis_ref(&self) -> Option<String> {
        self.osis_book().map(|b| format!("{b}.{}.{}", self.chapter, self.verse))
    }

    pub fn chapter_verse(&self) -> ChapterVerse {
        ChapterVerse::new(self.chapter, self.verse)
    }

    /// This one verse as a topos passage.
    pub fn passage(&self) -> Passage {
        Segment::chapter_verse(self.chapter, self.verse).as_segments().with_book(self.book)
    }
}

impl fmt::Display for VerseKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match bible_data().books().get_name(self.book) {
            Some(name) => write!(f, "{name} {}:{}", self.chapter, self.verse),
            None => write!(f, "Book {} {}:{}", self.book.0, self.chapter, self.verse),
        }
    }
}

/// A position in a module's index: testament plus slot number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IndexPos {
    pub testament: Testament,
    pub slot: u32,
}

/// A versification system (KJV, NRSVA, Synodal, …).
#[derive(Debug)]
pub struct Versification {
    name: &'static str,
    books: Vec<BookDef>,
    /// Verse counts of each book's chapters, parallel to `books`.
    verses: Vec<&'static [u8]>,
    /// How many of `books` are in the Old Testament.
    old_books: usize,
    /// Slot of each book's introduction, within its testament.
    book_slots: Vec<u32>,
    /// Slot of each chapter's introduction, per book (chapter 1 first).
    chapter_slots: Vec<Vec<u32>>,
    slot_counts: [u32; 2],
    by_id: HashMap<BookId, usize>,
}

impl Versification {
    fn build(system: &'static data::System) -> Versification {
        // Old-Testament-only systems (MT, Leningrad) report their Old Testament again
        // when asked for the New; such a list is taken as an empty New Testament.
        let same_as_ot = system.nt.len() == system.ot.len() && system.nt.iter().zip(system.ot).all(|(a, b)| a.osis == b.osis);
        let nt: &[data::Book] = if same_as_ot { &[] } else { system.nt };
        let mut v =
            Versification { name: system.name, books: Vec::new(), verses: Vec::new(), old_books: 0, book_slots: Vec::new(), chapter_slots: Vec::new(), slot_counts: [2, 2], by_id: HashMap::new() };
        for (testament, books) in [(Testament::Old, system.ot), (Testament::New, nt)] {
            for book in books {
                let Some(id) = book_id_for_osis(book.osis) else { continue };
                if v.by_id.contains_key(&id) {
                    continue;
                }
                let next = &mut v.slot_counts[testament.index()];
                v.book_slots.push(*next);
                let mut slot = *next + 1;
                let mut chapters = Vec::with_capacity(book.verses.len());
                for &count in book.verses {
                    chapters.push(slot);
                    slot += 1 + u32::from(count);
                }
                *next = slot;
                v.by_id.insert(id, v.books.len());
                v.books.push(BookDef { name: book.name, osis: book.osis, abbrev: book.abbrev, id, chapters: u8::try_from(book.verses.len()).unwrap_or(u8::MAX) });
                v.verses.push(book.verses);
                v.chapter_slots.push(chapters);
                if testament == Testament::Old {
                    v.old_books += 1;
                }
            }
        }
        v
    }

    /// All built-in systems.
    pub fn all() -> &'static [Versification] {
        static ALL: OnceLock<Vec<Versification>> = OnceLock::new();
        ALL.get_or_init(|| data::SYSTEMS.iter().map(Versification::build).collect())
    }

    /// Look up a system by name (case-insensitive), e.g. `"KJV"`, `"NRSVA"`.
    pub fn get(name: &str) -> Option<&'static Versification> {
        let name = name.trim();
        Versification::all().iter().find(|v| v.name.eq_ignore_ascii_case(name))
    }

    /// The KJV system (SWORD's default).
    pub fn kjv() -> &'static Versification {
        static KJV: OnceLock<&'static Versification> = OnceLock::new();
        KJV.get_or_init(|| match Versification::get("KJV") {
            Some(v) => v,
            // The data always has KJV; fall back to the first system rather than panic.
            None => &Versification::all()[0],
        })
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Books in canonical order (OT then NT).
    pub fn books(&self) -> &[BookDef] {
        &self.books
    }

    /// Index of a book in [`books`](Self::books).
    pub fn book_index(&self, book: BookId) -> Option<usize> {
        self.by_id.get(&book).copied()
    }

    /// Book definition by id.
    pub fn book(&self, book: BookId) -> Option<&BookDef> {
        self.book_index(book).map(|i| &self.books[i])
    }

    /// Book definition by OSIS id (case-insensitive).
    pub fn book_by_osis(&self, osis: &str) -> Option<&BookDef> {
        self.books.iter().find(|b| b.osis.eq_ignore_ascii_case(osis))
    }

    pub fn testament_of(&self, book_index: usize) -> Testament {
        if book_index < self.old_books {
            Testament::Old
        } else {
            Testament::New
        }
    }

    /// Number of chapters in a book.
    pub fn chapter_count(&self, book: BookId) -> Option<u8> {
        self.book(book).map(|b| b.chapters)
    }

    /// Number of verses in a chapter (1-based chapter).
    pub fn verse_count(&self, book: BookId, chapter: u8) -> Option<u8> {
        let i = self.book_index(book)?;
        self.verses[i].get(usize::from(chapter).checked_sub(1)?).copied()
    }

    /// The book's verse counts as topos versification, for expanding passages
    /// ([`Passage::verses`]) and comparing them.
    pub fn chapter_verses(&self, book: BookId) -> Option<ChapterVerses> {
        self.book_index(book).map(|i| ChapterVerses::new(self.verses[i].to_vec()))
    }

    /// Does this system contain the reference (allowing chapter 0 / verse 0 intros)?
    pub fn contains(&self, key: &VerseKey) -> bool {
        match key.chapter {
            0 => key.verse == 0 && self.book_index(key.book).is_some(),
            c => self.verse_count(key.book, c).is_some_and(|n| key.verse <= n),
        }
    }

    /// Map a reference to its slot in the module index.
    pub fn index_of(&self, key: &VerseKey) -> Option<IndexPos> {
        if !self.contains(key) {
            return None;
        }
        let i = self.book_index(key.book)?;
        let slot = match key.chapter {
            0 => self.book_slots[i],
            c => self.chapter_slots[i][usize::from(c) - 1] + u32::from(key.verse),
        };
        Some(IndexPos { testament: self.testament_of(i), slot })
    }

    /// Reverse of [`index_of`](Self::index_of). Slots 0 and 1 (module and testament
    /// headings) have no `VerseKey` and return `None`.
    pub fn key_at(&self, pos: IndexPos) -> Option<VerseKey> {
        if pos.slot < 2 || pos.slot >= self.slot_count(pos.testament) {
            return None;
        }
        let range = match pos.testament {
            Testament::Old => 0..self.old_books,
            Testament::New => self.old_books..self.books.len(),
        };
        let starts = &self.book_slots[range.clone()];
        let i = range.start + starts.partition_point(|&s| s <= pos.slot).checked_sub(1)?;
        let book = self.books[i].id;
        if pos.slot == self.book_slots[i] {
            return Some(VerseKey::new(book, 0, 0));
        }
        let chapters = &self.chapter_slots[i];
        let c = chapters.partition_point(|&s| s <= pos.slot).checked_sub(1)?;
        let verse = pos.slot - chapters[c];
        if verse > u32::from(self.verses[i][c]) {
            return None;
        }
        Some(VerseKey::new(book, u8::try_from(c + 1).ok()?, u8::try_from(verse).ok()?))
    }

    /// Number of slots in a testament's index.
    pub fn slot_count(&self, t: Testament) -> u32 {
        self.slot_counts[t.index()]
    }

    /// The range of slots a book occupies (its introduction to its last verse).
    pub(crate) fn book_slot_range(&self, book: BookId) -> Option<(Testament, std::ops::Range<u32>)> {
        let i = self.book_index(book)?;
        let start = self.book_slots[i];
        let len = 1 + self.verses[i].iter().map(|&n| 1 + u32::from(n)).sum::<u32>();
        Some((self.testament_of(i), start..start + len))
    }

    /// Iterate all verses (verse ≥ 1) in canonical order.
    pub fn verses(&self) -> impl Iterator<Item = VerseKey> + '_ {
        self.books.iter().zip(&self.verses).flat_map(|(book, counts)| {
            counts.iter().enumerate().flat_map(move |(c, &n)| {
                let chapter = u8::try_from(c + 1).unwrap_or(u8::MAX);
                (1..=n).map(move |v| VerseKey::new(book.id, chapter, v))
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(osis: &str, c: u8, v: u8) -> VerseKey {
        VerseKey::from_osis(osis, c, v).unwrap()
    }

    #[test]
    fn systems_and_books() {
        assert_eq!(Versification::all().len(), 18);
        for v in Versification::all() {
            assert!(!v.books().is_empty(), "{}", v.name());
        }
        let kjv = Versification::kjv();
        assert_eq!(kjv.name(), "KJV");
        assert_eq!(kjv.books().len(), 66);
        assert_eq!(kjv.book_by_osis("1sam").unwrap().name, "I Samuel");
        assert_eq!(kjv.verse_count(key("Ps", 1, 1).book, 119), Some(176));
        assert_eq!(Versification::get("mt").unwrap().books().len(), 39);
        assert_eq!(Versification::get("MT").unwrap().slot_count(Testament::New), 2);
    }

    #[test]
    fn slots() {
        let kjv = Versification::kjv();
        assert_eq!(kjv.index_of(&key("Gen", 1, 1)), Some(IndexPos { testament: Testament::Old, slot: 4 }));
        assert_eq!(kjv.index_of(&key("Matt", 1, 1)), Some(IndexPos { testament: Testament::New, slot: 4 }));
        assert_eq!(kjv.index_of(&key("Gen", 0, 0)).unwrap().slot, 2);
        assert_eq!(kjv.index_of(&key("Gen", 1, 0)).unwrap().slot, 3);
        assert_eq!(kjv.index_of(&key("Gen", 2, 1)).unwrap().slot, 4 + 31 + 1);
        assert_eq!(kjv.index_of(&key("Gen", 0, 1)), None);
        assert_eq!(kjv.index_of(&key("Gen", 1, 32)), None);
        assert_eq!(kjv.index_of(&key("Tob", 1, 1)), None);
        for v in Versification::all() {
            for t in [Testament::Old, Testament::New] {
                for slot in 0..v.slot_count(t) {
                    let pos = IndexPos { testament: t, slot };
                    match v.key_at(pos) {
                        Some(k) => assert_eq!(v.index_of(&k), Some(pos), "{} {k}", v.name()),
                        None => assert!(slot < 2, "{} {slot}", v.name()),
                    }
                }
            }
            assert_eq!(v.key_at(IndexPos { testament: Testament::Old, slot: v.slot_count(Testament::Old) }), None);
        }
    }

    #[test]
    fn keys() {
        let k = key("john", 3, 16);
        assert_eq!(k.to_string(), "John 3:16");
        assert_eq!(k.osis_ref().as_deref(), Some("John.3.16"));
        assert_eq!(k.passage(), bible_data().parse("John 3:16").unwrap());
        assert_eq!(VerseKey::from_osis("Nope", 1, 1), None);
        let kjv = Versification::kjv();
        assert_eq!(kjv.verses().count(), 31102);
        assert_eq!(kjv.verses().next(), Some(key("Gen", 1, 1)));
    }
}
