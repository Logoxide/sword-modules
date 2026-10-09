#!/usr/bin/env python3
"""Black-box differential tests: the crate's clean-room routines vs. libsword.

libsword is used only through its compiled programs (osis2mod, mod2imp) and its
public Python bindings (python3-sword); no libsword source is consulted.

    cargo build --release --examples
    python3 tools/differential/run.py [--quick]

Checks:
  norm  - entry normalisation: random byte strings written with RawText.setEntry
          and read back with getRawEntry
  pad   - Strong's key padding: random keys stored in a RawLD module, stored keys
          read back with mod2imp
  slot  - verse index slots for every verse of all 18 versification systems
          (VerseKey.getTestamentIndex)
  conf  - conf parsing: random continuation/whitespace layouts read through
          SWMgr/getConfigEntry
  lzss, cipher - random OSIS texts converted by osis2mod with LZSS (verse,
          chapter and book blocks) and with enciphering (plain and "personalised"
          keys); every entry compared with mod2imp
"""
import json, os, random, shutil, subprocess, sys, tempfile

import Sword

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
HARNESS = os.path.join(ROOT, "target/release/examples/differential_harness")
COMPARE = os.path.join(ROOT, "target/release/examples/compare_imp")
QUICK = "--quick" in sys.argv
rng = random.Random(20261008)


def harness(lines):
    p = subprocess.run([HARNESS], input="\n".join(lines) + "\n", capture_output=True, text=True, check=True)
    return p.stdout.split("\n")[: len(lines)]


def report(name, total, failures):
    status = "OK" if not failures else "FAIL"
    print(f"{name:7} {total:7} cases  {len(failures)} mismatches  {status}")
    for f in failures[:5]:
        print("        ", f)
    return not failures


def new_library():
    d = tempfile.mkdtemp(prefix="swdiff-")
    os.makedirs(os.path.join(d, "mods.d"))
    return d


def check_norm():
    lib = new_library()
    path = "modules/texts/rawtext/n"
    os.makedirs(os.path.join(lib, path))
    open(os.path.join(lib, "mods.d/n.conf"), "w").write(f"[N]\nDataPath=./{path}/\nModDrv=RawText\nEncoding=UTF-8\n")
    Sword.RawText.createModule(os.path.join(lib, path))
    mgr = Sword.SWMgr(lib)
    mod = mgr.getModule("N")
    alphabet = "ab \r\n\t"
    n = 20000 if QUICK else 200000
    cases, expected = [], []
    key = Sword.VerseKey("Gen 1:1")
    for _ in range(n):
        s = "".join(rng.choice(alphabet) for _ in range(rng.randint(0, 12)))
        mod.setKey(key)
        mod.setEntry(s, len(s))
        mod.setKey(key)
        cases.append(s)
        expected.append(mod.getRawEntry())
    ours = harness(["norm " + s.encode().hex() for s in cases])
    fails = [(c, e, bytes.fromhex(o).decode()) for c, e, o in zip(cases, expected, ours) if bytes.fromhex(o).decode() != e]
    shutil.rmtree(lib)
    return report("norm", n, fails)


def check_pad():
    lib = new_library()
    path = "modules/lexdict/rawld/k"
    os.makedirs(os.path.join(lib, path))
    open(os.path.join(lib, "mods.d/k.conf"), "w").write(f"[K]\nDataPath=./{path}/k\nModDrv=RawLD\nEncoding=UTF-8\n")
    Sword.RawLD.createModule(os.path.join(lib, path, "k"))
    mgr = Sword.SWMgr(lib)
    mod = mgr.getModule("K")
    alphabet = "GgHh0123456789ab!.-x"
    n = 2000 if QUICK else 20000
    keys = ["".join(rng.choice(alphabet) for _ in range(rng.randint(1, 10))) for _ in range(n)]
    for i, k in enumerate(keys):
        mod.setKey(Sword.SWKey(k))
        v = f"#{i}"
        mod.setEntry(v, len(v))
    del mod, mgr
    dump = subprocess.run(["mod2imp", "K"], capture_output=True, text=True, env={**os.environ, "SWORD_PATH": lib}).stdout
    stored = {}
    lines = dump.split("\n")
    for i in range(0, len(lines) - 1):
        if lines[i].startswith("$$$") and lines[i + 1].startswith("#"):
            stored[int(lines[i + 1][1:])] = lines[i][3:]
    ours = [o.upper() for o in harness(["pad " + k for k in keys])]
    # a later key padding to the same stored key overwrites the earlier entry,
    # so only entries that survived are checked (and every survivor must match)
    fails = [(keys[i], stored[i], ours[i]) for i in stored if stored[i] != ours[i]]
    shutil.rmtree(lib)
    return report("pad", len(stored), fails)


