//! Storage drivers: reading entries' bytes from the module files
//! (`docs/spec/formats.md`). Decoding and normalising the text is the module layer's job.

pub(crate) mod genbook;
pub(crate) mod strkey;
pub(crate) mod verse;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::conf::CompressType;
use crate::error::{Error, Result};

/// A data file read in pieces: opened once, then read at offsets.
#[derive(Debug)]
pub(crate) struct DataFile {
    path: PathBuf,
    len: u64,
    file: Mutex<File>,
}

impl DataFile {
    pub(crate) fn open(path: &Path) -> Result<DataFile> {
        let file = File::open(path).map_err(|e| Error::io(path, e))?;
        let len = file.metadata().map_err(|e| Error::io(path, e))?.len();
        Ok(DataFile { path: path.to_path_buf(), len, file: Mutex::new(file) })
    }

    /// `len` bytes at `offset`; reading past the end is corrupt data.
    pub(crate) fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        if offset.checked_add(len as u64).is_none_or(|end| end > self.len) {
            return Err(Error::Corrupt(format!("{}: {len} bytes at {offset} is past the end ({} bytes)", self.path.display(), self.len)));
        }
        let mut buf = vec![0u8; len];
        let mut file = self.file.lock().map_err(|_| Error::Corrupt(format!("{}: reader poisoned", self.path.display())))?;
        file.seek(SeekFrom::Start(offset)).map_err(|e| Error::io(&self.path, e))?;
        file.read_exact(&mut buf).map_err(|e| Error::io(&self.path, e))?;
        Ok(buf)
    }

    /// Up to `len` bytes at `offset`, stopping at the end of the file.
    pub(crate) fn read_up_to(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let available = self.len.saturating_sub(offset);
        self.read_at(offset.min(self.len), len.min(usize::try_from(available).unwrap_or(usize::MAX)))
    }
}

/// A whole (index) file.
pub(crate) fn read_file(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| Error::io(path, e))
}

/// A whole file, or `None` when it does not exist.
pub(crate) fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(path, e)),
    }
}

pub(crate) fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

pub(crate) fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

pub(crate) fn i32_at(b: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// A length field of 2 or 4 bytes.
pub(crate) fn size_at(b: &[u8], at: usize, width: usize) -> Option<u32> {
    if width == 2 {
        u16_at(b, at).map(u32::from)
    } else {
        u32_at(b, at)
    }
}

/// How compressed blocks are unpacked, with the last block kept for the next read
/// (neighbouring entries usually share it).
#[derive(Debug)]
pub(crate) struct BlockReader {
    compress: CompressType,
    cipher: Option<Vec<u8>>,
    cache: Mutex<Option<(u64, Arc<Vec<u8>>)>>,
}

impl BlockReader {
    pub(crate) fn new(compress: CompressType, cipher: Option<Vec<u8>>) -> BlockReader {
        BlockReader { compress, cipher, cache: Mutex::new(None) }
    }

    /// The unpacked block `id` (any number unique to the block), read by `fetch` when not cached.
    pub(crate) fn block(&self, id: u64, fetch: impl FnOnce() -> Result<Vec<u8>>) -> Result<Arc<Vec<u8>>> {
        if let Ok(cache) = self.cache.lock() {
            if let Some((cached, block)) = cache.as_ref() {
                if *cached == id {
                    return Ok(Arc::clone(block));
                }
            }
        }
        let mut stored = fetch()?;
        if let Some(key) = &self.cipher {
            crate::cipher::decipher(key, &mut stored);
        }
        let block = Arc::new(crate::compress::decompress(&self.compress, &stored)?);
        if let Ok(mut cache) = self.cache.lock() {
            *cache = Some((id, Arc::clone(&block)));
        }
        Ok(block)
    }
}
