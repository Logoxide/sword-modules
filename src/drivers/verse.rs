//! Verse-keyed storage: RawText/RawCom (and their `4` variants), zText/zCom, and
//! RawFiles personal notes.

use std::sync::Arc;

use super::{read_optional, size_at, u32_at, BlockReader, DataFile};
use crate::conf::{BlockType, CompressType};
use crate::error::{Error, Result};
use crate::files::Files;
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
    files: Arc<dyn Files>,
    dir: String,
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
    /// A testament whose data files are missing is left out, as if it had no index.
    pub(crate) fn open(files: Arc<dyn Files>, dir: &str, layout: Layout, v11n: &Versification, cipher: Option<Vec<u8>>) -> Result<VerseStore> {
        let at = |name: &str| format!("{dir}/{name}");
        let mut testaments = [None, None];
        let mut any_index = false;
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
            let Some(index) = read_optional(files.as_ref(), &at(&index_name))? else { continue };
            any_index = true;
            // An index without its data file (or block index) is a testament without
            // data: every entry reads as empty (docs/spec/formats.md).
            let Some(data) = DataFile::open_optional(files.as_ref(), &at(&data_name))? else { continue };
            let blocks = match blocks_name {
                Some(name) => match read_optional(files.as_ref(), &at(&name))? {
                    Some(blocks) => Some(blocks),
                    None => continue,
                },
                None => None,
            };
            // Some published modules declare the wrong index width; use the one that fits
            // the versification exactly when the declared one does not.
            let slots = v11n.slot_count(t) as usize;
            let (declared, other) = widths;
            let width = if index.len() != slots * declared && index.len() == slots * other { other } else { declared };
            testaments[slot_index(t)] = Some(TestamentFiles { index, width, data, blocks });
        }
        if !any_index {
            return Err(Error::Corrupt(format!("{dir}: no Old or New Testament index files")));
        }
        let reader = match &layout {
            Layout::Compressed { compress, .. } => Some(BlockReader::new(compress.clone(), cipher.clone())),
            _ => None,
        };
        Ok(VerseStore { files, dir: dir.to_string(), layout, testaments, cipher, reader })
    }

    pub(crate) fn has_testament(&self, t: Testament) -> bool {
        self.testaments[slot_index(t)].is_some()
    }

    /// The slot's index entry as `(block, offset, length)` (block 0 for uncompressed
    /// layouts), or `None` when the testament has no data, the slot is past the index,
    /// or the length is zero. Slots with equal index entries share one stored entry.
    pub(crate) fn index_entry(&self, pos: IndexPos) -> Option<(u32, u32, u32)> {
        let files = self.testaments[slot_index(pos.testament)].as_ref()?;
        let at = pos.slot as usize * files.width;
        if at + files.width > files.index.len() {
            return None;
        }
        let (block, offset, len) = match self.layout {
            Layout::Raw { .. } | Layout::Files => (0, u32_at(&files.index, at)?, size_at(&files.index, at + 4, files.width - 4)?),
            Layout::Compressed { .. } => (u32_at(&files.index, at)?, u32_at(&files.index, at + 4)?, size_at(&files.index, at + 8, files.width - 8)?),
        };
        (len > 0).then_some((block, offset, len))
    }

    /// The entry's stored bytes, or `None` when it has none.
    pub(crate) fn entry(&self, pos: IndexPos) -> Result<Option<Vec<u8>>> {
        let Some(files) = &self.testaments[slot_index(pos.testament)] else { return Ok(None) };
        let Some((block, offset, len)) = self.index_entry(pos) else { return Ok(None) };
        match &self.layout {
            Layout::Raw { .. } | Layout::Files => {
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
                let (offset, len) = (offset as usize, len as usize);
                let blocks = files.blocks.as_deref().unwrap_or(&[]);
                let b = block as usize * 12;
                let (Some(start), Some(stored)) = (u32_at(blocks, b), u32_at(blocks, b + 4)) else {
                    return Err(Error::Corrupt(format!("{}: block {block} is not in the block index", self.dir)));
                };
                let Some(reader) = &self.reader else { return Ok(None) };
                let id = (u64::from(slot_index(pos.testament) as u32) << 32) | u64::from(block);
                let unpacked = reader.block(id, || files.data.read_at(u64::from(start), stored as usize))?;
                match unpacked.get(offset..offset + len) {
                    Some(text) => Ok(Some(text.to_vec())),
                    None => Err(Error::Corrupt(format!("{}: entry at {offset}+{len} is outside block {block} ({} bytes)", self.dir, unpacked.len()))),
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
        match read_optional(self.files.as_ref(), &format!("{}/{name}", self.dir))? {
            Some(bytes) if !bytes.is_empty() => Ok(Some(bytes)),
            _ => Ok(None),
        }
    }
}
