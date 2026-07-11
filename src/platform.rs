//! Host platform detection and WAMR release asset naming.
//!
//! WAMR asset filenames aren't as regular as Wasmtime's — the OS tokens embed
//! CI runner labels (e.g. `ubuntu-22.04`, `macos-15-intel`, `windows-2022`) and
//! the exact runner may change over releases. So instead of computing exact
//! asset names we produce a **prefix + host pattern** that lets the install
//! flow pick the right asset by matching against the release's listed assets.
//!
//! On aarch64, upstream ships no assets — install.rs falls back to this repo's
//! `wamr-mirror-<ver>` releases, which use a predictable `aarch64-<runner>`
//! token; `asset_os_patterns` covers both.

use anyhow::{bail, Result};

#[derive(Debug, Clone)]
pub struct Platform {
    /// e.g. `x86_64` or `aarch64`.
    pub arch: &'static str,
    /// Short OS token used for cross-cutting labels: `linux`, `macos`, `windows`.
    pub os: &'static str,
    /// Substrings the asset OS-and-runner token can contain. First match wins.
    pub asset_os_patterns: &'static [&'static str],
    /// Archive extension for this OS.
    pub ext: &'static str,
    /// True when we need to consult the in-repo mirror (aarch64) rather than
    /// only the upstream release.
    pub needs_mirror: bool,
}

impl Platform {
    pub fn detect() -> Result<Platform> {
        let (arch, needs_mirror) = match std::env::consts::ARCH {
            "x86_64" => ("x86_64", false),
            "aarch64" | "arm64" => ("aarch64", true),
            other => bail!("unsupported CPU architecture: {other}"),
        };
        let (os, asset_os_patterns, ext): (_, &[&str], _) = match std::env::consts::OS {
            "linux" => ("linux", &["ubuntu-", "linux"], "tar.gz"),
            "macos" => ("macos", &["macos-", "darwin"], "tar.gz"),
            "windows" => ("windows", &["windows-"], "zip"),
            other => bail!("unsupported operating system: {other}"),
        };
        Ok(Platform {
            arch,
            os,
            asset_os_patterns,
            ext,
            needs_mirror,
        })
    }

    /// Manifest platform label, e.g. `macos-x86_64`.
    pub fn label(&self) -> String {
        format!("{}-{}", self.os, self.arch)
    }

    /// Assets of the form `<variant>-<version>-<arch>-<os-runner>.<ext>`.
    /// Returns `true` when `asset_name` matches this host for the given
    /// variant+version. `<os-runner>` is opaque (varies with CI runner
    /// version), so we probe by prefix + host pattern rather than compute it.
    pub fn matches_asset(&self, asset_name: &str, variant: &str, version: &str) -> bool {
        let prefix = format!("{variant}-{version}-{}-", self.arch);
        let Some(rest) = asset_name.strip_prefix(&prefix) else {
            return false;
        };
        let Some(rest) = rest.strip_suffix(&format!(".{}", self.ext)) else {
            return false;
        };
        self.asset_os_patterns
            .iter()
            .any(|p| rest.contains(p) || rest == p.trim_end_matches('-'))
    }
}
