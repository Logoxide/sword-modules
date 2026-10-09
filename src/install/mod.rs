//! Installing modules: CrossWire's repository list, repository catalogues, module zips,
//! and removing or unlocking installed modules.
//!
//! A repository root holds `mods.d.tar.gz` (every module's conf), `packages/<Module>.zip`
//! for some or all modules, and `modules/…` with the data of each. A module is installed
//! from its package zip when there is one, otherwise file by file from directory
//! listings. Everything is unpacked into a staging directory inside the library and only
//! moved into place once the module opens.

mod ftp;
pub mod transport;

pub use transport::{DefaultTransport, DirEntry, Transport};

use std::cmp::Ordering;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use crate::conf::{decode_conf_bytes, parse_conf, ModuleConf, ModuleKind};
use crate::error::{Error, Result};
use crate::library::{conf_files, read_module_confs, SwordLibrary};
use crate::module::Module;
use transport::percent_encode;

/// Where CrossWire publishes the list of known repositories.
pub const MASTER_REPO_LIST_URL: &str = "https://crosswire.org/ftpmirror/pub/sword/masterRepoList.conf";
/// FTP fallback for the master list (used by libsword when HTTPS fails).
pub const MASTER_REPO_LIST_FTP_URL: &str = "ftp://ftp.crosswire.org/pub/sword/masterRepoList.conf";

/// The most an installation may unpack or download.
const SIZE_CAP: u64 = 4 << 30;
/// How deep a module's data directory may nest when copied file by file.
const MAX_DEPTH: usize = 8;
/// The staging directory inside a library.
const STAGING: &str = ".sword-modules-staging";

/// Transfer protocol of a repository source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Protocol {
    Https,
    Http,
    Ftp,
    Sftp,
    File,
}

impl Protocol {
    fn scheme(self) -> &'static str {
        match self {
            Protocol::Https => "https",
            Protocol::Http => "http",
            Protocol::Ftp => "ftp",
            Protocol::Sftp => "sftp",
            Protocol::File => "file",
        }
    }

    fn from_scheme(s: &str) -> Option<Protocol> {
        Some(match s.to_ascii_lowercase().as_str() {
            "https" => Protocol::Https,
            "http" => Protocol::Http,
            "ftp" => Protocol::Ftp,
            "sftp" => Protocol::Sftp,
            "file" => Protocol::File,
            _ => return None,
        })
    }

    /// The protocol a conf key names before its `Source` / `PackagePreference` suffix
    /// (`HTTPSSource`, `FTPPackagePreference`).
    fn from_key(kind: &str) -> Option<Protocol> {
        let k = kind.to_ascii_uppercase();
        let name = k.strip_suffix("PACKAGEPREFERENCE").or_else(|| k.strip_suffix("SOURCE"))?;
        Some(match name {
            "HTTPS" => Protocol::Https,
            "HTTP" => Protocol::Http,
            "FTP" => Protocol::Ftp,
            "SFTP" => Protocol::Sftp,
            "DIR" | "FILE" | "LOCAL" => Protocol::File,
            _ => return None,
        })
    }
}

/// One way of reaching a repository (libsword's `InstallSource`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub protocol: Protocol,
    pub host: String,
    /// Repository root on the host, e.g. `/ftpmirror/pub/sword/raw`.
    pub directory: String,
    pub user: Option<String>,
    pub password: Option<String>,
    /// From a `…PackagePreference` entry: try this source first.
    pub package_preference: bool,
}

