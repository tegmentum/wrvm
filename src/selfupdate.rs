//! Native in-place self-upgrade of the `wrvm` binary.

use crate::layout::Layout;
use crate::{cache, hash, http, util};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const DEFAULT_REPO: &str = "tegmentum/wrvm";

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Serialize, Deserialize)]
struct CheckState {
    checked_at: i64,
    latest: String,
}

fn repo() -> String {
    std::env::var("WRVM_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_string())
}

fn asset_name() -> Result<String> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => bail!("unsupported architecture for self-upgrade: {other}"),
    };
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        other => bail!("unsupported OS for self-upgrade: {other}"),
    };
    Ok(format!("wrvm-{arch}-{os}"))
}

pub fn run(check_only: bool) -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");

    if let Some(exe) = canonical_current_exe() {
        if is_brew_owned(&exe) {
            print_brew_notice(current);
            return Ok(());
        }
    }

    let release = latest_release().context("checking for the latest wrvm release")?;
    let latest = util::normalize_version(release.tag_name.trim());

    if util::version_cmp(current, &latest) != std::cmp::Ordering::Less {
        println!("wrvm {current} is already up to date (latest release: {latest})");
        return Ok(());
    }

    if check_only {
        println!("a newer wrvm is available: {current} -> {latest}");
        println!("run `wrvm --upgrade` to install it");
        return Ok(());
    }

    let asset_name = asset_name()?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .ok_or_else(|| anyhow!("release {latest} has no asset {asset_name} for this host"))?;

    eprintln!("wrvm: upgrading {current} -> {latest} …");
    let bytes = http::get_bytes(&asset.browser_download_url)
        .with_context(|| format!("downloading {asset_name}"))?;

    if let Some(sum) = release
        .assets
        .iter()
        .find(|a| a.name == format!("{asset_name}.sha256"))
    {
        let text = http::get_string(&sum.browser_download_url)
            .with_context(|| format!("downloading {asset_name}.sha256"))?;
        let expected = text.split_whitespace().next().unwrap_or("").to_lowercase();
        if !expected.is_empty() {
            let observed = hash::sha256_hex(&bytes);
            if observed != expected {
                bail!("checksum mismatch for {asset_name}: expected {expected}, got {observed}");
            }
        }
    }

    replace_self(&bytes)?;
    println!("wrvm upgraded to {latest}");
    Ok(())
}

pub fn notify(layout: &Layout) {
    if std::env::var_os("WRVM_NO_UPDATE_NOTIFIER").is_some() {
        return;
    }
    // Homebrew-installed binaries can't be replaced in place — Homebrew owns
    // the Cellar file — so nagging about `wrvm --upgrade` would just point at
    // a command that can't work. `brew upgrade wrvm` is the right channel.
    if let Some(exe) = canonical_current_exe() {
        if is_brew_owned(&exe) {
            return;
        }
    }
    let current = env!("CARGO_PKG_VERSION");
    let now = now_epoch();
    let ttl = cache::refresh_interval();
    let state = read_check(layout);
    let fresh = matches!(&state, Some(s) if ttl > 0 && now.saturating_sub(s.checked_at) < ttl);

    let latest = if fresh || ttl == 0 {
        state.as_ref().map(|s| s.latest.clone())
    } else {
        match latest_release() {
            Ok(r) => {
                let v = util::normalize_version(r.tag_name.trim());
                let _ = write_check(layout, now, &v);
                Some(v)
            }
            Err(_) => state.as_ref().map(|s| s.latest.clone()),
        }
    };

    if let Some(latest) = latest {
        if !latest.is_empty() && util::version_cmp(current, &latest) == std::cmp::Ordering::Less {
            eprintln!(
                "wrvm: a newer version is available ({current} -> {latest}); run `wrvm --upgrade`"
            );
        }
    }
}

fn check_file(layout: &Layout) -> PathBuf {
    layout.cache_dir().join("update-check.json")
}

fn read_check(layout: &Layout) -> Option<CheckState> {
    let text = std::fs::read_to_string(check_file(layout)).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_check(layout: &Layout, now: i64, latest: &str) -> Result<()> {
    let path = check_file(layout);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let state = CheckState {
        checked_at: now,
        latest: latest.to_string(),
    };
    std::fs::write(&path, serde_json::to_string(&state)?)?;
    Ok(())
}

fn now_epoch() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn latest_release() -> Result<Release> {
    // Ask GitHub for the release it flags as "latest" first — that already
    // excludes prereleases + drafts and respects the `make_latest` flag we
    // set on mirror publishes. Belt-and-suspenders: if the tag looks like a
    // WAMR runtime mirror (e.g. because a prior mirror publish forgot to set
    // make_latest: false), fall back to scanning /releases for the newest
    // v*-tagged tool release.
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo());
    let body = http::get_string(&url).with_context(|| format!("requesting {url}"))?;
    let release: Release = serde_json::from_str(&body).context("parsing release JSON")?;
    if is_tool_release_tag(&release.tag_name) {
        return Ok(release);
    }
    latest_tool_release()
}

