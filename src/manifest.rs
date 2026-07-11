//! Per-installed-variant `manifest.json` — file list with digests, for verify.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub sha256: String,
    /// Octal mode string, e.g. `0755`.
    pub mode: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub runtime: String,
    pub version: String,
    pub variant: String,
    pub platform: String,
    pub archive_sha256: String,
    pub files: Vec<FileEntry>,
}

impl Manifest {
    pub fn write(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
            .with_context(|| format!("writing manifest {}", path.display()))?;
        Ok(())
    }

    pub fn read(path: &Path) -> Result<Manifest> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading manifest {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing manifest {}", path.display()))
    }
}

pub fn mode_string(mode: u32) -> String {
    format!("{:04o}", mode & 0o7777)
}