fn non_empty(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

impl Source {
    /// Parse `Caption|host|directory|user|password|uid` (the value of a
    /// `HTTPSSource=` / `FTPSource=` / `…PackagePreference=` entry).
    /// Returns the caption, the uid (if given) and the source.
    pub fn parse_entry(kind: &str, value: &str) -> Option<(String, Option<String>, Source)> {
        let kind = kind.trim();
        let package_preference = kind.to_ascii_lowercase().ends_with("packagepreference");
        let protocol = Protocol::from_key(kind)?;
        let fields: Vec<&str> = value.split('|').collect();
        if fields.len() < 3 {
            return None;
        }
        let caption = fields[0].trim().to_string();
        let host = fields[1].trim().to_string();
        let directory = fields[2].trim().trim_end_matches('/').to_string();
        if caption.is_empty() || (host.is_empty() && protocol != Protocol::File) {
            return None;
        }
        let source = Source { protocol, host, directory, user: non_empty(fields.get(3).copied()), password: non_empty(fields.get(4).copied()), package_preference };
        Some((caption, non_empty(fields.get(5).copied()), source))
    }

    /// Base URL of the repository root, ending in `/`.
    pub fn base_url(&self) -> String {
        let dir = self.directory.trim_matches('/');
        let path = if dir.is_empty() { "/".to_string() } else { format!("/{dir}/") };
        if self.protocol == Protocol::File {
            return format!("file://{path}");
        }
        let credentials = match (&self.user, &self.password) {
            (Some(u), Some(p)) => format!("{}:{}@", percent_encode(u), percent_encode(p)),
            (Some(u), None) => format!("{}@", percent_encode(u)),
            _ => String::new(),
        };
        format!("{}://{credentials}{}{path}", self.protocol.scheme(), self.host)
    }

    /// Build a source from a URL such as `https://ebible.org/sword`.
    pub fn from_url(url: &str) -> Option<Source> {
        let (scheme, rest) = url.trim().split_once("://")?;
        let protocol = Protocol::from_scheme(scheme)?;
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        let (credentials, host) = match authority.rsplit_once('@') {
            Some((c, h)) => (Some(c), h),
            None => (None, authority),
        };
        let (user, password) = match credentials.map(|c| c.split_once(':').unwrap_or((c, ""))) {
            Some((u, p)) => (non_empty(Some(&transport::percent_decode(u))), non_empty(Some(&transport::percent_decode(p)))),
            None => (None, None),
        };
        let host = if protocol == Protocol::File { host.strip_prefix("localhost").unwrap_or(host) } else { host };
        if host.is_empty() && protocol != Protocol::File {
            return None;
        }
        let directory = transport::percent_decode(path.trim_end_matches('/'));
        Some(Source { protocol, host: host.to_string(), directory, user, password, package_preference: false })
    }
}

/// A repository (e.g. "CrossWire", "eBible.org") with one or more sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    pub name: String,
    /// Stable id from the master list (e.g. `20081216195754`), if any.
    pub uid: Option<String>,
    /// Sources in the order they are tried.
    pub sources: Vec<Source>,
}

impl Repository {
    /// A repository reachable at one URL, e.g.
    /// `Repository::from_url("eBible.org", "https://ebible.org/sword")`.
    pub fn from_url(name: impl Into<String>, url: &str) -> Result<Repository> {
        let source = Source::from_url(url).ok_or_else(|| Error::Install(format!("not a repository URL: {url}")))?;
        Ok(Repository { name: name.into(), uid: None, sources: vec![source] })
    }

    /// CrossWire's main repository (HTTPS, falling back to FTP).
    pub fn crosswire() -> Repository {
        let source = |protocol, host: &str, directory: &str| Source { protocol, host: host.into(), directory: directory.into(), user: None, password: None, package_preference: false };
        Repository {
            name: "CrossWire".into(),
            uid: Some("20081216195754".into()),
            sources: vec![source(Protocol::Https, "crosswire.org", "/ftpmirror/pub/sword/raw"), source(Protocol::Ftp, "ftp.crosswire.org", "/pub/sword/raw")],
        }
    }
}

/// Gather sources into repositories by caption, package-preference sources first. Each
/// line of the master list has its own uid; a repository keeps the uid of its first
/// ordinary (`…Source`) entry, else of its first entry.
fn group_sources(found: Vec<(String, Option<String>, Source)>) -> Vec<Repository> {
    let mut repos: Vec<Repository> = Vec::new();
    for (caption, uid, source) in found {
        let preferred = source.package_preference;
        match repos.iter_mut().find(|r| r.name == caption) {
            Some(repo) => {
                let repo_uid_from_preference = repo.sources.iter().all(|s| s.package_preference);
                if uid.is_some() && (repo.uid.is_none() || (repo_uid_from_preference && !preferred)) {
                    repo.uid = uid;
                }
                if !repo.sources.contains(&source) {
                    repo.sources.push(source);
                }
            }
            None => repos.push(Repository { name: caption, uid, sources: vec![source] }),
        }
    }
    for repo in &mut repos {
        repo.sources.sort_by_key(|s| !s.package_preference);
    }
    repos
}

/// Parse CrossWire's `masterRepoList.conf` (`[Repos]` with `uid=TypeSource=…` lines).
pub fn parse_master_repo_list(text: &str) -> Vec<Repository> {
    let Ok(sections) = parse_conf(text) else { return Vec::new() };
    let mut found = Vec::new();
    for section in sections.iter().filter(|s| s.name().eq_ignore_ascii_case("Repos")) {
        for (uid, value) in section.entries() {
            let Some((kind, entry)) = value.split_once('=') else { continue };
            if let Some((caption, own_uid, source)) = Source::parse_entry(kind, entry) {
                let uid = own_uid.or_else(|| non_empty(Some(uid)));
                found.push((caption, uid, source));
            }
        }
    }
    group_sources(found)
}

