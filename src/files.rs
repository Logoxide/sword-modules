//! Where a library's files are read from: a folder on disk ([`FsFiles`]), or anything else
//! that can hand over a file's bytes (memory, a host's files in WebAssembly), so the readers
//! never touch the file system themselves. Paths are `/`-separated and relative to the
//! library root (`mods.d/kjv.conf`, `modules/texts/ztext/kjv/ot.bzs`).

use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::error::{Error, Result};

/// A library's files.
pub trait Files: Send + Sync {
    /// A whole file, or `None` when it does not exist.
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>>;

    /// The names of the files (not folders) directly in a folder, in any order; empty when
    /// the folder does not exist.
    fn list(&self, dir: &str) -> Result<Vec<String>>;

    /// A file to read in pieces (data files can be large), or `None` when it does not exist.
    /// The default reads the whole file with [`read`](Files::read).
    fn open(&self, path: &str) -> Result<Option<Box<dyn FileReader>>> {
        Ok(self.read(path)?.map(|bytes| Box::new(bytes) as Box<dyn FileReader>))
    }
}

impl fmt::Debug for dyn Files {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Files")
    }
}

/// A file opened for reading at offsets.
pub trait FileReader: Send + Sync {
    /// Its size in bytes.
    fn size(&self) -> u64;

    /// Fill `buf` from `offset`; an error when the file is shorter.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()>;
}

/// A file's bytes in memory.
impl FileReader for Vec<u8> {
    fn size(&self) -> u64 {
        self.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let start = usize::try_from(offset).map_err(|_| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        let bytes = start.checked_add(buf.len()).and_then(|end| self.get(start..end)).ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        buf.copy_from_slice(bytes);
        Ok(())
    }
}

/// A folder on disk: the library root.
#[derive(Debug, Clone)]
pub struct FsFiles {
    root: PathBuf,
}

impl FsFiles {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        FsFiles { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, path: &str) -> PathBuf {
        path.split('/').filter(|part| !part.is_empty()).fold(self.root.clone(), |dir, part| dir.join(part))
    }
}

/// A file on disk, opened once and read at offsets.
struct OpenFile {
    path: PathBuf,
    len: u64,
    file: Mutex<File>,
}

impl FileReader for OpenFile {
    fn size(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let mut file = self.file.lock().map_err(|_| io::Error::other(format!("{}: reader poisoned", self.path.display())))?;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(buf)
    }
}

/// Reads files from disk, opening data files once and reading them at offsets.
impl Files for FsFiles {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let path = self.path(path);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::io(path, e)),
        }
    }

    fn list(&self, dir: &str) -> Result<Vec<String>> {
        let dir = self.path(dir);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(Error::io(dir, e)),
        };
        Ok(entries.flatten().filter(|e| e.path().is_file()).filter_map(|e| e.file_name().to_str().map(str::to_string)).collect())
    }

    fn open(&self, path: &str) -> Result<Option<Box<dyn FileReader>>> {
        let path = self.path(path);
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::io(path, e)),
        };
        let len = file.metadata().map_err(|e| Error::io(&path, e))?.len();
        Ok(Some(Box::new(OpenFile { path, len, file: Mutex::new(file) })))
    }
}

/// Files kept in memory, by path (an unpacked module zip, a test).
#[derive(Debug, Clone, Default)]
pub struct MemoryFiles {
    files: BTreeMap<String, Vec<u8>>,
}

impl MemoryFiles {
    pub fn new() -> Self {
        MemoryFiles::default()
    }

    /// Add (or replace) a file.
    pub fn insert(&mut self, path: impl Into<String>, bytes: Vec<u8>) {
        self.files.insert(normalize(&path.into()), bytes);
    }
}

/// A path without empty or `.` parts: `./modules//x/` is `modules/x`.
fn normalize(path: &str) -> String {
    path.split('/').filter(|part| !part.is_empty() && *part != ".").collect::<Vec<_>>().join("/")
}

impl Files for MemoryFiles {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.files.get(&normalize(path)).cloned())
    }

    fn list(&self, dir: &str) -> Result<Vec<String>> {
        let dir = normalize(dir);
        let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
        Ok(self.files.keys().filter_map(|path| path.strip_prefix(&prefix)).filter(|rest| !rest.contains('/')).map(str::to_string).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_files_list_a_folders_files() {
        let mut files = MemoryFiles::new();
        files.insert("mods.d/kjv.conf", b"[KJV]".to_vec());
        files.insert("./modules/texts/ztext/kjv/ot.bzs", vec![1, 2, 3]);
        assert_eq!(files.list("mods.d").unwrap(), ["kjv.conf"]);
        assert_eq!(files.list("modules").unwrap(), Vec::<String>::new());
        assert_eq!(files.read("modules/texts/ztext/kjv/ot.bzs").unwrap(), Some(vec![1, 2, 3]));
        assert_eq!(files.read("mods.d/none.conf").unwrap(), None);
        let reader = files.open("modules/texts/ztext/kjv/ot.bzs").unwrap().unwrap();
        let mut buf = [0; 2];
        reader.read_at(1, &mut buf).unwrap();
        assert_eq!((reader.size(), buf), (3, [2, 3]));
        assert!(reader.read_at(2, &mut buf).is_err());
    }
}
