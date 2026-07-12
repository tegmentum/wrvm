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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_manifest() -> Manifest {
        Manifest {
            runtime: "wamr".to_string(),
            version: "2.4.5".to_string(),
            variant: "iwasm-gc-eh".to_string(),
            platform: "macos-aarch64".to_string(),
            archive_sha256: "deadbeef".to_string(),
            files: vec![
                FileEntry {
                    path: "bin/iwasm".to_string(),
                    sha256: "abc123".to_string(),
                    mode: "0755".to_string(),
                    size: 42,
                },
                FileEntry {
                    path: "LICENSE".to_string(),
                    sha256: "def456".to_string(),
                    mode: "0644".to_string(),
                    size: 10,
                },
            ],
        }
    }

    #[test]
    fn manifest_round_trip() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("manifest.json");
        let original = sample_manifest();
        original.write(&path).unwrap();
        let round = Manifest::read(&path).unwrap();
        assert_eq!(round.runtime, original.runtime);
        assert_eq!(round.version, original.version);
        assert_eq!(round.variant, original.variant);
        assert_eq!(round.platform, original.platform);
        assert_eq!(round.archive_sha256, original.archive_sha256);
        assert_eq!(round.files.len(), original.files.len());
        for (a, b) in round.files.iter().zip(original.files.iter()) {
            assert_eq!(a.path, b.path);
            assert_eq!(a.sha256, b.sha256);
            assert_eq!(a.mode, b.mode);
            assert_eq!(a.size, b.size);
        }
    }

    #[test]
    fn mode_string_formats() {
        assert_eq!(mode_string(0o755), "0755");
        assert_eq!(mode_string(0o644), "0644");
        assert_eq!(mode_string(0o600), "0600");
        // Masks the high bits (e.g. setuid) — only lower 12 bits survive.
        assert_eq!(mode_string(0o100_755), "0755");
    }
}