/// Parse a libsword `InstallMgr.conf` (`[Sources]` with `HTTPSSource=…` etc.), e.g.
/// `~/.sword/InstallMgr/InstallMgr.conf`, to reuse a user's configured repositories.
pub fn parse_install_conf(text: &str) -> Vec<Repository> {
    let Ok(sections) = parse_conf(text) else { return Vec::new() };
    let found = sections.iter().filter(|s| s.name().eq_ignore_ascii_case("Sources")).flat_map(|s| s.entries().iter().filter_map(|(kind, value)| Source::parse_entry(kind, value))).collect();
    group_sources(found)
}

/// A module available in a repository.
#[derive(Debug, Clone)]
pub struct RemoteModule {
    pub conf: ModuleConf,
    /// File name in `mods.d/` (e.g. `kjv.conf`).
    pub conf_file: String,
    /// The conf file exactly as published.
    pub conf_bytes: Vec<u8>,
}

impl RemoteModule {
    pub fn name(&self) -> &str {
        self.conf.name()
    }
}

/// The modules a repository offers.
#[derive(Debug, Clone)]
pub struct Catalog {
    pub repository: Repository,
    pub modules: Vec<RemoteModule>,
}

impl Catalog {
    /// Find a module by name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<&RemoteModule> {
        self.modules.iter().find(|m| m.name() == name).or_else(|| self.modules.iter().find(|m| m.name().eq_ignore_ascii_case(name)))
    }

    /// Modules of one kind (Bibles, commentaries, …).
    pub fn of_kind(&self, kind: ModuleKind) -> impl Iterator<Item = &RemoteModule> {
        self.modules.iter().filter(move |m| m.conf.module_kind().ok() == Some(kind))
    }
}

/// How a remote module relates to what is installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleStatus {
    NotInstalled,
    /// Remote `Version` is newer.
    UpdateAvailable,
    SameVersion,
    /// Installed copy is newer than the repository's.
    InstalledIsNewer,
}

/// Compare SWORD version strings (`1.2.10` > `1.2.9`; missing parts count as 0).
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let parts = |s: &str| -> Vec<(u64, String)> {
        s.trim()
            .split('.')
            .map(|p| {
                let digits = p.chars().take_while(char::is_ascii_digit).count();
                (p[..digits].parse().unwrap_or(0), p[digits..].to_string())
            })
            .collect()
    };
    let (pa, pb) = (parts(a), parts(b));
    let zero = (0, String::new());
    for i in 0..pa.len().max(pb.len()) {
        let (x, y) = (pa.get(i).unwrap_or(&zero), pb.get(i).unwrap_or(&zero));
        match x.0.cmp(&y.0).then_with(|| x.1.cmp(&y.1)) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    Ordering::Equal
}

/// Status of every module in `catalog` relative to `installed`.
pub fn module_status<'a>(catalog: &'a Catalog, installed: &SwordLibrary) -> Vec<(&'a RemoteModule, ModuleStatus)> {
    catalog
        .modules
        .iter()
        .map(|m| {
            let status = match installed.conf(m.name()) {
                None => ModuleStatus::NotInstalled,
                Some(local) => match compare_versions(m.conf.version().unwrap_or("0"), local.version().unwrap_or("0")) {
                    Ordering::Greater => ModuleStatus::UpdateAvailable,
                    Ordering::Equal => ModuleStatus::SameVersion,
                    Ordering::Less => ModuleStatus::InstalledIsNewer,
                },
            };
            (m, status)
        })
        .collect()
}

/// The repository a module came from, recorded in its installed conf.
struct Origin<'a> {
    caption: &'a str,
    uid: Option<&'a str>,
}

/// Downloads catalogues and installs modules.
pub struct Installer {
    transport: Box<dyn Transport>,
}

impl Default for Installer {
    fn default() -> Self {
        Self::new()
    }
}

impl Installer {
    /// Installer using [`DefaultTransport`].
    pub fn new() -> Self {
        Installer::with_transport(DefaultTransport::new())
    }

    /// Installer using your own [`Transport`].
    pub fn with_transport(transport: impl Transport + 'static) -> Self {
        Installer { transport: Box::new(transport) }
    }

    pub fn transport(&self) -> &dyn Transport {
        self.transport.as_ref()
    }

