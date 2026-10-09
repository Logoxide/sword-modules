//! Installer tests against a repository with CrossWire's layout (built from the
//! fixture modules), served by a minimal in-process HTTP server and via file://.
#![cfg(feature = "install")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use sword_modules::install::{self, Installer, ModuleStatus, Repository};
use sword_modules::{Error, SwordLibrary, VerseKey};

fn key(book: &str, chapter: u8, verse: u8) -> VerseKey {
    VerseKey::from_osis(book, chapter, verse).unwrap()
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn tmp(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let p = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("install-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// A repository: fixtures + mods.d.tar.gz + packages/ for some modules.
fn make_repo() -> PathBuf {
    let repo = tmp("repo");
    let f = std::fs::File::open(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures.tar.gz")).unwrap();
    tar::Archive::new(flate2::read::GzDecoder::new(f)).unpack(&repo).unwrap();
    for junk in ["ref", "test.osis.xml", "dict.imp", "book.imp", "gen.py"] {
        let p = repo.join(junk);
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::remove_file(&p);
    }
    // mods.d.tar.gz
    let gz = flate2::write::GzEncoder::new(std::fs::File::create(repo.join("mods.d.tar.gz")).unwrap(), flate2::Compression::default());
    let mut tb = tar::Builder::new(gz);
    tb.append_dir_all("mods.d", repo.join("mods.d")).unwrap();
    tb.into_inner().unwrap().finish().unwrap();
    // packages for TZipBook and LZip only; everything else needs the file-by-file path
    std::fs::create_dir_all(repo.join("packages")).unwrap();
    for (name, conf, data) in [
        ("TZipBook", "mods.d/TZipBook.conf", "modules/texts/ztext/TZipBook"),
        ("LZip", "mods.d/LZip.conf", "modules/lexdict/x/LZip"),
    ] {
        let mut files = vec![conf.to_string()];
        for e in std::fs::read_dir(repo.join(data)).unwrap() {
            files.push(format!("{data}/{}", e.unwrap().file_name().to_string_lossy()));
        }
        write_zip(&repo.join(format!("packages/{name}.zip")), &files.iter().map(|f| (f.as_str(), std::fs::read(repo.join(f)).unwrap())).collect::<Vec<_>>());
    }
    repo
}

fn write_zip(path: &Path, files: &[(&str, Vec<u8>)]) {
    let mut z = zip_archive::ZipWriter::new(std::fs::File::create(path).unwrap());
    let opts = zip_archive::write::SimpleFileOptions::default();
    for (name, data) in files {
        z.start_file(*name, opts).unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
}

/// Minimal HTTP/1.1 file server with Apache-style directory listings.
fn serve(root: PathBuf) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let root = root.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    return;
                }
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let rel = path.trim_start_matches("/repo").trim_start_matches('/');
                let rel = rel.replace("%20", " ");
                let fs_path = root.join(&rel);
                let (status, body) = if rel.contains("..") {
                    ("403 Forbidden", Vec::new())
                } else if fs_path.is_dir() {
                    let mut html = String::from("<html><body><pre><a href=\"?C=N;O=D\">Name</a>\n<a href=\"/\">Parent Directory</a>\n");
                    let mut names: Vec<_> = std::fs::read_dir(&fs_path).unwrap().flatten().collect();
                    names.sort_by_key(|e| e.file_name());
                    for e in names {
                        let n = e.file_name().to_string_lossy().into_owned();
                        let slash = if e.path().is_dir() { "/" } else { "" };
                        html.push_str(&format!("<a href=\"{}{slash}\">{n}{slash}</a>  2026-10-08 03:45  1K\n", n.replace(' ', "%20")));
                    }
                    html.push_str("</pre></body></html>");
                    ("200 OK", html.into_bytes())
                } else {
                    match std::fs::read(&fs_path) {
                        Ok(b) => ("200 OK", b),
                        Err(_) => ("404 Not Found", b"not found".to_vec()),
                    }
                };
                let mut s = stream;
                let _ = write!(s, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = s.write_all(&body);
                let _ = s.flush();
                let mut sink = [0u8; 1];
                let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(50)));
                let _ = s.read(&mut sink);
            });
        }
    });
    format!("http://{addr}/repo")
}

