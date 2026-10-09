//! The crate's error type.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Errors produced while reading SWORD modules.
#[derive(Debug)]
pub enum Error {
    /// Underlying I/O failure, with the file involved when known.
    Io { path: Option<PathBuf>, source: io::Error },
    /// A `.conf` file is malformed or lacks a required entry.
    Conf(String),
    /// The module uses a driver (`ModDrv`) this crate does not implement.
    UnsupportedDriver(String),
    /// The module's `CompressType` is unknown, or its cargo feature is disabled.
    UnsupportedCompression(String),
    /// The `Versification` named in the conf is not one of SWORD's built-in systems.
    UnknownVersification(String),
    /// The module is enciphered and no `CipherKey` was supplied (a "locked" module).
    Locked(String),
    /// Index or data files are inconsistent (truncated, out-of-range offsets, …).
    Corrupt(String),
    /// Decompression failed.
    Decompress(String),
    /// A module with that name was not found in the library.
    ModuleNotFound(String),
    /// The operation does not apply to this kind of module
    /// (e.g. verse lookup on a dictionary).
    WrongModuleType { expected: &'static str, found: &'static str },
    /// A remote file does not exist (HTTP 404, FTP 550, missing local file).
    NotFound(String),
    /// Any other network / transport failure.
    Network { url: String, message: String },
    /// Installing or removing a module failed (bad archive, unsafe paths, …).
    Install(String),
}

impl Error {
    /// An I/O error that names the file involved.
    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Error {
        Error::Io { path: Some(path.into()), source }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path: Some(path), source } => write!(f, "{}: {source}", path.display()),
            Error::Io { path: None, source } => write!(f, "{source}"),
            Error::Conf(m) => write!(f, "conf: {m}"),
            Error::UnsupportedDriver(d) => write!(f, "unsupported module driver: {d}"),
            Error::UnsupportedCompression(c) => write!(f, "unsupported compression: {c}"),
            Error::UnknownVersification(v) => write!(f, "unknown versification: {v}"),
            Error::Locked(m) => write!(f, "module {m} is enciphered and has no key"),
            Error::Corrupt(m) => write!(f, "corrupt module data: {m}"),
            Error::Decompress(m) => write!(f, "decompression failed: {m}"),
            Error::ModuleNotFound(m) => write!(f, "module not found: {m}"),
            Error::WrongModuleType { expected, found } => write!(f, "expected a {expected} module, found a {found} module"),
            Error::NotFound(u) => write!(f, "not found: {u}"),
            Error::Network { url, message } => write!(f, "{url}: {message}"),
            Error::Install(m) => write!(f, "install: {m}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(source: io::Error) -> Self {
        Error::Io { path: None, source }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