    /// Fetch and parse CrossWire's list of known repositories.
    pub fn master_repo_list(&self) -> Result<Vec<Repository>> {
        let bytes = match self.transport.get(MASTER_REPO_LIST_URL) {
            Ok(b) => b,
            Err(_) => self.transport.get(MASTER_REPO_LIST_FTP_URL)?,
        };
        Ok(parse_master_repo_list(&decode_conf_bytes(&bytes)))
    }

    /// Fetch a repository's catalogue (`mods.d.tar.gz`, or `mods.d/*.conf`).
    pub fn catalog(&self, repo: &Repository) -> Result<Catalog> {
        let mut last = None;
        for source in &repo.sources {
            match self.catalog_from(&source.base_url()) {
                Ok(mut modules) => {
                    modules.sort_by(|a, b| a.conf_file.cmp(&b.conf_file));
                    return Ok(Catalog { repository: repo.clone(), modules });
                }
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| Error::Install(format!("repository {} has no sources", repo.name))))
    }

    fn catalog_from(&self, base: &str) -> Result<Vec<RemoteModule>> {
        match self.transport.get(&format!("{base}mods.d.tar.gz")) {
            Ok(bytes) => confs_from_tar_gz(&bytes),
            Err(Error::NotFound(_)) => {
                let dir = format!("{base}mods.d/");
                let mut modules = Vec::new();
                for entry in self.transport.list(&dir)? {
                    if entry.is_dir || !entry.name.to_ascii_lowercase().ends_with(".conf") || !is_safe_name(&entry.name) {
                        continue;
                    }
                    let bytes = self.transport.get(&format!("{dir}{}", percent_encode(&entry.name)))?;
                    modules.extend(remote_module(entry.name, bytes));
                }
                Ok(modules)
            }
            Err(e) => Err(e),
        }
    }

    /// Install (or update) a module from a catalogue into the library rooted at
    /// `library_root` (created if needed). Returns the installed module names.
    pub fn install(&self, catalog: &Catalog, module: &str, library_root: impl AsRef<Path>) -> Result<Vec<String>> {
        let root = library_root.as_ref();
        let remote = catalog.get(module).ok_or_else(|| Error::ModuleNotFound(module.to_string()))?;
        let origin = Origin { caption: &catalog.repository.name, uid: catalog.repository.uid.as_deref() };
        let mut last = None;
        for source in &catalog.repository.sources {
            let base = source.base_url();
            match self.package(&base, remote.name()) {
                Ok(Some(zip)) => return install_zip_from(&zip, root, Some(&origin)),
                Ok(None) => {}
                Err(e) => {
                    last = Some(e);
                    continue;
                }
            }
            match self.install_files(remote, &base, root, &origin) {
                Ok(names) => return Ok(names),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| Error::Install(format!("repository {} has no sources", catalog.repository.name))))
    }

    /// The module's package zip, if the repository has one.
    fn package(&self, base: &str, name: &str) -> Result<Option<Vec<u8>>> {
        let mut names = vec![name.to_string()];
        if name.to_lowercase() != name {
            names.push(name.to_lowercase());
        }
        for n in names {
            match self.transport.get(&format!("{base}packages/{}.zip", percent_encode(&n))) {
                Ok(bytes) => return Ok(Some(bytes)),
                Err(Error::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }

    /// Copy a module's data directory file by file, then install it with its conf.
    fn install_files(&self, remote: &RemoteModule, base: &str, root: &Path, origin: &Origin) -> Result<Vec<String>> {
        if !is_safe_name(&remote.conf_file) {
            return Err(Error::Install(format!("unsafe conf file name {:?}", remote.conf_file)));
        }
        let data_dir = module_data_dir(&remote.conf)?;
        let staging = Staging::new(root)?;
        let url_dir: Vec<String> = data_dir.iter().map(|c| percent_encode(&c.to_string_lossy())).collect();
        let mut budget = SIZE_CAP;
        self.copy_dir(&format!("{base}{}/", url_dir.join("/")), &staging.path.join(&data_dir), 0, &mut budget)?;
        let conf_bytes = record_origin(&remote.conf_bytes, remote.name(), origin);
        let conf = remote.conf.clone();
        check_opens(&staging.path, &conf)?;
        staging.commit(root, &[Staged { name: conf.name().to_string(), conf_file: remote.conf_file.clone(), conf_bytes, data_dirs: vec![data_dir] }])
    }

    fn copy_dir(&self, url: &str, dest: &Path, depth: usize, budget: &mut u64) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err(Error::Install(format!("{url}: directories nest too deeply")));
        }
        std::fs::create_dir_all(dest).map_err(|e| Error::io(dest, e))?;
        for entry in self.transport.list(url)? {
            if !is_safe_name(&entry.name) {
                return Err(Error::Install(format!("{url}: unsafe file name {:?}", entry.name)));
            }
            let child = format!("{url}{}", percent_encode(&entry.name));
            if entry.is_dir {
                self.copy_dir(&format!("{child}/"), &dest.join(&entry.name), depth + 1, budget)?;
            } else {
                let bytes = self.transport.get(&child)?;
                *budget = budget.checked_sub(bytes.len() as u64).ok_or_else(|| Error::Install(format!("{url}: module is larger than 4 GiB")))?;
                let path = dest.join(&entry.name);
                std::fs::write(&path, bytes).map_err(|e| Error::io(path, e))?;
            }
        }
        Ok(())
    }

    /// Download a module zip from any URL (`https://…/KJV.zip`, `ftp://…`,
    /// `file://…`) and install every module it contains.
    pub fn install_from_url(&self, url: &str, library_root: impl AsRef<Path>) -> Result<Vec<String>> {
        let bytes = self.transport.get(url)?;
        install_zip(&bytes, library_root)
    }
}

/// One module (or one conf file's modules) ready to move into place.
struct Staged {
    name: String,
    conf_file: String,
    conf_bytes: Vec<u8>,
    data_dirs: Vec<PathBuf>,
}

/// A directory inside the library where downloads are unpacked; removed when dropped.
struct Staging {
    path: PathBuf,
}

impl Staging {
    fn new(root: &Path) -> Result<Staging> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let unique = format!("{}-{}-{nanos}", std::process::id(), COUNTER.fetch_add(1, AtomicOrdering::SeqCst));
        let path = root.join(STAGING).join(unique);
        std::fs::create_dir_all(&path).map_err(|e| Error::io(&path, e))?;
        Ok(Staging { path })
    }