#[test]
fn install_over_http_zip_and_file_by_file() {
    let repo_dir = make_repo();
    let url = serve(repo_dir.clone());
    let lib_root = tmp("lib-http");
    let installer = Installer::new();
    let repo = Repository::from_url("Test", &url).unwrap();
    let catalog = installer.catalog(&repo).unwrap();
    assert!(catalog.get("tzipbook").is_some());
    assert_eq!(catalog.modules.len(), 20);

    // zip package
    assert_eq!(installer.install(&catalog, "TZipBook", &lib_root).unwrap(), ["TZipBook"]);
    assert_eq!(installer.install(&catalog, "LZip", &lib_root).unwrap(), ["LZip"]);
    // file by file (no package): Bible, raw dictionary, general book, personal notes
    for m in ["TLzss", "LRaw", "GBook", "Personal"] {
        assert_eq!(installer.install(&catalog, m, &lib_root).unwrap(), [m]);
    }
    for (m, dir) in [
        ("TZipBook", "modules/texts/ztext/TZipBook"),
        ("TLzss", "modules/texts/ztext/TLzss"),
        ("LRaw", "modules/lexdict/x/LRaw"),
        ("GBook", "modules/genbook/rawgenbook/gb"),
        ("Personal", "modules/comments/rawfiles/personal"),
    ] {
        assert_same_dir(&repo_dir.join(dir), &lib_root.join(dir), m);
    }
    assert!(!lib_root.join(".sword-modules-staging").exists());

    let lib = SwordLibrary::open(&lib_root).unwrap();
    let kjv = lib.open_module("TLzss").unwrap().into_verse().unwrap();
    assert!(kjv.verse(&key("Gen", 1, 1)).unwrap().is_some());
    assert_eq!(lib.conf("TLzss").unwrap().get("InstallSourceCaption"), Some("Test"));
    assert!(lib.open_module("LZip").unwrap().into_dictionary().unwrap().get("G25").unwrap().is_some());

    // status
    let status = install::module_status(&catalog, &lib);
    let st = |n: &str| status.iter().find(|(m, _)| m.name() == n).unwrap().1;
    assert_eq!(st("TLzss"), ModuleStatus::SameVersion);
    assert_eq!(st("TXz"), ModuleStatus::NotInstalled);

    // reinstall replaces cleanly; uninstall removes data + conf
    installer.install(&catalog, "TLzss", &lib_root).unwrap();
    install::uninstall(&lib_root, "TLzss").unwrap();
    assert!(!lib_root.join("modules/texts/ztext/TLzss").exists());
    assert!(SwordLibrary::open(&lib_root).unwrap().conf("TLzss").is_none());
    assert!(matches!(install::uninstall(&lib_root, "TLzss"), Err(Error::ModuleNotFound(_))));

    // missing module, missing repository
    assert!(matches!(installer.install(&catalog, "Nope", &lib_root), Err(Error::ModuleNotFound(_))));
    let bad = Repository::from_url("Bad", &format!("{url}/does-not-exist")).unwrap();
    assert!(installer.catalog(&bad).is_err());

    // any zip URL
    let lib2 = tmp("lib-url");
    assert_eq!(installer.install_from_url(&format!("{url}/packages/LZip.zip"), &lib2).unwrap(), ["LZip"]);
}