def check_slots():
    systems = [s["name"] for s in json.load(open(os.path.join(ROOT, "data/versification.json")))]
    queries, expected = [], []
    for name in systems:
        k = Sword.VerseKey()
        k.setVersificationSystem(name)
        k.setIntros(True)          # address chapter 0 / verse 0
        k.setAutoNormalize(False)
        for t in (1, 2):
            k.setTestament(t)
            for b in range(1, k.getBookMax() + 1):
                k.setTestament(t); k.setBook(b)
                osis = k.getOSISBookName()
                for c in range(0, k.getChapterMax() + 1):
                    k.setChapter(c)
                    vmax = k.getVerseMax() if c > 0 else 0
                    for v in range(0, vmax + 1):
                        k.setTestament(t); k.setBook(b); k.setChapter(c); k.setVerse(v)
                        queries.append(f"slot {name} {osis} {c} {v}")
                        expected.append(f"{ord(k.getTestament())} {k.getTestamentIndex()}")
    ours = harness(queries)
    fails = [(q, e, o) for q, e, o in zip(queries, expected, ours) if e != o]
    return report("slot", len(queries), fails)


def check_conf():
    lib = new_library()
    path = "modules/texts/rawtext/c"
    os.makedirs(os.path.join(lib, path))
    Sword.RawText.createModule(os.path.join(lib, path))
    pieces = ["alpha", " beta", "\tgamma", "\\par", "x\\", "  ", "=", "é", "\\\\"]
    n = 300 if QUICK else 3000
    fails = []
    for i in range(n):
        # About = random pieces, split over lines with continuations and stray whitespace
        segments = ["".join(rng.choice(pieces) for _ in range(rng.randint(0, 4))) for _ in range(rng.randint(1, 4))]
        about = ""
        for j, seg in enumerate(segments):
            about += seg
            if j < len(segments) - 1:
                about += rng.choice(["\\", "\\ ", "\\\t", " \\"]) + rng.choice(["\n", "\r\n"]) + rng.choice(["", " ", "\t"])
        tail = rng.choice(["", " ", "\t", "\r"])
        text = f"[C{i}]\nDataPath=./{path}/\nModDrv=RawText\nEncoding=UTF-8\n" + rng.choice(["", "  "]) + f"About={about}{tail}\nVersion=1.{i}\n"
        conf = os.path.join(lib, "mods.d", "c.conf")
        open(conf, "w", encoding="utf-8").write(text)
        mgr = Sword.SWMgr(lib)
        m = mgr.getModule(f"C{i}")
        expected = m.getConfigEntry("About") if m else None
        expected = "-" if expected is None else expected.encode("utf-8", "surrogateescape").hex()
        ours = harness([f"conf {conf} C{i} About"])[0]
        if ours != expected:
            fails.append((text, expected, ours))
        del m, mgr
    shutil.rmtree(lib)
    return report("conf", n, fails)


ALPHA = "bceafgijklhmpqBrHostTuwxyAdCDzEFIJKGLNO7PQMRSUVWXY9012Z364n8v5"


def personalised_key():
    """A 5-segment key with a valid check segment (per docs/spec/cipher.md)."""
    segs = ["".join(rng.choice(ALPHA) for _ in range(rng.randint(3, 8))) for _ in range(4)]
    val = {c: i for i, c in enumerate(ALPHA)}
    check = ""
    for s in segs:
        n = min(len(s), len(segs[0]))
        check += ALPHA[sum(val[c] for c in s[:n]) % 62]
    return "-".join(segs + [check])


