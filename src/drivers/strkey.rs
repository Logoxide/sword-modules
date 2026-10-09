//! String-keyed storage: RawLD, RawLD4 and zLD.

use std::path::Path;

use super::{read_file, size_at, u32_at, BlockReader, DataFile};
use crate::conf::CompressType;
use crate::error::{Error, Result};

/// zLD's compressed blocks.
#[derive(Debug)]
struct Blocks {
    /// `.zdx`: (offset, size) in `.zdt` per block.
    index: Vec<u8>,
    data: DataFile,
    reader: BlockReader,
}

/// The files of one dictionary module.
#[derive(Debug)]
pub(crate) struct DictStore {
    prefix: String,
    /// `.idx`: (offset, size) in `.dat` per key, in sorted order.
    index: Vec<u8>,
    /// Bytes per `.idx` entry (6 for RawLD, 8 for RawLD4 and zLD).
    width: usize,
    dat: DataFile,
    blocks: Option<Blocks>,
    cipher: Option<Vec<u8>>,
}

/// One `.dat` record: the key and what follows its line.
pub(crate) struct Record {
    pub key: Vec<u8>,
    pub body: Vec<u8>,
}

/// The target of an `@LINK` body, if this is one.
pub(crate) fn link_target(body: &[u8]) -> Option<&[u8]> {
    let rest = body.strip_prefix(b"@LINK ")?;
    let end = rest.iter().position(|&b| b == b'\r' || b == b'\n' || b == 0).unwrap_or(rest.len());
    let target = rest[..end].trim_ascii();
    (!target.is_empty()).then_some(target)
}

impl DictStore {
    /// `compress` is `Some` for zLD.
    pub(crate) fn open(prefix: &Path, wide: bool, compress: Option<CompressType>, cipher: Option<Vec<u8>>) -> Result<DictStore> {
        let with_ext = |ext: &str| {
            let mut p = prefix.as_os_str().to_owned();
            p.push(ext);
            std::path::PathBuf::from(p)
        };
        let index = read_file(&with_ext(".idx"))?;
        let dat = DataFile::open(&with_ext(".dat"))?;
        let blocks = match compress {
            Some(compress) => Some(Blocks { index: read_file(&with_ext(".zdx"))?, data: DataFile::open(&with_ext(".zdt"))?, reader: BlockReader::new(compress, cipher.clone()) }),
            None => None,
        };
        let width = if wide || blocks.is_some() { 8 } else { 6 };
        Ok(DictStore { prefix: prefix.display().to_string(), index, width, dat, blocks, cipher })
    }

    pub(crate) fn len(&self) -> usize {
        self.index.len() / self.width
    }

    /// The `.dat` record of key `i`.
    pub(crate) fn record(&self, i: usize) -> Result<Record> {
        let at = i * self.width;
        let (Some(offset), Some(size)) = (u32_at(&self.index, at), size_at(&self.index, at + 4, self.width - 4)) else {
            return Err(Error::Corrupt(format!("{}.idx: no entry {i}", self.prefix)));
        };
        let bytes = self.dat.read_up_to(u64::from(offset), size as usize)?;
        // The key runs to the first CR, LF or backslash; the body starts after that line.
        let key_end = bytes.iter().position(|&b| matches!(b, b'\r' | b'\n' | b'\\')).unwrap_or(bytes.len());
        let body_start = bytes[key_end..].iter().position(|&b| b == b'\n').map_or(bytes.len(), |p| key_end + p + 1);
        Ok(Record { key: bytes[..key_end].to_vec(), body: bytes[body_start..].to_vec() })
    }

    /// The stored key `i`.
    pub(crate) fn key(&self, i: usize) -> Result<Vec<u8>> {
        Ok(self.record(i)?.key)
    }

    /// An entry's text from its record body: the body itself, or for zLD the entry it
    /// points to in a compressed block. `@LINK` bodies are returned as they are.
    pub(crate) fn text(&self, body: Vec<u8>) -> Result<Vec<u8>> {
        if link_target(&body).is_some() {
            return Ok(body);
        }
        let Some(blocks) = &self.blocks else {
            let mut body = body;
            if let Some(key) = &self.cipher {
                crate::cipher::decipher(key, &mut body);
            }
            return Ok(body);
        };
        let (Some(block), Some(entry)) = (u32_at(&body, 0), u32_at(&body, 4)) else {
            return Err(Error::Corrupt(format!("{}.dat: a zLD record without a block number", self.prefix)));
        };
        let b = block as usize * 8;
        let (Some(start), Some(size)) = (u32_at(&blocks.index, b), u32_at(&blocks.index, b + 4)) else {
            return Err(Error::Corrupt(format!("{}.zdx: no block {block}", self.prefix)));
        };
        let unpacked = blocks.reader.block(u64::from(block), || blocks.data.read_up_to(u64::from(start), size as usize))?;
        let count = u32_at(&unpacked, 0).unwrap_or(0);
        if entry >= count {
            return Err(Error::Corrupt(format!("{}: block {block} has {count} entries, not {}", self.prefix, entry + 1)));
        }
        let e = 4 + entry as usize * 8;
        let (Some(offset), Some(len)) = (u32_at(&unpacked, e), u32_at(&unpacked, e + 4)) else {
            return Err(Error::Corrupt(format!("{}: block {block} is truncated", self.prefix)));
        };
        let (offset, len) = (offset as usize, len as usize);
        let text = unpacked.get(offset..offset.saturating_add(len)).ok_or_else(|| Error::Corrupt(format!("{}: entry {entry} lies outside block {block}", self.prefix)))?;
        let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
        Ok(text[..end].to_vec())
    }
}