    /// Move staged data directories and confs into the library.
    fn commit(self, root: &Path, modules: &[Staged]) -> Result<Vec<String>> {
        let mods_d = root.join("mods.d");
        std::fs::create_dir_all(&mods_d).map_err(|e| Error::io(&mods_d, e))?;
        let mut names = Vec::new();
        for m in modules {
            for dir in &m.data_dirs {
                let from = self.path.join(dir);
                if !from.exists() {
                    continue;
                }
                let to = root.join(dir);
                if to.exists() {
                    std::fs::remove_dir_all(&to).map_err(|e| Error::io(&to, e))?;
                }
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
                }
                std::fs::rename(&from, &to).map_err(|e| Error::io(&to, e))?;
            }
            // An older conf for the same module under another file name would shadow this one.
            for (file, confs) in installed_confs(root) {
                if file.file_name().is_some_and(|n| n.to_string_lossy() != m.conf_file) && confs.iter().any(|c| c.name().eq_ignore_ascii_case(&m.name)) {
                    let _ = std::fs::remove_file(&file);
                }
            }
            let conf_path = mods_d.join(&m.conf_file);
            std::fs::write(&conf_path, &m.conf_bytes).map_err(|e| Error::io(&conf_path, e))?;
            names.push(m.name.clone());
        }
        Ok(names)
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
        if let Some(parent) = self.path.parent() {
            // Only removed when empty (another installation may be using it).
            let _ = std::fs::remove_dir(parent);
        }
    }
}

/// Every conf file of a library with its module sections.
fn installed_confs(root: &Path) -> Vec<(PathBuf, Vec<ModuleConf>)> {
    conf_files(root).unwrap_or_default().into_iter().filter_map(|f| read_module_confs(&f).ok().map(|c| (f, c))).collect()
}

/// A plain file or directory name: no separators, no `.`/`..`, no control characters.
fn is_safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0']) && !name.chars().any(char::is_control)
}

/// A relative archive path made safe, or `None` if it could escape its directory.
fn safe_relative(path: &str) -> Option<PathBuf> {
    let path = path.replace('\\', "/");
    if path.starts_with('/') {
        return None;
    }
    let mut out = PathBuf::new();
    for part in path.split('/').filter(|p| !p.is_empty() && *p != ".") {
        if !is_safe_name(part) {
            return None;
        }
        out.push(part);
    }
    Some(out)
}

/// The modules of a `mods.d.tar.gz`.
fn confs_from_tar_gz(bytes: &[u8]) -> Result<Vec<RemoteModule>> {
    let bad = |e: std::io::Error| Error::Install(format!("mods.d.tar.gz: {e}"));
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut modules = Vec::new();
    for entry in archive.entries().map_err(bad)? {
        let mut entry = entry.map_err(bad)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(bad)?.to_string_lossy().into_owned();
        let Some(file) = path.rsplit('/').next().map(str::to_string) else { continue };
        if !file.to_ascii_lowercase().ends_with(".conf") || !is_safe_name(&file) {
            continue;
        }
        let mut data = Vec::new();
        entry.by_ref().take(1 << 20).read_to_end(&mut data).map_err(bad)?;
        modules.extend(remote_module(file, data));
    }
    Ok(modules)
}

