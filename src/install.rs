//! Release resolution + install + `ensure`.

use crate::layout::{Layout, DEFAULT_VARIANT, VARIANTS, WAMR};
use crate::manifest::{mode_string, FileEntry, Manifest};
use crate::platform::Platform;
use crate::progress::Spinner;
use crate::spec::VersionSpec;
use crate::util::{normalize_version, version_cmp};
use crate::{archive, cache, discovery, hash, http};
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::cmp::Ordering;
use std::path::Path;

pub const REPO: &str = "bytecodealliance/wasm-micro-runtime";
/// Default in-repo aarch64 mirror. Overridable with `WRVM_RUNTIME_MIRROR`.
pub const DEFAULT_MIRROR: &str = "tegmentum/wrvm";

fn mirror_repo() -> String {
    std::env::var("WRVM_RUNTIME_MIRROR").unwrap_or_else(|_| DEFAULT_MIRROR.to_string())
}

/// Expected SHA-256 of an asset. Prefers the release JSON's `digest` field
/// (upstream WAMR sometimes fills it), then falls back to a sibling
/// `<name>.sha256` asset in the same release (the mirror workflow always
/// publishes these). `Ok(None)` when neither is available; the caller may
/// still install, with a warning.
fn expected_sha256(asset: &Asset, siblings: &[Asset]) -> Result<Option<String>> {
    if let Some(d) = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
    {
        return Ok(Some(d.to_lowercase()));
    }
    let sidecar_name = format!("{}.sha256", asset.name);
    let Some(sidecar) = siblings.iter().find(|a| a.name == sidecar_name) else {
        return Ok(None);
    };
    let text = http::get_string(&sidecar.browser_download_url)
        .with_context(|| format!("fetching {sidecar_name}"))?;
    let hex = text.split_whitespace().next().unwrap_or("").to_lowercase();
    Ok((!hex.is_empty()).then_some(hex))
}

