# Spec: versification and verse index layout

**Source of this spec:** facts reported by libsword's runtime API
(`VerseKey` per versification system; extracted by `tools/extract_v11n.py`
into `data/versification.json`), and index positions checked against
`VerseKey.getTestamentIndex()` for every verse of every system
(`tools/differential/`).

## Data

For each of the 18 built-in systems: the ordered Old- and New-Testament book
lists, each book's English name, OSIS id and abbreviation, and the number of
verses in each chapter.

## Index layout

Verse-keyed modules have one index per testament with a slot for every possible
position, in this order:

* slot 0 — module heading
* slot 1 — testament heading
* then, for each book of the testament in order:
  * one slot for the book introduction (chapter 0, verse 0)
  * for each chapter: one slot for the chapter introduction (verse 0), then
    one slot per verse.

Equivalently: a verse's slot is 2, plus one for each book introduction up to and
including its own book, plus (1 + verse count) for each earlier chapter in the
testament, plus 1 for its own chapter's introduction, plus its verse number.
In the KJV system Genesis 1:1 and Matthew 1:1 are both slot 4.