/// A catalogue entry from a conf file (its first module section).
fn remote_module(conf_file: String, conf_bytes: Vec<u8>) -> Option<RemoteModule> {
    let confs = parse_conf(&decode_conf_bytes(&conf_bytes)).ok()?;
    let conf = confs.iter().find(|c| c.get("ModDrv").is_some()).or(confs.first())?.clone();
    Some(RemoteModule { conf, conf_file, conf_bytes })
}

/// Replace (or add) `key=value` in the `[section]` of conf bytes, keeping everything else.
fn set_conf_value(bytes: &[u8], section: &str, key: &str, value: &str) -> Vec<u8> {
    let lines: Vec<&[u8]> = bytes.split_inclusive(|&b| b == b'\n').collect();
    let crlf = lines.first().is_some_and(|l| l.ends_with(b"\r\n"));
    let eol: &[u8] = if crlf { b"\r\n" } else { b"\n" };
    let new_line = [format!("{key}={value}").as_bytes(), eol].concat();
    let prefix = format!("{key}=");
    let mut out = Vec::with_capacity(bytes.len() + new_line.len());
    let mut in_section = false;
    let mut header_end = None;
    let mut written = false;
    for line in lines {
        let text = String::from_utf8_lossy(line);
        let trimmed = text.trim_start();
        if let Some(rest) = trimmed.strip_prefix('[') {
            in_section = rest.split(']').next().is_some_and(|n| n.eq_ignore_ascii_case(section));
            out.extend_from_slice(line);
            if !line.ends_with(b"\n") {
                out.extend_from_slice(eol);
            }
            if in_section && header_end.is_none() {
                header_end = Some(out.len());
            }
            continue;
        }
        if in_section && trimmed.starts_with(&prefix) {
            if !written {
                out.extend_from_slice(&new_line);
                written = true;
            }
            continue;
        }
        out.extend_from_slice(line);
    }
    if !written {
        match header_end {
            Some(at) => {
                out.splice(at..at, new_line);
            }
            None => {
                if !out.is_empty() && !out.ends_with(b"\n") {
                    out.extend_from_slice(eol);
                }
                out.extend_from_slice(&new_line);
            }
        }
    }
    out
}

/// Note the repository in an installed conf (`InstallSourceCaption`, `InstallSourceID`).
fn record_origin(conf_bytes: &[u8], section: &str, origin: &Origin) -> Vec<u8> {
    let mut bytes = set_conf_value(conf_bytes, section, "InstallSourceCaption", origin.caption);
    if let Some(uid) = origin.uid {
        bytes = set_conf_value(&bytes, section, "InstallSourceID", uid);
    }
    bytes
}

/// Opening errors that still allow installing: the module may be readable elsewhere.
fn check_opens(staging_root: &Path, conf: &ModuleConf) -> Result<()> {
    match Module::open(staging_root, conf.clone()) {
        Ok(_) | Err(Error::Locked(_) | Error::UnsupportedDriver(_) | Error::UnsupportedCompression(_) | Error::UnknownVersification(_)) => Ok(()),
        Err(e) => Err(Error::Install(format!("{} does not open: {e}", conf.name()))),
    }
}

/// Install every module contained in a SWORD module zip (as distributed by
/// CrossWire, AndBible, eBible.org, …). Returns the installed module names.
pub fn install_zip(zip_bytes: &[u8], library_root: impl AsRef<Path>) -> Result<Vec<String>> {
    install_zip_from(zip_bytes, library_root.as_ref(), None)
}

