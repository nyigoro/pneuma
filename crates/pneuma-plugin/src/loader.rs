use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Debug, Default)]
pub struct PluginLoader;

impl PluginLoader {
    pub fn discover<P: AsRef<Path>>(root: P) -> Result<Vec<PathBuf>> {
        crate::discovery::discover_plugins(root.as_ref())
    }

    pub fn load_all<P: AsRef<Path>>(root: P) -> Result<usize> {
        let candidates = Self::discover(root)?;
        Ok(candidates.len())
    }
}
