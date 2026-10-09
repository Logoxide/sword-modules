//! A library: the modules described by `mods.d/*.conf` under one or more roots.

use std::path::{Path, PathBuf};

use crate::conf::{decode_conf_bytes, parse_conf, ModuleConf, ModuleKind};
use crate::error::{Error, Result};
use crate::module::Module;

/// The modules found under one or more library roots.
#[derive(Debug, Clone, Default)]
pub struct SwordLibrary {
    modules: Vec<(PathBuf, ModuleConf)>,
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
pub(crate) fn read_module_confs(path: &Path) -> Result<Vec<ModuleConf>> {
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    Ok(parse_conf(&decode_conf_bytes(&bytes))?.into_iter().filter(|c| c.get("ModDrv").is_some()).collect())
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
        for file in conf_files(root)? {
            // One unreadable conf should not hide the rest of the library.
            let Ok(confs) = read_module_confs(&file) else { continue };
            for conf in confs {
                if self.conf(conf.name()).is_none() {
                    self.modules.push((root.to_path_buf(), conf));
                }
            }
        }
        Ok(())
    }

    /// Register a module conf whose data lives under `root`.
    pub fn add_module(&mut self, root: impl Into<PathBuf>, conf: ModuleConf) {
        let root = root.into();
        match self.modules.iter_mut().find(|(_, c)| c.name().eq_ignore_ascii_case(conf.name())) {
            Some(slot) => *slot = (root, conf),
            None => self.modules.push((root, conf)),
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

    fn entry(&self, name: &str) -> Option<&(PathBuf, ModuleConf)> {
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
        let (root, conf) = self.entry(name).ok_or_else(|| Error::ModuleNotFound(name.to_string()))?;
        Module::open(root, conf.clone())
    }

    /// Open an enciphered module with a key (without storing the key).
    pub fn open_module_with_key(&self, name: &str, cipher_key: &str) -> Result<Module> {
        let (root, conf) = self.entry(name).ok_or_else(|| Error::ModuleNotFound(name.to_string()))?;
        let mut conf = conf.clone();
        conf.set("CipherKey", cipher_key);
        Module::open(root, conf)
    }
}
