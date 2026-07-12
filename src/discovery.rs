//! Runtime discovery: project pin → session (`WRVM_VERSION`) → default
//! → `IWASM_HOME`/`WAMR_HOME` → PATH.
//!
//! Each of pin/session/default holds a [`VersionSpec`] (e.g. `latest`, `2`,
//! `2.4.5`) — floating specs track the newest matching installed release.
//! Resolution here is **offline**: specs resolve against the installed set.
//! Pulling a newer version from the network (auto-install) is layered on top
//! at the activation boundary in [`crate::install::ensure`].

use crate::layout::{Layout, DEFAULT_VARIANT, WAMR};
use crate::spec::VersionSpec;
use crate::util::version_cmp;
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Project pin file name, searched upward from the working directory.
pub const PIN_FILE: &str = "wrvm.toml";

/// Per-shell version override.
pub const SESSION_VAR: &str = "WRVM_VERSION";

/// Per-shell variant override. Falls back to the pinned or default variant.
pub const VARIANT_VAR: &str = "WRVM_VARIANT";

#[derive(Debug, Default, Deserialize)]
struct PinFile {
    wrvm: Option<PinSection>,
}

#[derive(Debug, Default, Deserialize)]
struct PinSection {
    runtime: Option<String>,
    variant: Option<String>,
}

/// A resolved binary plus context.
#[derive(Debug)]
pub struct Resolved {
    pub binary: PathBuf,
    pub version: String,
    pub variant: String,
    pub source: String,
}

/// Find a project pin by walking up from `start`.
pub fn find_pin(start: &Path) -> Result<Option<(String, Option<String>, PathBuf)>> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join(PIN_FILE);
        if candidate.is_file() {
            let text = std::fs::read_to_string(&candidate)
                .with_context(|| format!("reading {}", candidate.display()))?;
            let parsed: PinFile = toml::from_str(&text)
                .with_context(|| format!("parsing {}", candidate.display()))?;
            if let Some(section) = parsed.wrvm {
                if let Some(runtime) = section.runtime {
                    return Ok(Some((runtime, section.variant, candidate)));
                }
            }
        }
        dir = d.parent();
    }
    Ok(None)
}

/// Versions with at least one installed variant, sorted ascending.
pub fn installed_versions(layout: &Layout) -> Result<Vec<String>> {
    let dir = layout.versions_dir(WAMR);
    let mut versions = Vec::new();
    if dir.exists() {
        for entry in
            std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if !installed_variants(layout, &name)?.is_empty() {
                versions.push(name);
            }
        }
    }
    versions.sort_by(|a, b| version_cmp(a, b));
    Ok(versions)
}

/// Variants of a given version that are actually installed (have a manifest).
pub fn installed_variants(layout: &Layout, version: &str) -> Result<Vec<String>> {
    let dir = layout.version_dir(WAMR, version);
    let mut variants = Vec::new();
    if dir.exists() {
        for entry in
            std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if layout.manifest_file(WAMR, version, &name).is_file() {
                variants.push(name);
            }
        }
    }
    variants.sort();
    Ok(variants)
}

/// Resolve a spec against the installed set (offline). Returns the concrete
/// version string, or `None` when nothing matches.
pub fn resolve_installed(layout: &Layout, spec_str: &str) -> Option<String> {
    let spec = VersionSpec::parse(spec_str).ok()?;
    let installed = installed_versions(layout).ok()?;
    spec.resolve(&installed).map(str::to_string)
}

pub fn default_version(layout: &Layout) -> Option<String> {
    let text = std::fs::read_to_string(layout.default_file(WAMR)).ok()?;
    let v = text.trim();
    (!v.is_empty()).then(|| v.to_string())
}

pub fn set_default_version(layout: &Layout, spec: &str) -> Result<()> {
    let path = layout.default_file(WAMR);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, spec).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub fn session_version() -> Option<String> {
    match std::env::var(SESSION_VAR) {
        Ok(v) if !v.trim().is_empty() => Some(v.trim().to_string()),
        _ => None,
    }
}

pub fn session_variant() -> Option<String> {
    match std::env::var(VARIANT_VAR) {
        Ok(v) if !v.trim().is_empty() => Some(v.trim().to_string()),
        _ => None,
    }
}

