//! Pass-through runtime shim.
//!
//! When this binary is invoked as `iwasm` or `wamrc` (via `shims/…` links on
//! PATH), it resolves the active runtime, records the invocation to the usage
//! log, and execs the real runtime — forwarding all arguments.

use crate::{appmanifest, apps as apps_mod, discovery, hash, usage, util};
use anyhow::Result;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::layout::Layout;
use crate::usage::UsageEntry;

pub fn run(binary_name: &str) -> Result<()> {
    let layout = Layout::discover()?;
    let _ = std::fs::create_dir_all(&layout.root);
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let no_usage = raw.first().map(String::as_str) == Some("--no-usage");
    let args: Vec<String> = if no_usage { raw[1..].to_vec() } else { raw };
    let cwd = std::env::current_dir().ok();

    let resolve_dir = cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let resolved = discovery::resolve(&layout, &resolve_dir, binary_name)?;

    if !no_usage {
        record_invocation(&layout, &resolved, cwd.as_deref(), &args);
    }

    if std::env::var_os("WRVM_VERBOSE").is_some() {
        eprintln!(
            "wrvm(shim): {} [{}]",
            resolved.binary.display(),
            resolved.source
        );
    }

    ensure_executable(&resolved.binary);
    let mut cmd = Command::new(&resolved.binary);
    cmd.args(&args);
    exec_or_run(cmd, &resolved.binary)
}

pub fn record_invocation(
    layout: &Layout,
    resolved: &discovery::Resolved,
    cwd: Option<&Path>,
    args: &[String],
) {
    if std::env::var_os("WRVM_NO_USAGE").is_some() {
        return;
    }
    let module = identify_module(args);
    let module_path = module
        .as_deref()
        .and_then(|m| std::fs::canonicalize(m).ok())
        .map(|p| p.display().to_string());
    let module_sha256 = module.as_deref().and_then(|m| hash_module(Path::new(m)));

    let entry = UsageEntry {
        version: resolved.version.clone(),
        variant: Some(resolved.variant.clone()),
        runtime_path: Some(resolved.binary.display().to_string()),
        app: env_nonempty("WRVM_APP"),
        caller: detect_caller(),
        cwd: cwd.map(|c| c.display().to_string()),
        args: args.to_vec(),
        module,
        module_path,
        module_sha256,
        manifest: discover_app(cwd),
        invoked_at: now_epoch(),
    };
    let _ = usage::record(layout, &entry);

    if let Some(app_ref) = &entry.manifest {
        let _ = apps_mod::register(
            layout,
            &app_ref.name,
            Some(&app_ref.dir),
            app_ref.variant.as_deref(),
            app_ref.runtime_path.as_deref(),
            &app_ref.runtimes,
            entry.invoked_at,
        );
    }
}

fn discover_app(cwd: Option<&Path>) -> Option<usage::AppRef> {
    let mut dir = cwd?;
    loop {
        if dir.join(discovery::PIN_FILE).is_file() {
            let m = appmanifest::AppManifest::read_dir(dir).ok()?;
            return Some(usage::AppRef {
                name: m.name,
                dir: dir.display().to_string(),
                runtimes: m.runtimes,
                variant: m.variant,
                runtime_path: m.runtime_path,
            });
        }
        dir = dir.parent()?;
    }
}

fn hash_module(path: &Path) -> Option<String> {
    if let Ok(meta) = std::fs::metadata(path) {
        let threshold = large_module_threshold();
        if threshold > 0 && meta.len() >= threshold && std::io::stderr().is_terminal() {
            eprintln!(
                "wrvm: hashing large module ({}) for usage tracking; \
                 opt out with `--no-usage` or WRVM_NO_USAGE=1",
                util::human_bytes(meta.len())
            );
        }
    }
    hash::sha256_file(path).ok()
}

fn large_module_threshold() -> u64 {
    std::env::var("WRVM_HASH_WARN_MB")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(100)
        .saturating_mul(1024 * 1024)
}

/// Best-effort: the module argument in an iwasm command line — first positional
/// that is an existing file or has a wasm-ish extension.
fn identify_module(args: &[String]) -> Option<String> {
    for a in args {
        if a.starts_with('-') {
            continue;
        }
        if Path::new(a).is_file()
            || a.ends_with(".wasm")
            || a.ends_with(".wat")
            || a.ends_with(".aot")
        {
            return Some(a.clone());
        }
    }
    None
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

#[cfg(target_os = "linux")]
fn detect_caller() -> Option<String> {
    let ppid = std::os::unix::process::parent_id();
    std::fs::read_to_string(format!("/proc/{ppid}/comm")).map_or(None, |s| {
        let name = s.trim().to_string();
        (!name.is_empty()).then_some(name)
    })
}

#[cfg(target_os = "macos")]
fn detect_caller() -> Option<String> {
    let ppid = std::os::unix::process::parent_id();
    let out = std::process::Command::new("/bin/ps")
        .args(["-o", "comm=", "-p", &ppid.to_string()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let name = s.trim();
    let base = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(name);
    (!base.is_empty()).then(|| base.to_string())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn detect_caller() -> Option<String> {
    None
}

pub fn now_epoch() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(unix)]
pub fn ensure_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mode = meta.permissions().mode();
        if mode & 0o111 == 0 {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode | 0o755));
        }
    }
}

#[cfg(not(unix))]
pub fn ensure_executable(_path: &Path) {}

#[cfg(unix)]
pub fn exec_or_run(mut cmd: Command, bin: &Path) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let err = cmd.exec();
    Err(anyhow::anyhow!("failed to exec {}: {err}", bin.display()))
}

#[cfg(not(unix))]
pub fn exec_or_run(mut cmd: Command, bin: &Path) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("running {}", bin.display()))?;
    std::process::exit(status.code().unwrap_or(1));
}
