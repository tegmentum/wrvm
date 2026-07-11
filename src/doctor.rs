//! `wrvm doctor` — diagnose the installation and shell integration.

use crate::discovery;
use crate::layout::Layout;
use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn run(layout: &Layout) -> Result<()> {
    println!("wrvm doctor\n");
    let mut problems = 0usize;
    let exe = std::env::current_exe().ok();
    let shims_dir = layout.shims_dir();
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();

    section("WRVM_HOME");
    if dir_writable(&layout.root) {
        ok(&format!("{} (writable)", layout.root.display()));
    } else {
        fail(&format!("{} is not writable", layout.root.display()));
        problems += 1;
    }

    section("wrvm binary");
    let where_bin = exe
        .as_ref()
        .map(|e| format!(" at {}", e.display()))
        .unwrap_or_default();
    ok(&format!("wrvm {}{where_bin}", env!("CARGO_PKG_VERSION")));

    section("Host support");
    match crate::platform::Platform::detect() {
        Ok(p) => ok(&format!("{}-{}  (asset ext .{})", p.os, p.arch, p.ext)),
        Err(e) => {
            fail(&format!("{e}"));
            problems += 1;
        }
    }

    section("Shim & PATH");
    for name in ["iwasm", "wamrc"] {
        let shim = layout.shim_bin(name);
        match (exe.as_deref(), std::fs::read_link(&shim).ok()) {
            (Some(e), Some(t)) if t == e => {
                ok(&format!("shims/{name} → {} (current)", e.display()))
            }
            (_, Some(t)) => warn(&format!(
                "shims/{name} → {} (stale; any wrvm command refreshes it)",
                t.display()
            )),
            (_, None) if shim.exists() => ok(&format!("shims/{name} present")),
            _ => warn(&format!(
                "shims/{name} missing (any wrvm command creates it)"
            )),
        }
    }
    match path_dirs.iter().position(|d| d == &shims_dir) {
        Some(i) => ok(&format!("shims dir on PATH (position {})", i + 1)),
        None => {
            fail(&format!(
                "shims dir not on PATH — run `wrvm shell-init >> {}`",
                default_rc().display()
            ));
            problems += 1;
        }
    }

    let externals = detect_external(layout, &path_dirs);
    if let Some((p, _)) = externals.iter().find(|(p, _)| {
        first_path_index(&path_dirs, p.parent())
            .zip(path_dirs.iter().position(|d| d == &shims_dir))
            .map(|(ext, shim)| ext < shim)
            .unwrap_or(false)
    }) {
        warn(&format!(
            "an external iwasm at {} comes before the shim on PATH — `iwasm` will bypass wrvm",
            p.display()
        ));
    }

    section("Shell integration");
    match detect_hook(&shims_dir) {
        Some(f) => ok(&format!("shim/use hook found in {f}")),
        None => warn(&format!(
            "hook not found — run `wrvm shell-init >> {}`, then restart your shell",
            default_rc().display()
        )),
    }

    section("Default runtime");
    match discovery::default_version(layout) {
        Some(spec) => match discovery::resolve_installed(layout, &spec) {
            Some(v) => ok(&format!("default '{spec}' → {v} (installed)")),
            None => warn(&format!(
                "default '{spec}' set but no matching version installed — `wrvm install {spec}`"
            )),
        },
        None => warn("no default set — `wrvm install latest` then `wrvm default latest`"),
    }

    section("External iwasm binaries (not managed by wrvm)");
    if externals.is_empty() {
        ok("none found");
    } else {
        for (p, ver) in &externals {
            println!(
                "  • {}  at {}",
                ver.as_deref().unwrap_or("unknown version"),
                p.display()
            );
        }
        println!("  (wrvm can fall back to these via IWASM_HOME / PATH; it does not manage them)");
    }

    println!();
    if problems == 0 {
        println!("No problems found.");
        Ok(())
    } else {
        println!("{problems} problem(s) found.");
        std::process::exit(1);
    }
}

fn detect_external(layout: &Layout, path_dirs: &[PathBuf]) -> Vec<(PathBuf, Option<String>)> {
    let mut candidates: Vec<PathBuf> = path_dirs.iter().map(|d| d.join("iwasm")).collect();
    for var in ["IWASM_HOME", "WAMR_HOME"] {
        if let Some(v) = std::env::var_os(var) {
            let p = PathBuf::from(v);
            candidates.push(p.join("bin").join("iwasm"));
            candidates.push(p.join("iwasm"));
        }
    }
    for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        candidates.push(PathBuf::from(dir).join("iwasm"));
    }

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for c in candidates {
        if !c.is_file() {
            continue;
        }
        let canon = std::fs::canonicalize(&c).unwrap_or(c);
        if canon.starts_with(&layout.root) {
            continue;
        }
        if !seen.insert(canon.clone()) {
            continue;
        }
        let version = Command::new(&canon)
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .map(|l| l.trim().to_string())
            })
            .filter(|s| !s.is_empty());
        out.push((canon, version));
    }
    out
}

fn first_path_index(path_dirs: &[PathBuf], dir: Option<&Path>) -> Option<usize> {
    let dir = dir?;
    path_dirs.iter().position(|d| d == dir)
}

fn default_rc() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let shell = std::env::var("SHELL").unwrap_or_default();
    let base = Path::new(&shell)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    match base {
        "zsh" => home.join(".zshrc"),
        "bash" => home.join(".bashrc"),
        "fish" => home.join(".config/fish/config.fish"),
        _ => home.join(".profile"),
    }
}

fn detect_hook(shims_dir: &Path) -> Option<String> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let needle = shims_dir.to_string_lossy().into_owned();
    let files = [
        ".zshrc",
        ".bashrc",
        ".bash_profile",
        ".profile",
        ".config/fish/config.fish",
    ];
    for f in files {
        let path = home.join(f);
        if let Ok(text) = std::fs::read_to_string(&path) {
            if text.contains(&needle) {
                return Some(f.to_string());
            }
        }
    }
    None
}

fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(".wrvm-doctor-probe");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn section(title: &str) {
    println!("{title}");
}

fn ok(msg: &str) {
    println!("  \u{2713} {msg}");
}

fn warn(msg: &str) {
    println!("  \u{26a0} {msg}");
}

fn fail(msg: &str) {
    println!("  \u{2717} {msg}");
}