/// (spec, source) — session overrides default, no pin.
pub fn effective_spec(layout: &Layout) -> Option<(String, &'static str)> {
    if let Some(v) = session_version() {
        return Some((v, "session"));
    }
    default_version(layout).map(|v| (v, "default"))
}

pub fn effective_version(layout: &Layout) -> Option<(String, &'static str)> {
    let (spec_str, src) = effective_spec(layout)?;
    resolve_installed(layout, &spec_str).map(|v| (v, src))
}

fn binary_in(layout: &Layout, version: &str, variant: &str, binary_name: &str) -> PathBuf {
    layout
        .variant_dir(WAMR, version, variant)
        .join("bin")
        .join(binary_name)
}

fn describe(spec: &VersionSpec, resolved: &str, src: &str) -> String {
    if spec.is_floating() {
        format!("{src} ({spec} -> {resolved})")
    } else {
        format!("{src} ({resolved})")
    }
}

/// Resolve a runtime binary. `binary_name` selects the executable within a
/// variant (`iwasm` for the runtime, `wamrc` for the AOT compiler).
///
/// Order: project pin → session → default → env → PATH. For the pin the
/// variant is taken from `[wrvm].variant` if set. Otherwise the variant comes
/// from `WRVM_VARIANT`, else `binary_name` for `wamrc`, else `iwasm` (the
/// default installable maps to `bin/iwasm`).
pub fn resolve(layout: &Layout, cwd: &Path, binary_name: &str) -> Result<Resolved> {
    let installed = installed_versions(layout)?;

    // Project pin — a pin naming an unsatisfiable spec is a hard error.
    if let Some((spec_str, pin_variant, file)) = find_pin(cwd)? {
        let spec =
            VersionSpec::parse(&spec_str).map_err(|e| anyhow!("{e} (in {})", file.display()))?;
        match spec.resolve(&installed) {
            Some(version) => {
                let variant = choose_variant(binary_name, pin_variant.as_deref())?;
                let bin = binary_in(layout, version, &variant, binary_name);
                if bin.exists() {
                    return Ok(Resolved {
                        binary: bin,
                        version: version.to_string(),
                        variant,
                        source: describe(
                            &spec,
                            version,
                            &format!("project pin ({})", file.display()),
                        ),
                    });
                }
                bail!(
                    "project pins WAMR '{spec}' (variant {variant}, from {}) but that variant is not installed; \
                     run `wrvm install {spec} --variant {variant}`",
                    file.display()
                );
            }
            None => bail!(
                "project pins WAMR '{spec}' (from {}) but no matching version is installed; \
                 run `wrvm install {spec}`",
                file.display()
            ),
        }
    }

    // Session, then default.
    for (spec_str, src) in [
        session_version().map(|v| (v, "session")),
        default_version(layout).map(|v| (v, "default")),
    ]
    .into_iter()
    .flatten()
    {
        let Ok(spec) = VersionSpec::parse(&spec_str) else {
            continue;
        };
        if let Some(version) = spec.resolve(&installed) {
            let variant = choose_variant(binary_name, session_variant().as_deref())?;
            let bin = binary_in(layout, version, &variant, binary_name);
            if bin.exists() {
                return Ok(Resolved {
                    binary: bin,
                    version: version.to_string(),
                    variant,
                    source: describe(&spec, version, src),
                });
            }
        }
    }

    // Environment path override.
    for var in ["IWASM_HOME", "WAMR_HOME"] {
        if let Some(val) = std::env::var_os(var) {
            if val.is_empty() {
                continue;
            }
            let p = PathBuf::from(val);
            for candidate in [p.join("bin").join(binary_name), p.join(binary_name)] {
                if candidate.is_file() {
                    return Ok(Resolved {
                        binary: candidate,
                        version: "external".to_string(),
                        variant: "external".to_string(),
                        source: format!("${var}"),
                    });
                }
            }
        }
    }

    // PATH lookup.
    if let Some(bin) = which(binary_name) {
        return Ok(Resolved {
            binary: bin,
            version: "external".to_string(),
            variant: "external".to_string(),
            source: "PATH".to_string(),
        });
    }

    bail!("no {binary_name} runtime found; try `wrvm install latest` then `wrvm default latest`")
}

