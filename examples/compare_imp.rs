//! Developer tool: compare every entry this crate reads with the output of
//! libsword's `mod2imp` (the reference implementation).
//!
//!     mod2imp KJV > kjv.imp
//!     cargo run --example compare_imp -- /usr/share/sword KJV kjv.imp
use sword_modules::encoding::decode_text;
use sword_modules::{IndexPos, Module, SwordLibrary, Testament};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let (root, name, imp) = (&args[1], &args[2], &args[3]);
    let lib = SwordLibrary::open(root)?;
    let conf = lib.conf(name).ok_or("no such module")?;
    let enc = conf.encoding();
    // Same rule as the reader: an undeclared encoding with valid non-ASCII UTF-8 is UTF-8.
    let sniff_utf8 = conf.get("Encoding").is_none();
    let module = lib.open_module(name)?;

    // reference: bodies of all non-empty entries, in order
    let bytes = std::fs::read(imp)?;
    let mut expected = Vec::new();
    let mut cur: Option<Vec<u8>> = None;
    for line in bytes.split(|&b| b == b'\n') {
        if line.starts_with(b"$$$") {
            if let Some(body) = cur.take() {
                expected.push(body);
            }
            cur = Some(Vec::new());
        } else if let Some(body) = cur.as_mut() {
            if !body.is_empty() {
                body.push(b'\n');
            }
            body.extend_from_slice(line);
        }
    }
    if let Some(body) = cur.take() {
        expected.push(body);
    }
    let expected: Vec<String> = expected
        .into_iter()
        .map(|mut b| {
            while b.last() == Some(&b'\n') {
                b.pop();
            }
            if sniff_utf8 && !b.is_ascii() && std::str::from_utf8(&b).is_ok() {
                String::from_utf8(b).unwrap()
            } else {
                decode_text(&b, enc)
            }
        })
        .filter(|s| !s.is_empty())
        .collect();

    let actual: Vec<String> = match &module {
        Module::Verse(m) => {
            let v = m.versification();
            let mut out = Vec::new();
            for t in [Testament::Old, Testament::New] {
                if !m.has_testament(t) {
                    continue;
                }
                for slot in 0..v.slot_count(t) {
                    if let Some(e) = m.at(IndexPos { testament: t, slot })? {
                        out.push(e.raw);
                    }
                }
            }
            out
        }
        Module::Dictionary(d) => d.iter().map(|r| r.map(|(_, e)| e.raw)).collect::<Result<_, _>>()?,
        Module::GenBook(g) => {
            let mut out = Vec::new();
            for (_, n) in g.walk()? {
                if let Some(e) = g.text(&n)? {
                    out.push(e.raw);
                }
            }
            out
        }
    };

    let mut mismatches = 0;
    for (i, (a, e)) in actual.iter().zip(&expected).enumerate() {
        if a != e {
            mismatches += 1;
            if mismatches <= 3 {
                println!("MISMATCH #{i}\n  ours: {:?}\n  ref:  {:?}", &a.chars().take(160).collect::<String>(), &e.chars().take(160).collect::<String>());
            }
        }
    }
    println!(
        "{name}: ours={} ref={} mismatches={} {}",
        actual.len(),
        expected.len(),
        mismatches,
        if mismatches == 0 && actual.len() == expected.len() { "OK" } else { "FAIL" }
    );
    Ok(())
}
