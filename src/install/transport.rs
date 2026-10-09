//! Fetching files and directory listings over HTTP(S), FTP and `file://`.

use std::path::PathBuf;
use std::time::Duration;

use crate::error::{Error, Result};

/// One entry of a remote directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    /// Size in bytes if the listing reports it.
    pub size: Option<u64>,
}

/// How the installer talks to the outside world.
pub trait Transport: Send + Sync {
    /// Download a whole file. Must return [`Error::NotFound`] for missing files
    /// (HTTP 404, FTP 550) so the installer can fall back to other locations.
    fn get(&self, url: &str) -> Result<Vec<u8>>;

    /// List a directory (`url` ends with `/`). Only needed for repositories that
    /// lack `packages/<Module>.zip`. The default parses an HTML index page (the
    /// way libsword does for HTTP repositories).
    fn list(&self, url: &str) -> Result<Vec<DirEntry>> {
        let html = self.get(url)?;
        Ok(parse_html_listing(&String::from_utf8_lossy(&html), url))
    }
}

/// Decode `%xx` escapes (invalid ones are kept as written).
pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(b) = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Escape a path segment for a URL.
pub(crate) fn percent_encode(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for b in segment.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'=' | b':' | b'@') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The path part of a URL (`/a/b/` of `http://host/a/b/`).
fn url_path(url: &str) -> &str {
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    after_scheme.find('/').map_or("/", |i| &after_scheme[i..])
}

/// Read the entries of an HTML directory index (Apache, nginx, lighttpd and the like):
/// every link to a name inside the directory. Sorting links, parent links and links
/// elsewhere are skipped.
fn parse_html_listing(html: &str, url: &str) -> Vec<DirEntry> {
    let base_path = percent_decode(url_path(url));
    let mut out: Vec<DirEntry> = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find("href") {
        let at = from + found + 4;
        from = at;
        let rest = html[at..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let rest = rest.trim_start();
        let href = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
            _ => rest.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or(""),
        };
        let href = href.split(['?', '#']).next().unwrap_or("");
        if href.is_empty() || href.contains("://") || href.starts_with("mailto:") {
            continue;
        }
        let mut target = percent_decode(href).replace("&amp;", "&");
        if let Some(abs) = target.strip_prefix('/') {
            // An absolute link counts only when it points inside this directory.
            match format!("/{abs}").strip_prefix(base_path.as_str()) {
                Some(inside) => target = inside.to_string(),
                None => continue,
            }
        }
        let target = target.strip_prefix("./").unwrap_or(&target);
        let is_dir = target.ends_with('/');
        let name = target.trim_end_matches('/');
        if name.is_empty() || name == "." || name == ".." || name.contains('/') {
            continue;
        }
        if !out.iter().any(|e| e.name == name) {
            out.push(DirEntry { name: name.to_string(), is_dir, size: None });
        }
    }
    out
}

/// A `file://` URL's local path.
pub(crate) fn file_url_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let mut path = percent_decode(rest);
    // `file:///C:/x` on Windows.
    if path.len() > 2 && path.starts_with('/') && path.as_bytes()[2] == b':' {
        path.remove(0);
    }
    Some(PathBuf::from(path))
}

/// Built-in transport: HTTP(S) via `ureq` (feature `https`), FTP, `file://`.
pub struct DefaultTransport {
    timeout: Duration,
    #[cfg(feature = "https")]
    agent: ureq::Agent,
}

impl Default for DefaultTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultTransport {
    pub fn new() -> Self {
        Self::with_timeout(Duration::from_secs(30))
    }

    /// Per-connection timeout for connecting and reading. (Over HTTP, a whole download
    /// may take twenty times as long.)
    pub fn with_timeout(timeout: std::time::Duration) -> Self {
        DefaultTransport {
            timeout,
            #[cfg(feature = "https")]
            agent: ureq::Agent::new_with_config(ureq::Agent::config_builder().timeout_connect(Some(timeout)).timeout_recv_response(Some(timeout)).timeout_recv_body(Some(timeout * 20)).build()),
        }
    }

