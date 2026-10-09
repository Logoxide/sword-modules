#!/usr/bin/env python3
"""Extract SWORD's versification facts (book names, OSIS ids, chapter and verse
counts) by querying a libsword installation through its public runtime API
(the `python3-sword` bindings), and write them as JSON.

This deliberately does not read any libsword source files: it only records the
observable facts each built-in versification system reports.

    python3 tools/extract_v11n.py > data/versification.json
"""
import json
import Sword

def systems():
    names = Sword.VersificationMgr.getSystemVersificationMgr().getVersificationSystems()
    out = []
    it = names.begin() if hasattr(names, "begin") else None
    try:
        for n in names:
            out.append(str(n))
    except TypeError:
        pass
    return out

KNOWN = ["KJV", "Leningrad", "MT", "KJVA", "NRSV", "NRSVA", "Synodal", "SynodalProt", "Vulg",
         "German", "Luther", "Catholic", "Catholic2", "LXX", "Orthodox", "Calvin", "DarbyFr", "Segond"]

def extract(name):
    k = Sword.VerseKey()
    k.setVersificationSystem(name)
    assert k.getVersificationSystem() == name, name
    testaments = []
    for t in (1, 2):
        k.setTestament(t)
        books = []
        for b in range(1, k.getBookMax() + 1):
            k.setTestament(t); k.setBook(b)
            chapters = []
            for c in range(1, k.getChapterMax() + 1):
                k.setChapter(c)
                chapters.append(k.getVerseMax())
            books.append({"name": k.getBookName(), "osis": k.getOSISBookName(),
                          "abbrev": k.getBookAbbrev(), "verses": chapters})
        testaments.append(books)
    return {"name": name, "ot": testaments[0], "nt": testaments[1]}

if __name__ == "__main__":
    names = systems() or KNOWN
    print(json.dumps([extract(n) for n in names], ensure_ascii=False, indent=1))
