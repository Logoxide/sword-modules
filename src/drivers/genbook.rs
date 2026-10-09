//! Tree-keyed storage: RawGenBook.

use std::path::Path;

use super::{i32_at, read_file, u16_at, u32_at, DataFile};
use crate::error::{Error, Result};

/// A node of a general book's table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    /// Stable node id (the node's byte offset in the `.idx` file; root = 0).
    pub offset: u32,
    pub parent: Option<u32>,
    pub next_sibling: Option<u32>,
    pub first_child: Option<u32>,
    /// The node's name (one path segment, e.g. `Chapter 1`).
    pub name: String,
    /// Where the node's text is in `.bdt` (offset, size), if it has any.
    text: Option<(u32, u32)>,
}

impl TreeNode {
    pub fn has_children(&self) -> bool {
        self.first_child.is_some()
    }

    /// Whether the node has entry text of its own.
    pub fn has_text(&self) -> bool {
        self.text.is_some()
    }

    pub(crate) fn text_location(&self) -> Option<(u32, u32)> {
        self.text
    }
}

/// The files of one general book.
#[derive(Debug)]
pub(crate) struct TreeStore {
    prefix: String,
    idx: Vec<u8>,
    dat: Vec<u8>,
    bdt: DataFile,
    cipher: Option<Vec<u8>>,
}

fn link(v: Option<i32>) -> Option<u32> {
    v.and_then(|v| u32::try_from(v).ok())
}

impl TreeStore {
    pub(crate) fn open(prefix: &Path, cipher: Option<Vec<u8>>) -> Result<TreeStore> {
        let with_ext = |ext: &str| {
            let mut p = prefix.as_os_str().to_owned();
            p.push(ext);
            std::path::PathBuf::from(p)
        };
        let idx = read_file(&with_ext(".idx"))?;
        let dat = read_file(&with_ext(".dat"))?;
        let bdt = DataFile::open(&with_ext(".bdt"))?;
        let store = TreeStore { prefix: prefix.display().to_string(), idx, dat, bdt, cipher };
        // The root must be readable.
        store.node(0)?;
        Ok(store)
    }

    /// The node whose `.idx` entry is at `offset`; `name` is decoded by the caller.
    pub(crate) fn node_raw(&self, offset: u32) -> Result<(TreeNode, Vec<u8>)> {
        let corrupt = |what: &str| Error::Corrupt(format!("{}: node {offset}: {what}", self.prefix));
        let at = u32_at(&self.idx, offset as usize).ok_or_else(|| corrupt("not in .idx"))? as usize;
        let parent = i32_at(&self.dat, at).ok_or_else(|| corrupt("truncated .dat"))?;
        let next = i32_at(&self.dat, at + 4).ok_or_else(|| corrupt("truncated .dat"))?;
        let child = i32_at(&self.dat, at + 8).ok_or_else(|| corrupt("truncated .dat"))?;
        let name_start = at + 12;
        let name_len = self.dat.get(name_start..).and_then(|rest| rest.iter().position(|&b| b == 0)).ok_or_else(|| corrupt("unterminated name"))?;
        let name = self.dat[name_start..name_start + name_len].to_vec();
        let user_at = name_start + name_len + 1;
        let user_len = u16_at(&self.dat, user_at).unwrap_or(0) as usize;
        let text = if user_len >= 8 {
            match (u32_at(&self.dat, user_at + 2), u32_at(&self.dat, user_at + 6)) {
                (Some(o), Some(s)) if s > 0 => Some((o, s)),
                _ => None,
            }
        } else {
            None
        };
        let node = TreeNode { offset, parent: link(Some(parent)), next_sibling: link(Some(next)), first_child: link(Some(child)), name: String::new(), text };
        Ok((node, name))
    }

    pub(crate) fn node(&self, offset: u32) -> Result<TreeNode> {
        self.node_raw(offset).map(|(n, _)| n)
    }

    /// Number of nodes the index could hold (a bound for walking the tree).
    pub(crate) fn node_count(&self) -> usize {
        self.idx.len() / 4
    }

    /// A node's text bytes.
    pub(crate) fn text(&self, node: &TreeNode) -> Result<Option<Vec<u8>>> {
        let Some((offset, size)) = node.text_location() else { return Ok(None) };
        let mut bytes = self.bdt.read_up_to(u64::from(offset), size as usize)?;
        if let Some(key) = &self.cipher {
            crate::cipher::decipher(key, &mut bytes);
        }
        Ok(Some(bytes))
    }
}