/// True for tags that name a wrvm tool release (`v0.1.0`, `0.2.3`, …). False
/// for mirror or auxiliary tags (`wamr-mirror-*`, `nightly-*`, …).
fn is_tool_release_tag(tag: &str) -> bool {
    let s = tag.trim_start_matches('v');
    let first = s.split('.').next().unwrap_or("");
    !first.is_empty() && first.chars().all(|c| c.is_ascii_digit())
}

/// Scan /releases and return the newest release whose tag looks like a
/// wrvm tool version.
fn latest_tool_release() -> Result<Release> {
    let url = format!(
        "https://api.github.com/repos/{}/releases?per_page=50",
        repo()
    );
    let body = http::get_string(&url).with_context(|| format!("requesting {url}"))?;
    let all: Vec<Release> = serde_json::from_str(&body).context("parsing releases JSON")?;
    all.into_iter()
        .find(|r| is_tool_release_tag(&r.tag_name))
        .ok_or_else(|| anyhow!("no tool release found on {}", repo()))
}

fn replace_self(bytes: &[u8]) -> Result<()> {
    let exe = std::env::current_exe().context("locating the running wrvm binary")?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow!("wrvm binary path has no parent directory"))?;
    let tmp = dir.join(".wrvm.upgrade");

    std::fs::write(&tmp, bytes).with_context(|| {
        format!(
            "writing {} (need write access to {})",
            tmp.display(),
            dir.display()
        )
    })?;
    set_executable(&tmp)?;

    std::fs::rename(&tmp, &exe).with_context(|| {
        let _ = std::fs::remove_file(&tmp);
        format!(
            "replacing {}; if it is system-owned, re-run the install script or use sudo",
            exe.display()
        )
    })?;
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// Canonicalize `std::env::current_exe()` so symlink shims (e.g. Homebrew's
/// `/opt/homebrew/bin/wrvm` -> `…/Cellar/wrvm/<ver>/bin/wrvm`) resolve to the
/// real, package-manager-owned file. Returns `None` if either lookup fails.
fn canonical_current_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    std::fs::canonicalize(&exe).ok().or(Some(exe))
}

/// True when `exe` lives inside a Homebrew Cellar for the `wrvm` formula on
/// any of the three supported prefixes (Apple Silicon, Intel macOS,
/// Linuxbrew). Callers should pass a canonicalized path.
fn is_brew_owned(exe: &Path) -> bool {
    let s = exe.to_string_lossy();
    s.contains("/opt/homebrew/Cellar/wrvm/")
        || s.contains("/usr/local/Cellar/wrvm/")
        || s.contains("/home/linuxbrew/.linuxbrew/Cellar/wrvm/")
}

fn print_brew_notice(current: &str) {
    println!("wrvm {current} is installed via Homebrew; `wrvm --upgrade` cannot replace a brew-owned file.");
    println!("Upgrade with:");
    println!("    brew upgrade wrvm");
    println!("If brew reports no newer version, refresh its formula index first:");
    println!("    brew update && brew upgrade wrvm");
}

#[cfg(test)]
mod tests {
    use super::is_brew_owned;
    use std::path::Path;

    #[test]
    fn brew_owned_apple_silicon() {
        assert!(is_brew_owned(Path::new(
            "/opt/homebrew/Cellar/wrvm/0.1.3/bin/wrvm"
        )));
    }

    #[test]
    fn brew_owned_intel_macos() {
        assert!(is_brew_owned(Path::new(
            "/usr/local/Cellar/wrvm/0.1.3/bin/wrvm"
        )));
    }

    #[test]
    fn brew_owned_linuxbrew() {
        assert!(is_brew_owned(Path::new(
            "/home/linuxbrew/.linuxbrew/Cellar/wrvm/0.1.3/bin/wrvm"
        )));
    }

    #[test]
    fn not_brew_owned() {
        assert!(!is_brew_owned(Path::new("/usr/local/bin/wrvm")));
        assert!(!is_brew_owned(Path::new("/home/alice/.cargo/bin/wrvm")));
        assert!(!is_brew_owned(Path::new(
            "/home/alice/.tegmentum/wrvm/bin/wrvm"
        )));
    }
}
