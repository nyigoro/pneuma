use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

pub fn discover_plugins(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut plugins = Vec::new();
    let ext = plugin_extension();

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some(ext) {
            plugins.push(path);
        }
    }

    Ok(plugins)
}

fn plugin_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    }
}
