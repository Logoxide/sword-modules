//! Line-oriented access to individual routines, for black-box differential
//! testing against libsword (see `tools/differential/run.py`).
//!
//! Commands on stdin, one per line; one result line each:
//!   norm <hex>                      -> <hex>   entry normalisation
//!   pad <key>                       -> <key>   Strong's key padding
//!   lzss <hex>                      -> <hex>   LZSS decode
//!   slot <system> <book> <ch> <v>   -> <testament> <slot>
//!   conf <file> <module> <key>      -> <value as hex> (or "-" if absent)
use std::io::{BufRead, Write};
use sword_modules::{normalize_entry, strongs_pad, ModuleConf, VerseKey, Versification};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::BufWriter::new(std::io::stdout());
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let mut parts = line.splitn(2, ' ');
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("");
        let result = match cmd {
            "norm" => hex(&normalize_entry(&unhex(arg))),
            "pad" => strongs_pad(arg),
            "lzss" => hex(&sword_modules::__lzss_decode_for_tests(&unhex(arg))),
            "slot" => {
                let f: Vec<&str> = arg.split(' ').collect();
                let v = Versification::get(f[0]).unwrap();
                let key = VerseKey::from_osis(f[1], f[2].parse().unwrap(), f[3].parse().unwrap());
                match key.and_then(|k| v.index_of(&k)) {
                    Some(p) => format!("{} {}", if p.testament == sword_modules::Testament::Old { 1 } else { 2 }, p.slot),
                    None => "none".into(),
                }
            }
            "conf" => {
                let f: Vec<&str> = arg.splitn(3, ' ').collect();
                let c = ModuleConf::from_file(f[0]).unwrap();
                assert_eq!(c.name(), f[1]);
                c.get(f[2]).map(|v| hex(v.as_bytes())).unwrap_or_else(|| "-".into())
            }
            other => panic!("unknown command {other}"),
        };
        writeln!(out, "{result}").unwrap();
    }
}