#[test]
fn install_over_file_urls_and_locked_modules() {
    let repo_dir = make_repo();
    let lib_root = tmp("lib-file");
    let installer = Installer::new();
    let repo = Repository::from_url("Local", &format!("file://{}", repo_dir.display())).unwrap();
    let catalog = installer.catalog(&repo).unwrap();
    // An enciphered module installs even when its conf has no key ("locked"), as
    // published modules are. Patch the catalogue entry to simulate that.
    let mut catalog_locked = catalog.clone();
    let m = catalog_locked.modules.iter_mut().find(|m| m.name() == "TCiphZ").unwrap();
    let text = String::from_utf8(m.conf_bytes.clone()).unwrap().replace("CipherKey=s3cr3t-Key", "CipherKey=");
    m.conf_bytes = text.clone().into_bytes();
    m.conf = sword_modules::ModuleConf::parse(&text).unwrap();
    // TCiphZ has no package zip, so this exercises the file-by-file path over file://
    installer.install(&catalog_locked, "TCiphZ", &lib_root).unwrap();
    let lib = SwordLibrary::open(&lib_root).unwrap();
    assert!(matches!(lib.open_module("TCiphZ"), Err(Error::Locked(_))));
    install::save_cipher_key(&lib_root, "TCiphZ", "s3cr3t-Key").unwrap();
    let lib = SwordLibrary::open(&lib_root).unwrap();
    let m = lib.open_module("TCiphZ").unwrap().into_verse().unwrap();
    assert!(m.verse(&key("Gen", 1, 1)).unwrap().unwrap().raw.contains("end."));
    installer.install(&catalog, "TXz", &lib_root).unwrap();
    assert!(SwordLibrary::open(&lib_root).unwrap().conf("TXz").is_some());
}

#[test]
fn rejects_hostile_archives() {
    let lib_root = tmp("lib-hostile");
    let marker = lib_root.join("keep.txt");
    std::fs::write(&marker, "x").unwrap();
    let dir = tmp("zips");

    // DataPath pointing at the library root
    let z1 = dir.join("root.zip");
    write_zip(&z1, &[("mods.d/evil.conf", b"[Evil]\nDataPath=./\nModDrv=RawText\n".to_vec()), ("modules/x", b"x".to_vec())]);
    assert!(matches!(install::install_zip_file(&z1, &lib_root), Err(Error::Install(_))));

    // path traversal inside the archive
    let z2 = dir.join("traversal.zip");
    write_zip(&z2, &[("../../escape.txt", b"x".to_vec())]);
    assert!(matches!(install::install_zip_file(&z2, &lib_root), Err(Error::Install(_))));
    assert!(!lib_root.parent().unwrap().join("escape.txt").exists());

    // not a zip at all / zip without a module
    assert!(matches!(install::install_zip(b"not a zip", &lib_root), Err(Error::Install(_))));
    let z3 = dir.join("empty.zip");
    write_zip(&z3, &[("readme.txt", b"hello".to_vec())]);
    assert!(matches!(install::install_zip_file(&z3, &lib_root), Err(Error::Install(_))));

    // data that does not open as the declared module is rejected before installing
    let z4 = dir.join("broken.zip");
    write_zip(&z4, &[("mods.d/b.conf", b"[Broken]\nDataPath=./modules/texts/ztext/broken/\nModDrv=zText\nBlockType=BOOK\n".to_vec()), ("modules/texts/ztext/broken/readme", b"x".to_vec())]);
    assert!(matches!(install::install_zip_file(&z4, &lib_root), Err(Error::Install(_))));

    assert!(marker.exists());
    assert!(!lib_root.join("modules").exists());
    assert!(!lib_root.join(".sword-modules-staging").exists());
}

fn assert_same_dir(a: &Path, b: &Path, what: &str) {
    let names = |p: &Path| {
        let mut v: Vec<String> = std::fs::read_dir(p).unwrap_or_else(|e| panic!("{what}: {}: {e}", p.display())).flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    };
    let (na, nb) = (names(a), names(b));
    assert_eq!(na, nb, "{what}");
    for n in na {
        assert_eq!(std::fs::read(a.join(&n)).unwrap(), std::fs::read(b.join(&n)).unwrap(), "{what}/{n}");
    }
}