/// Pick the variant for a given binary. `wamrc` is its own variant; everything
/// else honors the explicit hint or falls back to the default (`iwasm`).
fn choose_variant(binary_name: &str, hint: Option<&str>) -> Result<String> {
    if binary_name == "wamrc" {
        return Ok("wamrc".to_string());
    }
    Ok(hint
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_VARIANT)
        .to_string())
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn layout_in(dir: &tempfile::TempDir) -> Layout {
        Layout {
            root: dir.path().to_path_buf(),
        }
    }

    fn touch_manifest(layout: &Layout, version: &str, variant: &str) {
        let mfile = layout.manifest_file(WAMR, version, variant);
        std::fs::create_dir_all(mfile.parent().unwrap()).unwrap();
        std::fs::write(&mfile, "{}").unwrap();
    }

    #[test]
    fn installed_versions_empty_when_none_present() {
        let tmp = tempdir().unwrap();
        let layout = layout_in(&tmp);
        assert!(installed_versions(&layout).unwrap().is_empty());
    }

    #[test]
    fn installed_versions_sees_variants() {
        let tmp = tempdir().unwrap();
        let layout = layout_in(&tmp);
        touch_manifest(&layout, "2.4.5", "iwasm");
        touch_manifest(&layout, "2.4.4", "iwasm");
        // Directory without a manifest doesn't count.
        std::fs::create_dir_all(layout.version_dir(WAMR, "1.0.0").join("iwasm")).unwrap();
        let versions = installed_versions(&layout).unwrap();
        // Sorted ascending.
        assert_eq!(versions, vec!["2.4.4".to_string(), "2.4.5".to_string()]);
    }

    #[test]
    fn installed_variants_lists_all() {
        let tmp = tempdir().unwrap();
        let layout = layout_in(&tmp);
        touch_manifest(&layout, "2.4.5", "iwasm");
        touch_manifest(&layout, "2.4.5", "iwasm-gc-eh");
        touch_manifest(&layout, "2.4.5", "wamrc");
        let variants = installed_variants(&layout, "2.4.5").unwrap();
        assert_eq!(
            variants,
            vec![
                "iwasm".to_string(),
                "iwasm-gc-eh".to_string(),
                "wamrc".to_string(),
            ]
        );
    }

    #[test]
    fn find_pin_walks_upward() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(
            root.join("wrvm.toml"),
            "[wrvm]\nruntime = \"2.4.5\"\nvariant = \"iwasm-gc-eh\"\n",
        )
        .unwrap();
        let nested = root.join("a").join("b").join("c");
        std::fs::create_dir_all(&nested).unwrap();
        let (spec, variant, file) = find_pin(&nested).unwrap().unwrap();
        assert_eq!(spec, "2.4.5");
        assert_eq!(variant.as_deref(), Some("iwasm-gc-eh"));
        assert_eq!(file, root.join("wrvm.toml"));
    }

    #[test]
    fn find_pin_returns_none_when_absent() {
        let tmp = tempdir().unwrap();
        assert!(find_pin(tmp.path()).unwrap().is_none());
    }

    #[test]
    fn default_version_round_trips() {
        let tmp = tempdir().unwrap();
        let layout = layout_in(&tmp);
        assert!(default_version(&layout).is_none());
        set_default_version(&layout, "2").unwrap();
        assert_eq!(default_version(&layout).as_deref(), Some("2"));
    }

    #[test]
    fn resolve_installed_matches_floating_spec() {
        let tmp = tempdir().unwrap();
        let layout = layout_in(&tmp);
        touch_manifest(&layout, "2.4.5", "iwasm");
        touch_manifest(&layout, "2.4.4", "iwasm");
        touch_manifest(&layout, "2.3.0", "iwasm");
        assert_eq!(resolve_installed(&layout, "2").as_deref(), Some("2.4.5"));
        assert_eq!(resolve_installed(&layout, "2.4").as_deref(), Some("2.4.5"));
        assert_eq!(
            resolve_installed(&layout, "2.3.0").as_deref(),
            Some("2.3.0")
        );
        assert!(resolve_installed(&layout, "3").is_none());
    }
}
