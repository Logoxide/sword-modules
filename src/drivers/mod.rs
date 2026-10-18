//! Storage drivers: reading entries' bytes from the module files
//! (`docs/spec/formats.md`). Decoding and normalising the text is the module layer's job.

pub(crate) mod genbook;
pub(crate) mod strkey;
pub(crate) mod verse;

use std::sync::{Arc, Mutex};

use crate::conf::CompressType;
use crate::error::{Error, Result};
use crate::files::{FileReader, Files};

/// A data file read in pieces: opened once, then read at offsets.
pub(crate) struct DataFile {
    path: String,
    reader: Box<dyn FileReader>,
}

impl std::fmt::Debug for DataFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataFile").field("path", &self.path).field("size", &self.reader.size()).finish()
    }
}

impl DataFile {
    pub(crate) fn open(files: &dyn Files, path: &str) -> Result<DataFile> {
        DataFile::open_optional(files, path)?.ok_or_else(|| Error::io(path, std::io::ErrorKind::NotFound.into()))
    }

    /// The file, or `None` when it does not exist.
    pub(crate) fn open_optional(files: &dyn Files, path: &str) -> Result<Option<DataFile>> {
        Ok(files.open(path)?.map(|reader| DataFile { path: path.to_string(), reader }))
    }

    fn len(&self) -> u64 {
        self.reader.size()
    }

    /// `len` bytes at `offset`; reading past the end is corrupt data.
    pub(crate) fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        if offset.checked_add(len as u64).is_none_or(|end| end > self.len()) {
            return Err(Error::Corrupt(format!("{}: {len} bytes at {offset} is past the end ({} bytes)", self.path, self.len())));
        }
        let mut buf = vec![0u8; len];
        self.reader.read_at(offset, &mut buf).map_err(|e| Error::io(&self.path, e))?;
        Ok(buf)
    }

    /// Up to `len` bytes at `offset`, stopping at the end of the file.
    pub(crate) fn read_up_to(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let available = self.len().saturating_sub(offset);
        self.read_at(offset.min(self.len()), len.min(usize::try_from(available).unwrap_or(usize::MAX)))
    }
}

/// A whole (index) file.
pub(crate) fn read_file(files: &dyn Files, path: &str) -> Result<Vec<u8>> {
    read_optional(files, path)?.ok_or_else(|| Error::io(path, std::io::ErrorKind::NotFound.into()))
}

/// A whole file, or `None` when it does not exist.
pub(crate) fn read_optional(files: &dyn Files, path: &str) -> Result<Option<Vec<u8>>> {
    files.read(path)
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