/// Fetch the aarch64 mirror release for `version` from this repo. `Ok(None)`
/// on 404 (mirror hasn't been built for this version yet); `Err` on network
/// or parse failures.
fn fetch_mirror_release(version: &str) -> Result<Option<Release>> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/tags/wamr-mirror-{version}",
        mirror_repo()
    );
    match http::get_string(&url) {
        Ok(body) => Ok(Some(
            serde_json::from_str(&body).context("parsing mirror release JSON")?,
        )),
        Err(e) => {
            let msg = format!("{e:#}");
            if msg.contains("404") || msg.contains("Not Found") {
                Ok(None)
            } else {
                Err(e)
            }
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<Asset>,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
}

fn now_epoch() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn resolve_release(version_arg: &str) -> Result<Release> {
    let spec = VersionSpec::parse(version_arg).map_err(|e| anyhow!(e))?;
    if matches!(spec, VersionSpec::Lts) {
        bail!(
            "WAMR upstream does not designate LTS releases; use `latest` or a version like `2.4.5`"
        );
    }

    let url = match &spec {
        VersionSpec::Latest => format!("https://api.github.com/repos/{REPO}/releases/latest"),
        _ => {
            let available = fetch_release_versions(false)?;
            let version = spec
                .resolve(&available)
                .ok_or_else(|| anyhow!("no available WAMR version matches '{spec}'"))?;
            format!("https://api.github.com/repos/{REPO}/releases/tags/WAMR-{version}")
        }
    };
    let body = http::get_string(&url)
        .with_context(|| format!("fetching release metadata for {version_arg}"))?;
    serde_json::from_str(&body).context("parsing release JSON")
}

/// Ensure the newest match for `spec_str` (with variant `variant`) is installed,
/// downloading it if absent, and return the concrete version. Floating specs
/// may consult the network (bounded by the release cache); an exact spec
/// installs that version if missing. Offline, falls back to the best installed
/// match.
pub fn ensure(spec_str: &str, variant: &str) -> Result<String> {
    let layout = Layout::discover()?;
    let spec = VersionSpec::parse(spec_str).map_err(|e| anyhow!(e))?;
    if matches!(spec, VersionSpec::Lts) {
        bail!("WAMR upstream does not designate LTS releases");
    }

    let installed = discovery::installed_versions(&layout)?;
    // Only versions that have *this variant* installed count as satisfying.
    let installed_with_variant: Vec<String> = installed
        .into_iter()
        .filter(|v| layout.manifest_file(WAMR, v, variant).is_file())
        .collect();
    let installed_best = spec.resolve(&installed_with_variant).map(str::to_string);

    let remote_best = match fetch_release_versions(false) {
        Ok(list) => spec.resolve(&list).map(str::to_string),
        Err(_) => None,
    };

    let target = match (remote_best, installed_best) {
        (Some(r), Some(i)) => {
            if version_cmp(&r, &i) == Ordering::Greater {
                r
            } else {
                i
            }
        }
        (Some(r), None) => r,
        (None, Some(i)) => i,
        (None, None) => {
            bail!("no WAMR version (variant {variant}) matches '{spec}' (and none is installed)")
        }
    };

    if !layout.manifest_file(WAMR, &target, variant).is_file() {
        install_inner(&target, variant, false, false)?;
    }
    Ok(target)
}

/// List versions available from WAMR releases (first page, most recent first).
/// Only stable releases with at least one asset for this host are returned;
/// `all` includes prereleases and versions without a host asset.
pub fn fetch_release_versions(all: bool) -> Result<Vec<String>> {
    let layout = Layout::discover()?;
    let now = now_epoch();
    let ttl = cache::refresh_interval();

    if let Some(c) = cache::read(&layout, all) {
        if ttl == 0 || c.is_fresh(now, ttl) {
            return Ok(c.versions);
        }
    }

    let platform = Platform::detect()?;
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=100");
    let body = http::get_string(&url).context("fetching release list")?;
    let releases: Vec<Release> = serde_json::from_str(&body).context("parsing release list")?;

    let mut out = Vec::new();
    for r in &releases {
        if r.draft {
            continue;
        }
        if r.prerelease && !all {
            continue;
        }
        // Filter WAMR's pre-2.x tags — the repo also carries old date-shaped
        // tags like `12-30-2021` and `fast-jit-06-29-2022` that aren't
        // proper releases. Real releases are tagged `WAMR-<X.Y.Z>`.
        if !r.tag_name.starts_with("WAMR-") {
            continue;
        }
        let version = normalize_version(&r.tag_name);
        // Belt-and-suspenders: reject anything that doesn't parse as an exact
        // version, so a stray `WAMR-nightly-foo` never leaks in.
        if !matches!(VersionSpec::parse(&version), Ok(VersionSpec::Exact(_))) {
            continue;
        }
        let has_build = r
            .assets
            .iter()
            .any(|a| platform.matches_asset(&a.name, DEFAULT_VARIANT, &version));
        // On aarch64 the upstream release has no assets we can match, but the
        // mirror may (or will) carry them. Accept the version and let install
        // resolve the actual mirror asset lazily.
        if !has_build && !all && !platform.needs_mirror {
            continue;
        }
        out.push(version);
    }
    let _ = cache::write(&layout, all, &out, now);
    Ok(out)
}

/// `wrvm install <spec>` — resolve and install a runtime. When `auto_default`
/// is set and no default exists yet, the first install becomes the default.
pub fn install(version_arg: &str, variant: &str, make_default: bool) -> Result<()> {
    install_inner(version_arg, variant, make_default, true)
}

fn store_default_spec(layout: &Layout, version_arg: &str, resolved: &str) -> Result<()> {
    let spec = VersionSpec::parse(version_arg)
        .map(|s| s.to_string())
        .unwrap_or_else(|_| resolved.to_string());
    discovery::set_default_version(layout, &spec)?;
    if spec != resolved {
        println!("Default is now '{spec}' (currently WAMR {resolved}, used by new shells)");
    } else {
        println!("Default is now WAMR {resolved} (used by new shells)");
    }
    Ok(())
}

fn install_inner(
    version_arg: &str,
    variant: &str,
    make_default: bool,
    auto_default: bool,
) -> Result<()> {
    validate_variant(variant)?;

    let layout = Layout::discover()?;
    layout.ensure_base()?;
    let platform = Platform::detect()?;

    let sp = Spinner::new("Resolving release");
    let release = resolve_release(version_arg)?;
    let version = normalize_version(&release.tag_name);
    sp.finish(&format!("Resolved WAMR {version}"));

    if layout.manifest_file(WAMR, &version, variant).is_file() {
        println!("WAMR {version} ({variant}) is already installed");
        if make_default {
            store_default_spec(&layout, version_arg, &version)?;
        }
        return Ok(());
    }

    // Prefer upstream. On aarch64, fall back to the in-repo mirror. Track the
    // sibling asset list from whichever release the pick came from, so
    // sidecar `.sha256` lookup below hits the right release.
    let upstream_hit = release
        .assets
        .iter()
        .find(|a| platform.matches_asset(&a.name, variant, &version))
        .cloned();
    let (asset, siblings) = match (upstream_hit, platform.needs_mirror) {
        (Some(a), _) => (a, release.assets.clone()),
        (None, true) => {
            let mirror = fetch_mirror_release(&version)
                .with_context(|| format!("consulting aarch64 mirror for WAMR-{version}"))?
                .ok_or_else(|| {
                    anyhow!(
                        "aarch64 mirror release wamr-mirror-{version} not found on {} — \
                         run the `mirror-wamr` workflow for this version, or use x86_64",
                        mirror_repo()
                    )
                })?;
            let hit = mirror
                .assets
                .iter()
                .find(|a| platform.matches_asset(&a.name, variant, &version))
                .cloned()
                .ok_or_else(|| {
                    anyhow!(
                        "no aarch64 asset for variant '{variant}' in wamr-mirror-{version} — \
                         re-run the `mirror-wamr` workflow if a variant was added"
                    )
                })?;
            (hit, mirror.assets)
        }
        (None, false) => bail!(
            "no asset for variant '{variant}' (host {}-{}) in WAMR-{version}",
            platform.arch,
            platform.os
        ),
    };

    let download_path = layout.downloads_dir().join(&asset.name);
    http::download_with_progress(
        &asset.browser_download_url,
        &download_path,
        &format!("Downloading {}", asset.name),
    )?;

    let archive_sha256 = hash::sha256_file(&download_path)?;
    match expected_sha256(&asset, &siblings)? {
        Some(expected) if expected != archive_sha256 => {
            let _ = std::fs::remove_file(&download_path);
            bail!(
                "checksum mismatch for {}: expected {expected}, got {archive_sha256}",
                asset.name
            );
        }
        Some(_) => eprintln!("✓ Verified checksum ({}…)", &archive_sha256[..12]),
        None => eprintln!("warning: no published checksum for {}", asset.name),
    }

    let count = materialize_install(
        &layout,
        &version,
        variant,
        &platform,
        &download_path,
        archive_sha256,
    )?;
    let _ = std::fs::remove_file(&download_path);
    println!("Installed WAMR {version} ({variant}, {count} files)");

    if make_default || (auto_default && discovery::default_version(&layout).is_none()) {
        store_default_spec(&layout, version_arg, &version)?;
    }
    Ok(())
}

/// `wrvm install <version> --variant <v> --from <archive>` — install from a
/// local `.tar.gz` without any network. Version must be exact.
pub fn install_from(
    version_arg: &str,
    variant: &str,
    archive_path: &str,
    make_default: bool,
) -> Result<()> {
    validate_variant(variant)?;
    let layout = Layout::discover()?;
    layout.ensure_base()?;
    let platform = Platform::detect()?;

    let version = match VersionSpec::parse(version_arg).map_err(|e| anyhow!(e))? {
        VersionSpec::Exact(v) => v,
        _ => bail!(
            "install --from requires an exact version, e.g. `wrvm install 2.4.5 --from <archive>`"
        ),
    };
    if layout.manifest_file(WAMR, &version, variant).is_file() {
        println!("WAMR {version} ({variant}) is already installed");
        if make_default {
            store_default_spec(&layout, version_arg, &version)?;
        }
        return Ok(());
    }

    let archive = Path::new(archive_path);
    if !archive.is_file() {
        bail!("archive not found: {archive_path}");
    }
    let archive_sha256 = hash::sha256_file(archive)?;
    let count = materialize_install(
        &layout,
        &version,
        variant,
        &platform,
        archive,
        archive_sha256,
    )?;
    println!("Installed WAMR {version} ({variant}) from {archive_path} ({count} files)");

    if make_default || discovery::default_version(&layout).is_none() {
        store_default_spec(&layout, version_arg, &version)?;
    }
    Ok(())
}

fn materialize_install(
    layout: &Layout,
    version: &str,
    variant: &str,
    platform: &Platform,
    archive_path: &Path,
    archive_sha256: String,
) -> Result<usize> {
    let extract = Spinner::new("Extracting archive");
    let files = archive::extract_tar_gz(archive_path)?;
    extract.finish(&format!("Extracted {} files", files.len()));

    let staging = layout
        .version_dir(WAMR, version)
        .join(format!(".staging-{variant}"));
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    std::fs::create_dir_all(&staging)
        .with_context(|| format!("creating staging dir {}", staging.display()))?;

    let mut entries = Vec::new();
    let mut store_sp = Spinner::new("Writing files");
    for (i, f) in files.iter().enumerate() {
        let dest = staging.join(&f.logical_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&dest, &f.data).with_context(|| format!("writing {}", dest.display()))?;
        set_mode(&dest, f.mode);
        entries.push(FileEntry {
            path: f.logical_path.clone(),
            sha256: hash::sha256_hex(&f.data),
            mode: mode_string(f.mode),
            size: f.data.len() as u64,
        });
        store_sp.tick(&format!("{}/{}", i + 1, files.len()));
    }
    store_sp.finish(&format!("Wrote {} files", files.len()));

    let manifest = Manifest {
        runtime: WAMR.to_string(),
        version: version.to_string(),
        variant: variant.to_string(),
        platform: platform.label(),
        archive_sha256,
        files: entries,
    };
    manifest.write(&staging.join("manifest.json"))?;

    let final_dir = layout.variant_dir(WAMR, version, variant);
    if final_dir.exists() {
        let _ = std::fs::remove_dir_all(&final_dir);
    }
    if let Some(parent) = final_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&staging, &final_dir)
        .with_context(|| format!("publishing {}", final_dir.display()))?;
    Ok(files.len())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o7777));
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) {}

pub fn validate_variant(variant: &str) -> Result<()> {
    if VARIANTS.contains(&variant) {
        Ok(())
    } else {
        bail!(
            "unknown variant '{variant}'; expected one of: {}",
            VARIANTS.join(", ")
        )
    }
}