def random_osis(path, verses):
    words = "in the beginning God created heaven and earth light darkness spirit waters ἀγάπη שָׁלוֹם é ß".split()
    out = ['<?xml version="1.0" encoding="UTF-8"?>',
           '<osis xmlns="http://www.bibletechnologies.net/2003/OSIS/namespace"><osisText osisIDWork="R" osisRefWork="Bible" xml:lang="en"><header><work osisWork="R"/></header>']
    k = Sword.VerseKey()
    k.setVersificationSystem("KJV")
    count = 0
    for t in (1, 2):
        k.setTestament(t)
        for b in range(1, k.getBookMax() + 1):
            k.setTestament(t); k.setBook(b)
            osis = k.getOSISBookName()
            out.append(f'<div type="book" osisID="{osis}">')
            for c in range(1, k.getChapterMax() + 1):
                k.setChapter(c)
                out.append(f'<chapter osisID="{osis}.{c}">')
                for v in range(1, k.getVerseMax() + 1):
                    if rng.random() < verses:
                        text = " ".join(rng.choice(words) for _ in range(rng.randint(1, 60)))
                        out.append(f'<verse osisID="{osis}.{c}.{v}">{text}</verse>')
                        count += 1
                out.append("</chapter>")
            out.append("</div>")
    out.append("</osisText></osis>")
    open(path, "w", encoding="utf-8").write("\n".join(out))
    return count


def check_osis2mod_variants():
    lib = new_library()
    src = os.path.join(lib, "r.osis.xml")
    random_osis(src, 0.05 if QUICK else 0.5)
    variants = [
        ("RLzssV", ["-z", "l", "-b", "2"], "zText", "CompressType=LZSS\nBlockType=VERSE\n"),
        ("RLzssC", ["-z", "l", "-b", "3"], "zText", "CompressType=LZSS\nBlockType=CHAPTER\n"),
        ("RLzssB", ["-z", "l", "-b", "4"], "zText", "CompressType=LZSS\nBlockType=BOOK\n"),
    ]
    for i in range(2 if QUICK else 6):
        key = personalised_key() if i % 2 == 0 else "plain" + "".join(rng.choice(ALPHA) for _ in range(rng.randint(1, 20)))
        variants.append((f"RCiph{i}", ["-z", rng.choice("lz"), "-c", key], "zText", f"CompressType=%s\nBlockType=BOOK\nCipherKey={key}\n"))
    ok = True
    for name, args, drv, extra in variants:
        if "%s" in extra:
            extra = extra % ("LZSS" if args[1] == "l" else "ZIP")
        d = os.path.join(lib, "modules/texts/ztext", name)
        os.makedirs(d)
        subprocess.run(["osis2mod", d + "/", src] + args, capture_output=True, check=True)
        open(os.path.join(lib, "mods.d", name + ".conf"), "w").write(
            f"[{name}]\nDataPath=./modules/texts/ztext/{name}/\nModDrv={drv}\nSourceType=OSIS\nEncoding=UTF-8\n{extra}")
        imp = os.path.join(lib, name + ".imp")
        with open(imp, "wb") as f:
            subprocess.run(["mod2imp", name], stdout=f, env={**os.environ, "SWORD_PATH": lib}, check=True)
        r = subprocess.run([COMPARE, lib, name, imp], capture_output=True, text=True)
        line = r.stdout.strip().split("\n")[-1] if r.stdout.strip() else r.stderr.strip()
        good = line.endswith("OK")
        ok &= good
        label = "cipher" if "Ciph" in name else "lzss"
        print(f"{label:7} {name:8} {' '.join(args):28} {line}")
    shutil.rmtree(lib)
    return ok


if __name__ == "__main__":
    results = [check_norm(), check_pad(), check_slots(), check_conf(), check_osis2mod_variants()]
    sys.exit(0 if all(results) else 1)
