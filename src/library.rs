//! A library: the modules described by `mods.d/*.conf` under one or more roots.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::conf::{decode_conf_bytes, parse_conf, ModuleConf, ModuleKind};
use crate::error::{Error, Result};
use crate::files::{Files, FsFiles};
use crate::module::Module;

/// The modules found under one or more library roots.
#[derive(Debug, Clone, Default)]
pub struct SwordLibrary {
    modules: Vec<(Arc<dyn Files>, ModuleConf)>,
}

/// Every `*.conf` file in `root/mods.d`, sorted by name.
pub(crate) fn conf_files(root: &Path) -> Result<Vec<PathBuf>> {
    let dir = root.join("mods.d");
    let entries = std::fs::read_dir(&dir).map_err(|e| Error::io(&dir, e))?;
    let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("conf"))).collect();
    files.sort();
    Ok(files)
}

/// The module sections of one conf file (those with a `ModDrv`).
#[cfg(feature = "install")]
pub(crate) fn read_module_confs(path: &Path) -> Result<Vec<ModuleConf>> {
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    module_confs(&bytes)
}

/// The module sections of a conf file's bytes (those with a `ModDrv`).
fn module_confs(bytes: &[u8]) -> Result<Vec<ModuleConf>> {
    Ok(parse_conf(&decode_conf_bytes(bytes))?.into_iter().filter(|c| c.get("ModDrv").is_some()).collect())
}

impl SwordLibrary {
    /// Scan `root/mods.d/*.conf`. Sections without `ModDrv` (e.g. `[Globals]`) are ignored.
    pub fn open(root: impl AsRef<Path>) -> Result<SwordLibrary> {
        let mut lib = SwordLibrary::default();
        lib.add_root(root)?;
        Ok(lib)
    }

    /// Add another root (later roots do not replace modules already found).
    pub fn add_root(&mut self, root: impl AsRef<Path>) -> Result<()> {
        let root = root.as_ref();
        // A root without `mods.d` is an error, as it has always been
        conf_files(root)?;
        self.add_files(Arc::new(FsFiles::new(root)))
    }

    /// Register a module conf whose data lives under `root`.
    pub fn add_module(&mut self, root: impl Into<PathBuf>, conf: ModuleConf) {
        self.add_module_files(Arc::new(FsFiles::new(root)), conf);
    }

    /// Scan `mods.d/*.conf` in `files` (as [`open`](Self::open) does a folder), e.g. a
    /// module zip unpacked into [`MemoryFiles`](crate::MemoryFiles).
    pub fn open_files(files: Arc<dyn Files>) -> Result<SwordLibrary> {
        let mut lib = SwordLibrary::default();
        lib.add_files(files)?;
        Ok(lib)
    }

    /// Add another library's files (later ones do not replace modules already found).
    pub fn add_files(&mut self, files: Arc<dyn Files>) -> Result<()> {
        let mut names: Vec<String> = files.list("mods.d")?.into_iter().filter(|n| n.to_ascii_lowercase().ends_with(".conf")).collect();
        names.sort();
        for name in names {
            // One unreadable conf should not hide the rest of the library.
            let Ok(Some(bytes)) = files.read(&format!("mods.d/{name}")) else { continue };
            let Ok(confs) = module_confs(&bytes) else { continue };
            for conf in confs {
                if self.conf(conf.name()).is_none() {
                    self.modules.push((Arc::clone(&files), conf));
                }
            }
        }
        Ok(())
    }

    /// Register a module conf whose data is in `files`.
    pub fn add_module_files(&mut self, files: Arc<dyn Files>, conf: ModuleConf) {
        match self.modules.iter_mut().find(|(_, c)| c.name().eq_ignore_ascii_case(conf.name())) {
            Some(slot) => *slot = (files, conf),
            None => self.modules.push((files, conf)),
        }
    }

    /// All module confs.
    pub fn modules(&self) -> impl Iterator<Item = &ModuleConf> {
        self.modules.iter().map(|(_, c)| c)
    }

    /// Module confs of one kind.
    pub fn modules_of_kind(&self, kind: ModuleKind) -> impl Iterator<Item = &ModuleConf> {
        self.modules().filter(move |c| c.module_kind().ok() == Some(kind))
    }

    fn entry(&self, name: &str) -> Option<&(Arc<dyn Files>, ModuleConf)> {
        self.modules.iter().find(|(_, c)| c.name() == name).or_else(|| self.modules.iter().find(|(_, c)| c.name().eq_ignore_ascii_case(name)))
    }

    /// Conf by module name (case-insensitive).
    pub fn conf(&self, name: &str) -> Option<&ModuleConf> {
        self.entry(name).map(|(_, c)| c)
    }

    /// Mutable conf by name — e.g. to set a `CipherKey` before opening.
    pub fn conf_mut(&mut self, name: &str) -> Option<&mut ModuleConf> {
        let i = self.modules.iter().position(|(_, c)| c.name() == name).or_else(|| self.modules.iter().position(|(_, c)| c.name().eq_ignore_ascii_case(name)))?;
        Some(&mut self.modules[i].1)
    }

    /// Open a module by name.
    pub fn open_module(&self, name: &str) -> Result<Module> {
        let (files, conf) = self.entry(name).ok_or_else(|| Error::ModuleNotFound(name.to_string()))?;
        Module::open_files(Arc::clone(files), conf.clone())
    }

    /// Open an enciphered module with a key (without storing the key).
    pub fn open_module_with_key(&self, name: &str, cipher_key: &str) -> Result<Module> {
        let (files, conf) = self.entry(name).ok_or_else(|| Error::ModuleNotFound(name.to_string()))?;
        let mut conf = conf.clone();
        conf.set("CipherKey", cipher_key);
        Module::open_files(Arc::clone(files), conf)
    }
}
