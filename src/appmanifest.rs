//! Application manifest — the `[app]` section of an app's `wrvm.toml`.
//!
//! ```toml
//! [app]
//! name = "tegmentum-foo"
//! runtimes = ["2.4.5"]           # WAMR versions tested against
//! variant = "iwasm-gc-eh"         # optional; defaults to "iwasm"
//! # runtime-path = "/opt/foo/bin/iwasm"   # or bring your own
//! ```

use crate::util::normalize_version;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::Path;

/// Shared with the project-pin format.
pub const MANIFEST_FILE: &str = "wrvm.toml";

#[derive(Debug, Clone)]
pub struct AppManifest {
    pub name: String,
    pub runtimes: Vec<String>,
    pub variant: Option<String>,
    pub runtime_path: Option<String>,
}

#[derive(Deserialize)]
struct RawFile {
    app: Option<RawApp>,
}

#[derive(Deserialize)]
struct RawApp {
    name: String,
    #[serde(default)]
    runtimes: Vec<String>,
    #[serde(default)]
    variant: Option<String>,
    #[serde(default, rename = "runtime-path")]
    runtime_path: Option<String>,
}

impl AppManifest {
    pub fn read_dir(dir: &Path) -> Result<AppManifest> {
        let path = dir.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<AppManifest> {
        let raw: RawFile = toml::from_str(text)?;
        let app = raw
            .app
            .context("no [app] section (an application manifest needs `[app]`)")?;
        if app.name.trim().is_empty() {
            bail!("[app] name must not be empty");
        }
        if app.runtimes.is_empty() && app.runtime_path.is_none() {
            bail!("[app] must list `runtimes` or set `runtime-path`");
        }
        Ok(AppManifest {
            name: app.name,
            runtimes: app.runtimes.iter().map(|v| normalize_version(v)).collect(),
            variant: app.variant,
            runtime_path: app.runtime_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_with_runtimes() {
        let m = AppManifest::parse(
            r#"
            [app]
            name = "foo"
            runtimes = ["2.4.5", "v2.4.4", "WAMR-2.3.0"]
            "#,
        )
        .unwrap();
        assert_eq!(m.name, "foo");
        // normalize_version strips both `v` and `WAMR-`.
        assert_eq!(
            m.runtimes,
            vec![
                "2.4.5".to_string(),
                "2.4.4".to_string(),
                "2.3.0".to_string(),
            ]
        );
        assert!(m.variant.is_none());
    }

    #[test]
    fn parse_with_variant() {
        let m = AppManifest::parse(
            r#"
            [app]
            name = "foo"
            runtimes = ["2.4.5"]
            variant = "iwasm-gc-eh"
            "#,
        )
        .unwrap();
        assert_eq!(m.variant.as_deref(), Some("iwasm-gc-eh"));
    }

    #[test]
    fn parse_with_runtime_path() {
        let m = AppManifest::parse(
            r#"
            [app]
            name = "foo"
            runtime-path = "/opt/foo/bin/iwasm"
            "#,
        )
        .unwrap();
        assert_eq!(m.runtime_path.as_deref(), Some("/opt/foo/bin/iwasm"));
        assert!(m.runtimes.is_empty());
    }

    #[test]
    fn parse_rejects_no_runtimes_or_path() {
        let e = AppManifest::parse(
            r#"
            [app]
            name = "foo"
            "#,
        );
        assert!(e.is_err());
    }

    #[test]
    fn parse_rejects_empty_name() {
        let e = AppManifest::parse(
            r#"
            [app]
            name = "  "
            runtimes = ["2.4.5"]
            "#,
        );
        assert!(e.is_err());
    }

    #[test]
    fn parse_rejects_no_app_section() {
        let e = AppManifest::parse("[wrvm]\nruntime = \"2.4.5\"\n");
        assert!(e.is_err());
    }
}