fn install_zip_from(zip_bytes: &[u8], root: &Path, origin: Option<&Origin>) -> Result<Vec<String>> {
    let mut archive = zip_archive::ZipArchive::new(Cursor::new(zip_bytes)).map_err(|e| Error::Install(format!("not a zip archive: {e}")))?;
    // Check every name before writing anything.
    let mut files = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i).map_err(|e| Error::Install(format!("zip entry {i}: {e}")))?;
        let name = entry.name().to_string();
        let path = safe_relative(&name).ok_or_else(|| Error::Install(format!("unsafe path in zip: {name:?}")))?;
        let top = path.iter().next().map(|c| c.to_string_lossy().into_owned());
        if !matches!(top.as_deref(), Some("mods.d" | "modules")) || entry.is_dir() || path.iter().count() < 2 {
            continue;
        }
        files.push((i, path));
    }
    if !files.iter().any(|(_, p)| p.starts_with("mods.d") && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("conf"))) {
        return Err(Error::Install("the zip contains no module (no mods.d/*.conf)".into()));
    }
    let staging = Staging::new(root)?;
    let mut budget = SIZE_CAP;
    for (i, path) in &files {
        let mut entry = archive.by_index(*i).map_err(|e| Error::Install(format!("zip entry {}: {e}", path.display())))?;
        let dest = staging.path.join(path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let mut data = Vec::new();
        entry.by_ref().take(budget + 1).read_to_end(&mut data).map_err(|e| Error::Install(format!("zip entry {}: {e}", path.display())))?;
        budget = budget.checked_sub(data.len() as u64).ok_or_else(|| Error::Install("the zip unpacks to more than 4 GiB".into()))?;
        std::fs::write(&dest, data).map_err(|e| Error::io(&dest, e))?;
    }
    let mut staged = Vec::new();
    for conf_path in conf_files(&staging.path)? {
        let conf_file = conf_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut conf_bytes = std::fs::read(&conf_path).map_err(|e| Error::io(&conf_path, e))?;
        let confs = read_module_confs(&conf_path)?;
        let mut data_dirs = Vec::new();
        for conf in &confs {
            data_dirs.push(module_data_dir(conf)?);
            check_opens(&staging.path, conf)?;
            if let Some(origin) = origin {
                conf_bytes = record_origin(&conf_bytes, conf.name(), origin);
            }
        }
        let names: Vec<String> = confs.iter().map(|c| c.name().to_string()).collect();
        if let Some(name) = names.first() {
            staged.push(Staged { name: name.clone(), conf_file, conf_bytes, data_dirs });
        }
    }
    if staged.is_empty() {
        return Err(Error::Install("the zip contains no module (no conf with ModDrv)".into()));
    }
    staging.commit(root, &staged)
}

/// Install from a local `.zip` file.
pub fn install_zip_file(zip_path: impl AsRef<Path>, library_root: impl AsRef<Path>) -> Result<Vec<String>> {
    let path = zip_path.as_ref();
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    install_zip(&bytes, library_root)
}

/// The directory holding a module's data, relative to the library root. For
/// dictionaries and general books `DataPath` is a file prefix, so its parent is used.
/// Refuses anything that is not safely inside `modules/`.
pub fn module_data_dir(conf: &ModuleConf) -> Result<PathBuf> {
    let refuse = |why: &str| Error::Install(format!("{}: DataPath {:?} {why}", conf.name(), conf.get("DataPath").unwrap_or("")));
    let data_path = conf.data_path().ok_or_else(|| refuse("is missing"))?.replace('\\', "/");
    let parts: Vec<&str> = data_path.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    let parts = match conf.module_kind() {
        Ok(ModuleKind::Dictionary | ModuleKind::GenBook) if !data_path.ends_with('/') => &parts[..parts.len().saturating_sub(1)],
        _ => &parts[..],
    };
    if parts.iter().any(|p| !is_safe_name(p)) {
        return Err(refuse("leaves the library"));
    }
    if parts.first() != Some(&"modules") || parts.len() < 3 {
        return Err(refuse("is not a module directory inside modules/"));
    }
    // `modules/texts/ztext` and the like hold every module of a driver.
    if parts.len() == 3 && matches!(parts[1], "texts" | "comments" | "lexdict" | "genbook") {
        return Err(refuse("is a shared directory, not a module's own"));
    }
    Ok(parts.iter().collect())
}

/// The conf file declaring an installed module, and its conf.
fn find_installed(root: &Path, module: &str) -> Result<(PathBuf, ModuleConf)> {
    for (file, confs) in installed_confs(root) {
        if let Some(conf) = confs.into_iter().find(|c| c.name().eq_ignore_ascii_case(module)) {
            return Ok((file, conf));
        }
    }
    Err(Error::ModuleNotFound(module.to_string()))
}

/// Remove an installed module: its data directory and its `.conf` file.
pub fn uninstall(library_root: impl AsRef<Path>, module: &str) -> Result<()> {
    let root = library_root.as_ref();
    let (file, conf) = find_installed(root, module)?;
    let dir = root.join(module_data_dir(&conf)?);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    }
    std::fs::remove_file(&file).map_err(|e| Error::io(&file, e))
}

