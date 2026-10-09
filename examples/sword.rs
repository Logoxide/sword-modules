//! Small CLI showing the API.
//!
//!     cargo run --example sword -- <library-root> list
//!     cargo run --example sword -- <library-root> verse KJV "John 3:16-18; 5" [--html|--strongs|--raw]
//!     cargo run --example sword -- <library-root> chapter KJV Ps 23
//!     cargo run --example sword -- <library-root> dict StrongsGreek G25
//!     cargo run --example sword -- <library-root> toc SomeGenBook
//!     cargo run --example sword -- <library-root> book SomeGenBook "/Part 1/Chapter 2"
//!     cargo run --example sword -- <library-root> repos
//!     cargo run --example sword -- <library-root> remote <repo-url-or-name>
//!     cargo run --example sword -- <library-root> install <repo-url-or-name> KJV
//!     cargo run --example sword -- <library-root> install-url https://example.org/KJV.zip
//!     cargo run --example sword -- <library-root> uninstall KJV
use sword_modules::install::{self, Installer, ModuleStatus, Repository};
use sword_modules::markup::HtmlRenderer;
use sword_modules::{bible_data, Filter, Kind, Module, SwordLibrary, TextOptions};

/// A repository given as a URL, or by name from CrossWire's master list.
fn repository(installer: &Installer, spec: &str) -> Result<Repository, Box<dyn std::error::Error>> {
    if spec.contains("://") {
        return Ok(Repository::from_url(spec, spec)?);
    }
    installer
        .master_repo_list()?
        .into_iter()
        .find(|r| r.name.eq_ignore_ascii_case(spec))
        .ok_or_else(|| format!("no repository named {spec}").into())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let installer = Installer::new();
    match args[1].as_str() {
        "repos" => {
            for r in installer.master_repo_list()? {
                let urls: Vec<String> = r.sources.iter().map(|s| s.base_url()).collect();
                println!("{:<28} {}", r.name, urls.join("  "));
            }
            return Ok(());
        }
        "remote" => {
            let repo = repository(&installer, &args[2])?;
            let catalog = installer.catalog(&repo)?;
            let local = SwordLibrary::open(&args[0]).unwrap_or_default();
            for (m, status) in install::module_status(&catalog, &local) {
                let mark = match status {
                    ModuleStatus::NotInstalled => " ",
                    ModuleStatus::UpdateAvailable => "U",
                    ModuleStatus::SameVersion => "=",
                    ModuleStatus::InstalledIsNewer => ">",
                };
                println!("{mark} {:<16} {:<10} {}", m.name(), m.conf.version().unwrap_or(""), m.conf.description().unwrap_or(""));
            }
            return Ok(());
        }
        "install" => {
            let repo = repository(&installer, &args[2])?;
            let catalog = installer.catalog(&repo)?;
            for name in &args[3..] {
                println!("installed {:?}", installer.install(&catalog, name, &args[0])?);
            }
            return Ok(());
        }
        "install-url" => {
            println!("installed {:?}", installer.install_from_url(&args[2], &args[0])?);
            return Ok(());
        }
        "uninstall" => {
            install::uninstall(&args[0], &args[2])?;
            println!("removed {}", args[2]);
            return Ok(());
        }
        _ => {}
    }
    let lib = SwordLibrary::open(&args[0])?;
    let flag = |f: &str| args.iter().any(|a| a == f);
    match args[1].as_str() {
        "list" => {
            for c in lib.modules() {
                println!(
                    "{:<16} {:<10} {:<9} {:<6} {}",
                    c.name(),
                    c.module_kind().map(|k| format!("{k:?}")).unwrap_or_default(),
                    c.get("ModDrv").unwrap_or(""),
                    format!("{:?}", c.source_type()),
                    c.description().unwrap_or("")
                );
            }
        }
        "verse" => {
            let m = lib.open_module(&args[2])?.into_verse()?;
            let passage = bible_data().parse(&args[3]).ok_or_else(|| format!("not a reference: {}", args[3]))?;
            let verses = m.passage(&passage)?;
            if verses.is_empty() {
                println!("(no text)");
            }
            for (key, e) in verses {
                let text = if flag("--raw") {
                    e.raw.clone()
                } else if flag("--html") {
                    e.html()
                } else {
                    e.plain_text_with(&TextOptions { strongs: flag("--strongs"), filter: Filter::text().keep(Kind::Title), ..Default::default() })
                };
                println!("{key}  {text}");
            }
        }
        "chapter" => {
            let m = lib.open_module(&args[2])?.into_verse()?;
            let mut html = HtmlRenderer::new();
            let book = bible_data().books().search(&args[3]).ok_or_else(|| format!("no book named {}", args[3]))?;
            for (k, e) in m.chapter(book, args[4].parse()?)? {
                if flag("--html") {
                    println!("<sup>{}</sup>{}", k.verse, html.render(&e.nodes()));
                } else {
                    println!("{:>3} {}", k.verse, e.plain_text_with(&TextOptions { filter: Filter::text().keep(Kind::Title), ..Default::default() }));
                }
            }
        }
        "dict" => {
            let d = lib.open_module(&args[2])?.into_dictionary()?;
            match d.get(&args[3])? {
                Some(e) => println!("{}", if flag("--raw") { e.raw } else { e.plain_text() }),
                None => match d.get_nearest(&args[3])? {
                    Some((k, _)) => println!("(not found; nearest key: {k})"),
                    None => println!("(not found)"),
                },
            }
        }
        "toc" => {
            let Module::GenBook(g) = lib.open_module(&args[2])? else { return Err("not a general book".into()) };
            for (depth, node) in g.walk()?.into_iter().skip(1) {
                println!("{}{}", "  ".repeat(depth - 1), node.name);
            }
        }
        "book" => {
            let g = lib.open_module(&args[2])?.into_genbook()?;
            match g.get(&args[3])? {
                Some(e) => println!("{}", e.plain_text()),
                None => println!("(not found)"),
            }
        }
        other => return Err(format!("unknown command {other}").into()),
    }
    Ok(())
}
