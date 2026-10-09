//! Verse-keyed storage: RawText/RawCom (and their `4` variants), zText/zCom, and
//! RawFiles personal notes.

use std::path::{Path, PathBuf};

use super::{read_optional, size_at, u32_at, BlockReader, DataFile};
use crate::conf::{BlockType, CompressType};
use crate::error::{Error, Result};
use crate::versification::{IndexPos, Testament, Versification};

/// How a module stores its entries.
#[derive(Debug, Clone)]
pub(crate) enum Layout {
    /// `ot.vss` → `ot`, with a 2- or 4-byte length.
    Raw { wide: bool },
    /// `ot.Xzv` → `ot.Xzs` → `ot.Xzz`.
    Compressed { wide: bool, block: BlockType, compress: CompressType },
    /// RawText layout whose entries name files holding each note.
    Files,
}

#[derive(Debug)]
struct TestamentFiles {
    index: Vec<u8>,
    /// Bytes per index entry.
    width: usize,
    data: DataFile,
    /// `.Xzs` for compressed modules: (offset, stored size, unpacked size) per block.
    blocks: Option<Vec<u8>>,
}

/// The files of one verse-keyed module.
#[derive(Debug)]
pub(crate) struct VerseStore {
    dir: PathBuf,
    layout: Layout,
    testaments: [Option<TestamentFiles>; 2],
    cipher: Option<Vec<u8>>,
    reader: Option<BlockReader>,
}

fn slot_index(t: Testament) -> usize {
    match t {
        Testament::Old => 0,
        Testament::New => 1,
    }
}

impl VerseStore {
    /// Open the index files of both testaments; at least one must be present.
    pub(crate) fn open(dir: &Path, layout: Layout, v11n: &Versification, cipher: Option<Vec<u8>>) -> Result<VerseStore> {
        let mut testaments = [None, None];
        for t in [Testament::Old, Testament::New] {
            let prefix = t.file_prefix();
            let (index_name, data_name, blocks_name, widths) = match &layout {
                Layout::Raw { wide } => (format!("{prefix}.vss"), prefix.to_string(), None, if *wide { (8, 6) } else { (6, 8) }),
                Layout::Files => (format!("{prefix}.vss"), prefix.to_string(), None, (6, 8)),
                Layout::Compressed { wide, block, .. } => {
                    let x = block.file_letter();
                    (format!("{prefix}.{x}zv"), format!("{prefix}.{x}zz"), Some(format!("{prefix}.{x}zs")), if *wide { (12, 10) } else { (10, 12) })
                }
            };
            let Some(index) = read_optional(&dir.join(&index_name))? else { continue };
            let data_path = dir.join(&data_name);
            let data = DataFile::open(&data_path)?;
            let blocks = match blocks_name {
                Some(name) => Some(read_optional(&dir.join(&name))?.ok_or_else(|| Error::Corrupt(format!("{}: missing", dir.join(&name).display())))?),
                None => None,
            };
            // Some published modules declare the wrong index width; use the one that fits
            // the versification exactly when the declared one does not.
            let slots = v11n.slot_count(t) as usize;
            let (declared, other) = widths;
            let width = if index.len() != slots * declared && index.len() == slots * other { other } else { declared };
            testaments[slot_index(t)] = Some(TestamentFiles { index, width, data, blocks });
        }
        if testaments.iter().all(Option::is_none) {
            return Err(Error::Corrupt(format!("{}: no Old or New Testament index files", dir.display())));
        }
        let reader = match &layout {
            Layout::Compressed { compress, .. } => Some(BlockReader::new(compress.clone(), cipher.clone())),
            _ => None,
        };
        Ok(VerseStore { dir: dir.to_path_buf(), layout, testaments, cipher, reader })
    }

    pub(crate) fn has_testament(&self, t: Testament) -> bool {
        self.testaments[slot_index(t)].is_some()
    }

    /// The index entry's stored length (0 when empty or out of range).
    pub(crate) fn stored_len(&self, pos: IndexPos) -> u32 {
        let Some(files) = &self.testaments[slot_index(pos.testament)] else { return 0 };
        let at = pos.slot as usize * files.width;
        let len_at = match self.layout {
            Layout::Compressed { .. } => at + 8,
            _ => at + 4,
        };
        let len_width = files.width - (len_at - at);
        size_at(&files.index, len_at, len_width).unwrap_or(0)
    }

    /// The entry's stored bytes, or `None` when it has none.
    pub(crate) fn entry(&self, pos: IndexPos) -> Result<Option<Vec<u8>>> {
        let Some(files) = &self.testaments[slot_index(pos.testament)] else { return Ok(None) };
        let at = pos.slot as usize * files.width;
        if at + files.width > files.index.len() {
            return Ok(None);
        }
        match &self.layout {
            Layout::Raw { .. } | Layout::Files => {
                let offset = u32_at(&files.index, at).unwrap_or(0);
                let len = size_at(&files.index, at + 4, files.width - 4).unwrap_or(0);
                if len == 0 {
                    return Ok(None);
                }
                let mut bytes = files.data.read_at(u64::from(offset), len as usize)?;
                if let Layout::Files = self.layout {
                    return self.note_file(&bytes);
                }
                if let Some(key) = &self.cipher {
                    crate::cipher::decipher(key, &mut bytes);
                }
                Ok(Some(bytes))
            }
            Layout::Compressed { .. } => {
                let block = u32_at(&files.index, at).unwrap_or(0);
                let offset = u32_at(&files.index, at + 4).unwrap_or(0) as usize;
                let len = size_at(&files.index, at + 8, files.width - 8).unwrap_or(0) as usize;
                if len == 0 {
                    return Ok(None);
                }
                let blocks = files.blocks.as_deref().unwrap_or(&[]);
                let b = block as usize * 12;
                let (Some(start), Some(stored)) = (u32_at(blocks, b), u32_at(blocks, b + 4)) else {
                    return Err(Error::Corrupt(format!("{}: block {block} is not in the block index", self.dir.display())));
                };
                let Some(reader) = &self.reader else { return Ok(None) };
                let id = (u64::from(slot_index(pos.testament) as u32) << 32) | u64::from(block);
                let unpacked = reader.block(id, || files.data.read_at(u64::from(start), stored as usize))?;
                match unpacked.get(offset..offset + len) {
                    Some(text) => Ok(Some(text.to_vec())),
                    None => Err(Error::Corrupt(format!("{}: entry at {offset}+{len} is outside block {block} ({} bytes)", self.dir.display(), unpacked.len()))),
                }
            }
        }
    }

    /// A personal note: the index entry names a file in the module directory.
    fn note_file(&self, name: &[u8]) -> Result<Option<Vec<u8>>> {
        let name = String::from_utf8_lossy(name);
        let name = name.trim_matches(|c: char| c.is_whitespace() || c == '\0');
        // Only plain file names; anything that could point elsewhere is ignored.
        if name.is_empty() || name.starts_with('.') || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.') {
            return Ok(None);
        }
        match read_optional(&self.dir.join(name))? {
            Some(bytes) if !bytes.is_empty() => Ok(Some(bytes)),
            _ => Ok(None),
        }
    }
}