/// Store a `CipherKey` for an installed module in its `.conf` (as libsword's
/// installers do), so it opens without supplying the key each time.
pub fn save_cipher_key(library_root: impl AsRef<Path>, module: &str, key: &str) -> Result<()> {
    let root = library_root.as_ref();
    let (file, conf) = find_installed(root, module)?;
    let bytes = std::fs::read(&file).map_err(|e| Error::io(&file, e))?;
    std::fs::write(&file, set_conf_value(&bytes, conf.name(), "CipherKey", key)).map_err(|e| Error::io(&file, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_and_lists() {
        let (caption, uid, s) = Source::parse_entry("FTPSource", "CrossWire|ftp.crosswire.org|/pub/sword/raw|||").unwrap();
        assert_eq!((caption.as_str(), uid, s.protocol, s.base_url()), ("CrossWire", None, Protocol::Ftp, "ftp://ftp.crosswire.org/pub/sword/raw/".to_string()));
        assert!(Source::parse_entry("Other", "a|b|c").is_none());
        let list = "[Repos]\n110=HTTPSPackagePreference=CrossWire|crosswire.org|/ftpmirror/pub/sword/raw\n111=FTPSource=CrossWire|ftp.crosswire.org|/pub/sword/raw\n#9=FTPSource=Old|x|/y\n222=HTTPSSource=eBible.org|ebible.org|/sword\n";
        let repos = parse_master_repo_list(list);
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].uid.as_deref(), Some("111"));
        assert_eq!(repos[0].sources[0].protocol, Protocol::Https);
        assert!(repos[0].sources[0].package_preference);
        assert_eq!(repos[1].sources[0].base_url(), "https://ebible.org/sword/");
        let conf = "[General]\nPassiveFTP=true\n[Sources]\nFTPSource=Bible.org|ftp.bible.org|/sword\nHTTPSource=IBT|ibt.org.ru|/ftp/pub/sword|u|p|33\n";
        let repos = parse_install_conf(conf);
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[1].uid.as_deref(), Some("33"));
        assert_eq!(repos[1].sources[0].base_url(), "http://u:p@ibt.org.ru/ftp/pub/sword/");
        let s = Source::from_url("https://ebible.org/sword/").unwrap();
        assert_eq!((s.host.as_str(), s.directory.as_str(), s.base_url()), ("ebible.org", "/sword", "https://ebible.org/sword/".to_string()));
        assert_eq!(Source::from_url("file:///srv/repo").unwrap().base_url(), "file:///srv/repo/");
        assert!(Source::from_url("gopher://x/").is_none());
        assert_eq!(Repository::crosswire().sources.len(), 2);
    }

    #[test]
    fn versions() {
        assert_eq!(compare_versions("1.2.10", "1.2.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.2", "1.2.0"), Ordering::Equal);
        assert_eq!(compare_versions("2.0", "10"), Ordering::Less);
        assert_eq!(compare_versions("1.0b", "1.0a"), Ordering::Greater);
    }

    #[test]
    fn data_dirs() {
        let conf = |drv: &str, path: &str| ModuleConf::new("M", vec![("ModDrv".into(), drv.into()), ("DataPath".into(), path.into())]);
        assert_eq!(module_data_dir(&conf("zText", "./modules/texts/ztext/kjv/")).unwrap(), PathBuf::from("modules/texts/ztext/kjv"));
        assert_eq!(module_data_dir(&conf("RawLD", "./modules/lexdict/rawld/strongs/strongs")).unwrap(), PathBuf::from("modules/lexdict/rawld/strongs"));
        for bad in ["./", "./modules/texts/ztext/", "../modules/texts/x/y/", "./modules/texts/ztext/../../../x/", "/etc/passwd/", "./other/a/b/c/"] {
            assert!(matches!(module_data_dir(&conf("zText", bad)), Err(Error::Install(_))), "{bad}");
        }
        assert!(safe_relative("../x").is_none());
        assert!(safe_relative("/x").is_none());
        assert_eq!(safe_relative("./mods.d//a.conf"), Some(PathBuf::from("mods.d/a.conf")));
    }

    #[test]
    fn conf_editing() {
        let text = b"[A]\r\nModDrv=zText\r\nCipherKey=\r\n";
        assert_eq!(set_conf_value(text, "A", "CipherKey", "k"), b"[A]\r\nModDrv=zText\r\nCipherKey=k\r\n");
        assert_eq!(set_conf_value(b"[A]\nX=1", "a", "Y", "2"), b"[A]\nY=2\nX=1");
        assert_eq!(set_conf_value(b"[A]\nX=1\n[B]\nX=1\n", "B", "X", "2"), b"[A]\nX=1\n[B]\nX=2\n");
    }
}