    #[cfg(feature = "https")]
    fn http_get(&self, url: &str) -> Result<Vec<u8>> {
        const LIMIT: u64 = 4 << 30;
        match self.agent.get(url).call() {
            Ok(mut response) => response.body_mut().with_config().limit(LIMIT).read_to_vec().map_err(|e| Error::Network { url: url.to_string(), message: e.to_string() }),
            Err(ureq::Error::StatusCode(404 | 410)) => Err(Error::NotFound(url.to_string())),
            Err(e) => Err(Error::Network { url: url.to_string(), message: e.to_string() }),
        }
    }

    #[cfg(not(feature = "https"))]
    fn http_get(&self, url: &str) -> Result<Vec<u8>> {
        Err(Error::Network { url: url.to_string(), message: "HTTP needs the `https` feature or your own Transport".into() })
    }
}

fn scheme(url: &str) -> String {
    url.split_once("://").map(|(s, _)| s.to_ascii_lowercase()).unwrap_or_default()
}

impl Transport for DefaultTransport {
    fn get(&self, url: &str) -> Result<Vec<u8>> {
        match scheme(url).as_str() {
            "http" | "https" => self.http_get(url),
            "ftp" => super::ftp::get(url, self.timeout),
            "file" => {
                let path = file_url_path(url).ok_or_else(|| Error::NotFound(url.to_string()))?;
                match std::fs::read(&path) {
                    Ok(bytes) => Ok(bytes),
                    Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::IsADirectory) => Err(Error::NotFound(url.to_string())),
                    Err(e) => Err(Error::io(path, e)),
                }
            }
            other => Err(Error::Network { url: url.to_string(), message: format!("unsupported protocol {other:?}") }),
        }
    }

    fn list(&self, url: &str) -> Result<Vec<DirEntry>> {
        match scheme(url).as_str() {
            "ftp" => super::ftp::list(url, self.timeout),
            "file" => {
                let path = file_url_path(url).ok_or_else(|| Error::NotFound(url.to_string()))?;
                let entries = match std::fs::read_dir(&path) {
                    Ok(entries) => entries,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Error::NotFound(url.to_string())),
                    Err(e) => return Err(Error::io(path, e)),
                };
                let mut out: Vec<DirEntry> = entries
                    .flatten()
                    .map(|e| {
                        let meta = e.metadata().ok();
                        let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
                        DirEntry { name: e.file_name().to_string_lossy().into_owned(), is_dir, size: meta.filter(|m| m.is_file()).map(|m| m.len()) }
                    })
                    .collect();
                out.sort_by(|a, b| a.name.cmp(&b.name));
                Ok(out)
            }
            _ => {
                let html = self.get(url)?;
                Ok(parse_html_listing(&String::from_utf8_lossy(&html), url))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_listings() {
        let html = r#"<html><body><pre><a href="?C=N;O=D">Name</a> <a href="/">Parent Directory</a>
<a href="../">../</a>
<a href="ot.bzs">ot.bzs</a>  2026-10-08 1K
<A HREF='images/'>images/</A>
<a href="My%20File.txt">My File.txt</a>
<a href="/repo/modules/x/abs.dat">abs.dat</a>
<a href="/elsewhere/y">y</a>
<a href="https://example.org/z">z</a>
<a href="ot.bzs">again</a>"#;
        let entries = parse_html_listing(html, "http://host/repo/modules/x/");
        let names: Vec<(&str, bool)> = entries.iter().map(|e| (e.name.as_str(), e.is_dir)).collect();
        assert_eq!(names, [("ot.bzs", false), ("images", true), ("My File.txt", false), ("abs.dat", false)]);
    }

    #[test]
    fn urls() {
        assert_eq!(percent_decode("a%20b%zz"), "a b%zz");
        assert_eq!(percent_encode("a b/é"), "a%20b%2F%C3%A9");
        assert_eq!(file_url_path("file:///tmp/x%20y/"), Some(PathBuf::from("/tmp/x y/")));
        assert_eq!(file_url_path("file://localhost/tmp"), Some(PathBuf::from("/tmp")));
    }
}
