//! Filesystem layout for the wrvm root.
//!
//! ```text
//! ~/.tegmentum/wrvm/
//!   runtimes/wamr/versions/<version>/<variant>/{bin/iwasm, manifest.json}
//!   runtimes/wamr/default
//!   shims/iwasm                   # link to the wrvm binary
//!   downloads/
//!   cache/
//!   apps.json
//!   usage.log
//!   bin/wrvm                      # installer target
//! ```
//!
//! A *variant* is `iwasm`, `iwasm-gc-eh`, `wamrc`, or `wasi-extensions` —
//! separate installables tracked side-by-side under the same version.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// The runtime family managed by wrvm.
pub const WAMR: &str = "wamr";

/// The default variant when the user doesn't specify one.
pub const DEFAULT_VARIANT: &str = "iwasm";

/// Every variant wrvm can install.
pub const VARIANTS: &[&str] = &["iwasm", "iwasm-gc-eh", "wamrc", "wasi-extensions"];

#[derive(Debug, Clone)]
pub struct Layout {
    pub root: PathBuf,
}

impl Layout {
    /// Resolve the wrvm root, honoring `WRVM_HOME` then falling back to
    /// `~/.tegmentum/wrvm`.
    pub fn discover() -> Result<Layout> {
        if let Some(v) = std::env::var_os("WRVM_HOME") {
            if !v.is_empty() {
                return Ok(Layout {
                    root: PathBuf::from(v),
                });
            }
        }
        let home = dirs::home_dir().context("could not determine home directory; set WRVM_HOME")?;
        Ok(Layout {
            root: home.join(".tegmentum").join("wrvm"),
        })
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.root.join("downloads")
    }

    pub fn apps_file(&self) -> PathBuf {
        self.root.join("apps.json")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn release_cache_file(&self, all: bool) -> PathBuf {
        let name = if all {
            "releases-all.json"
        } else {
            "releases.json"
        };
        self.cache_dir().join(name)
    }

    pub fn runtime_dir(&self, runtime: &str) -> PathBuf {
        self.root.join("runtimes").join(runtime)
    }

    pub fn versions_dir(&self, runtime: &str) -> PathBuf {
        self.runtime_dir(runtime).join("versions")
    }

    pub fn version_dir(&self, runtime: &str, version: &str) -> PathBuf {
        self.versions_dir(runtime).join(version)
    }

    /// The directory a single (runtime, version, variant) installs into.
    pub fn variant_dir(&self, runtime: &str, version: &str, variant: &str) -> PathBuf {
        self.version_dir(runtime, version).join(variant)
    }

    pub fn manifest_file(&self, runtime: &str, version: &str, variant: &str) -> PathBuf {
        self.variant_dir(runtime, version, variant)
            .join("manifest.json")
    }

    /// Plain-text file naming the persistent default spec (used by new shells).
    pub fn default_file(&self, runtime: &str) -> PathBuf {
        self.runtime_dir(runtime).join("default")
    }

    pub fn shims_dir(&self) -> PathBuf {
        self.root.join("shims")
    }

    pub fn shim_bin(&self, name: &str) -> PathBuf {
        self.shims_dir().join(name)
    }

    pub fn usage_log(&self) -> PathBuf {
        self.root.join("usage.log")
    }

    /// Ensure the base directory skeleton exists.
    pub fn ensure_base(&self) -> Result<()> {
        for dir in [self.downloads_dir(), self.versions_dir(WAMR)] {
            std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        Ok(())
    }
}
