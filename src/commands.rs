//! CLI command implementations.

use crate::appmanifest::AppManifest;
use crate::layout::{Layout, DEFAULT_VARIANT, WAMR};
use crate::manifest::Manifest;
use crate::spec::VersionSpec;
use crate::util::{normalize_version, version_cmp};
use crate::{apps as apps_mod, cache, discovery, hash, install, progress, usage};
use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashSet;
use std::path::Path;

fn now_epoch() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `wrvm list [--all] [--variant <v>]` — one list of all available versions
/// (most recent first), with installed ones marked. Offline falls back to
/// installed only.
pub fn list(all: bool, variant: &str) -> Result<()> {
    let layout = Layout::discover()?;
    layout.ensure_base()?;

    let default_spec = discovery::default_version(&layout);
    let default = default_spec
        .as_deref()
        .and_then(|s| discovery::resolve_installed(&layout, s));
    let effective = discovery::effective_version(&layout);

    let installed = discovery::installed_versions(&layout)?;
    let installed_set: HashSet<&str> = installed.iter().map(String::as_str).collect();

    let now = now_epoch();
    let usage_entries = usage::read(&layout).unwrap_or_default();
    let usage_map: std::collections::HashMap<String, i64> = usage::by_version(&usage_entries)
        .into_iter()
        .map(|u| (u.version, u.last_used))
        .collect();

    let (mut versions, offline) = match install::fetch_release_versions(all) {
        Ok(mut v) => {
            for i in &installed {
                if !v.contains(i) {
                    v.push(i.clone());
                }
            }
            (v, false)
        }
        Err(e) => {
            eprintln!("warning: could not fetch available versions ({e}); showing installed only");
            (installed.clone(), true)
        }
    };
    versions.sort_by(|a, b| version_cmp(b, a));
    versions.dedup();

    if versions.is_empty() {
        println!("No runtimes available. Try again with a network connection.");
        return Ok(());
    }

    if let Some(spec) = &default_spec {
        if VersionSpec::parse(spec)
            .map(|s| s.is_floating())
            .unwrap_or(false)
        {
            match &default {
                Some(v) => println!("Default: {spec} → {v}"),
                None => println!("Default: {spec} (no matching version installed)"),
            }
        }
    }
    println!("WAMR runtimes  (variant: {variant}; * current; tags: installed, default)");
    let width = versions.iter().map(String::len).max().unwrap_or(0);
    for v in &versions {
        let is_current = effective.as_ref().map(|(e, _)| e == v).unwrap_or(false);
        let marker = if is_current { "*" } else { " " };
        let has_variant = layout.manifest_file(WAMR, v, variant).is_file();
        let mut tags: Vec<&str> = Vec::new();
        if installed_set.contains(v.as_str()) {
            if has_variant {
                tags.push("installed");
            } else {
                tags.push("installed (other variant)");
            }
        }
        if default.as_deref() == Some(v.as_str()) {
            tags.push("default");
        }
        let suffix = if tags.is_empty() {
            String::new()
        } else {
            format!("\t[{}]", tags.join(", "))
        };
        let usage_note = match usage_map.get(v.as_str()) {
            Some(&t) if installed_set.contains(v.as_str()) => {
                format!("  · used {}", humanize_ago(now, t))
            }
            _ => String::new(),
        };
        println!("{marker} {v:<width$}{suffix}{usage_note}");
    }
    if offline {
        eprintln!("(offline: only installed versions shown)");
    }

    report_stale_runtimes(&layout, &usage_map)?;
    Ok(())
}

fn report_stale_runtimes(
    layout: &Layout,
    usage: &std::collections::HashMap<String, i64>,
) -> Result<()> {
    if usage.is_empty() {
        return Ok(());
    }
    let threshold_days: i64 = std::env::var("WRVM_STALE_DAYS")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(90);
    let now = now_epoch();
    let default_resolved =
        discovery::default_version(layout).and_then(|s| discovery::resolve_installed(layout, &s));

    let mut stale: Vec<(String, String)> = Vec::new();
    for v in discovery::installed_versions(layout)? {
        if default_resolved.as_deref() == Some(v.as_str()) {
            continue;
        }
        if !apps_mod::apps_using(layout, &v)
            .unwrap_or_default()
            .is_empty()
        {
            continue;
        }
        match usage.get(v.as_str()) {
            Some(&t) => {
                let age = (now - t) / 86400;
                if age >= threshold_days {
                    stale.push((v, format!("last used {age}d ago")));
                }
            }
            None => stale.push((v, "never used".to_string())),
        }
    }

    if !stale.is_empty() {
        println!("\nStale runtimes (unused ≥ {threshold_days}d; not default/app-required):");
        for (v, note) in stale {
            println!("  {v}   {note}   → wrvm uninstall {v}");
        }
    }
    Ok(())
}

pub fn current() -> Result<()> {
    let layout = Layout::discover()?;
    let Some((spec_str, source)) = discovery::effective_spec(&layout) else {
        eprintln!("no default runtime set (use `wrvm default <version>`)");
        std::process::exit(1);
    };
    match discovery::effective_version(&layout) {
        Some((v, _)) => {
            println!("{v}");
            let floating = VersionSpec::parse(&spec_str)
                .map(|s| s.is_floating())
                .unwrap_or(false);
            if floating {
                eprintln!("(resolved from '{spec_str}')");
            }
            if std::env::var_os("WRVM_VERBOSE").is_some() {
                eprintln!("(via {source})");
            }
        }
        None => {
            eprintln!(
                "selected '{spec_str}' but no matching version is installed; run `wrvm install {spec_str}`"
            );
            std::process::exit(1);
        }
    }
    Ok(())
}

pub fn path(spec_arg: Option<&str>, variant: &str) -> Result<()> {
    let layout = Layout::discover()?;
    let version = match spec_arg {
        Some(v) => discovery::resolve_installed(&layout, v)
            .ok_or_else(|| anyhow!("no installed WAMR matches '{v}'"))?,
        None => discovery::effective_version(&layout)
            .map(|(v, _)| v)
            .ok_or_else(|| {
                anyhow!("no default runtime; pass a version or run `wrvm default <version>`")
            })?,
    };
    let dir = layout.variant_dir(WAMR, &version, variant);
    if !dir.exists() {
        bail!("WAMR {version} ({variant}) is not installed");
    }
    println!("{}", dir.display());
    Ok(())
}

/// `wrvm default <spec>` — set the persistent default.
pub fn set_default(spec_arg: &str, variant: &str) -> Result<()> {
    let layout = Layout::discover()?;
    let spec = VersionSpec::parse(spec_arg).map_err(|e| anyhow!(e))?;
    let resolved = install::ensure(spec_arg, variant)?;
    discovery::set_default_version(&layout, &spec.to_string())?;
    if spec.is_floating() {
        println!("Default is now '{spec}' (currently WAMR {resolved}, used by new shells)");
    } else {
        println!("Default is now WAMR {resolved} (used by new shells)");
    }
    Ok(())
}

pub fn upgrade(spec_arg: Option<&str>, all: bool, variant: &str) -> Result<()> {
    let layout = Layout::discover()?;
    cache::clear(&layout);

    if all {
        let installed = discovery::installed_versions(&layout)?;
        let mut majors: Vec<u64> = installed
            .iter()
            .filter_map(|v| v.split('.').next().and_then(|m| m.parse().ok()))
            .collect();
        majors.sort_unstable();
        majors.dedup();
        if majors.is_empty() {
            println!("Nothing installed to upgrade.");
            return Ok(());
        }
        for m in majors {
            upgrade_one(&layout, &m.to_string(), variant)?;
        }
        return Ok(());
    }

    match spec_arg {
        Some(s) => upgrade_one(&layout, s, variant),
        None => match discovery::default_version(&layout) {
            Some(spec_str) => {
                let spec = VersionSpec::parse(&spec_str).map_err(|e| anyhow!(e))?;
                if !spec.is_floating() {
                    println!("Default is pinned to exact {spec_str}; nothing to upgrade.");
                    return Ok(());
                }
                upgrade_one(&layout, &spec_str, variant)
            }
            None => {
                println!("No default set; pass a spec (e.g. `wrvm upgrade 2`) or `--all`.");
                Ok(())
            }
        },
    }
}

fn upgrade_one(layout: &Layout, spec_str: &str, variant: &str) -> Result<()> {
    let before = discovery::resolve_installed(layout, spec_str);
    let after = install::ensure(spec_str, variant)?;
    match before {
        Some(b) if b == after => println!("{spec_str}: already up to date ({after})"),
        Some(b) => println!("{spec_str}: {b} → {after}"),
        None => println!("{spec_str}: installed {after}"),
    }
    Ok(())
}

fn shell_rc_file() -> &'static str {
    let shell = std::env::var("SHELL").unwrap_or_default();
    match shell.rsplit('/').next().unwrap_or("") {
        "bash" => "~/.bashrc",
        "zsh" => "~/.zshrc",
        "fish" => "~/.config/fish/config.fish",
        _ => "your shell startup file (e.g. ~/.bashrc)",
    }
}

pub fn use_version(spec_arg: &str, variant: &str) -> Result<()> {
    let spec = VersionSpec::parse(spec_arg).map_err(|e| anyhow!(e))?;
    let resolved = install::ensure(spec_arg, variant)?;

    if progress::stdout_is_terminal() {
        eprintln!("WAMR {resolved} ({variant}) is installed.");
        eprintln!(
            "`wrvm use` switches the runtime for the current shell, which needs the shell hook:"
        );
        eprintln!(
            "    wrvm shell-init >> {}   # once, then restart your shell",
            shell_rc_file()
        );
        eprintln!(
            "Then `wrvm use {spec}` applies to this shell. For the persistent default: `wrvm default {spec}`."
        );
    } else {
        println!("export {}={spec}", discovery::SESSION_VAR);
        if variant != DEFAULT_VARIANT {
            println!("export {}={variant}", discovery::VARIANT_VAR);
        }
        if spec.is_floating() {
            eprintln!("Now using '{spec}' (WAMR {resolved}) for this shell");
        } else {
            eprintln!("Now using WAMR {resolved} (this shell)");
        }
    }
    Ok(())
}

pub fn deactivate() -> Result<()> {
    let layout = Layout::discover()?;
    if progress::stdout_is_terminal() {
        eprintln!("`wrvm deactivate` clears the per-shell override and needs the shell hook (`wrvm shell-init`).");
    } else {
        println!("unset {}", discovery::SESSION_VAR);
        println!("unset {}", discovery::VARIANT_VAR);
        match discovery::default_version(&layout) {
            Some(d) => eprintln!("Reverted to default (WAMR {d}) for this shell"),
            None => eprintln!("Cleared session override (no default set)"),
        }
    }
    Ok(())
}

pub fn uninstall(version_arg: &str, variant: Option<&str>, force: bool) -> Result<()> {
    let layout = Layout::discover()?;
    let version = discovery::resolve_installed(&layout, version_arg)
        .unwrap_or_else(|| normalize_version(version_arg));
    if version != normalize_version(version_arg) {
        eprintln!("Resolved '{version_arg}' to installed WAMR {version}");
    }

    // If no variant given, wipe the whole version (all its variants).
    match variant {
        Some(v) => remove_variant(&layout, &version, v, force)?,
        None => {
            let variants = discovery::installed_variants(&layout, &version)?;
            if variants.is_empty() {
                bail!("WAMR {version} is not installed");
            }
            for v in &variants {
                remove_variant(&layout, &version, v, force)?;
            }
            let dir = layout.version_dir(WAMR, &version);
            if dir.exists() {
                let _ = std::fs::remove_dir_all(&dir);
            }
            if discovery::default_version(&layout).as_deref() == Some(version.as_str()) {
                let _ = std::fs::remove_file(layout.default_file(WAMR));
                eprintln!("note: {version} was the default; no default is set now");
            }
            println!(
                "Uninstalled WAMR {version} (variants: {})",
                variants.join(", ")
            );
        }
    }
    Ok(())
}

fn remove_variant(layout: &Layout, version: &str, variant: &str, force: bool) -> Result<()> {
    let dir = layout.variant_dir(WAMR, version, variant);
    if !dir.exists() {
        bail!("WAMR {version} ({variant}) is not installed");
    }
    let dependents = apps_mod::apps_using(layout, version).unwrap_or_default();
    if !dependents.is_empty() {
        if !force {
            bail!(
                "WAMR {version} is required by registered app(s): {}.\n\
                 Migrate them or re-run with --force to remove anyway.",
                dependents.join(", ")
            );
        }
        eprintln!(
            "warning: removing WAMR {version} still required by: {}",
            dependents.join(", ")
        );
    }
    std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
    println!("Uninstalled WAMR {version} ({variant})");
    Ok(())
}

pub fn verify(version_arg: Option<&str>) -> Result<()> {
    let layout = Layout::discover()?;
    let versions = match version_arg {
        Some(v) => vec![normalize_version(v)],
        None => discovery::installed_versions(&layout)?,
    };
    if versions.is_empty() {
        println!("No runtimes installed.");
        return Ok(());
    }

    let mut problems = 0usize;
    for version in &versions {
        let variants = discovery::installed_variants(&layout, version)?;
        if variants.is_empty() {
            println!("✗ {version}: no variants installed");
            problems += 1;
            continue;
        }
        for variant in variants {
            let manifest_path = layout.manifest_file(WAMR, version, &variant);
            let manifest = Manifest::read(&manifest_path)?;
            let variant_dir = layout.variant_dir(WAMR, version, &variant);
            let mut ok = true;
            for entry in &manifest.files {
                let p = variant_dir.join(&entry.path);
                if !p.exists() {
                    println!("✗ {version}/{variant}: {} is missing", entry.path);
                    ok = false;
                    continue;
                }
                let actual = hash::sha256_file(&p)?;
                if actual != entry.sha256 {
                    println!("✗ {version}/{variant}: {} digest mismatch", entry.path);
                    ok = false;
                }
            }
            if ok {
                println!(
                    "✓ {version}/{variant}: {} files verified",
                    manifest.files.len()
                );
            } else {
                problems += 1;
            }
        }
    }
    if problems > 0 {
        bail!("{problems} variant(s) failed verification");
    }
    Ok(())
}

pub fn register(app_dir: &str) -> Result<()> {
    let layout = Layout::discover()?;
    let dir = Path::new(app_dir);
    let manifest = AppManifest::read_dir(dir)?;

    apps_mod::register(
        &layout,
        &manifest.name,
        Some(app_dir),
        manifest.variant.as_deref(),
        manifest.runtime_path.as_deref(),
        &manifest.runtimes,
        now_epoch(),
    )?;

    println!("Registered application '{}'", manifest.name);
    if let Some(p) = &manifest.runtime_path {
        println!("  custom runtime: {p}");
    }
    if let Some(v) = &manifest.variant {
        println!("  variant: {v}");
    }
    if !manifest.runtimes.is_empty() {
        for v in &manifest.runtimes {
            let note = if is_installed(&layout, v) {
                ""
            } else {
                "  (not installed)"
            };
            println!("  runtime: {v}{note}");
        }
    }
    Ok(())
}

pub fn unregister(name: &str) -> Result<()> {
    let layout = Layout::discover()?;
    if apps_mod::unregister(&layout, name)? {
        println!("Unregistered application '{name}'");
    } else {
        bail!("no application named '{name}' is registered");
    }
    Ok(())
}

pub fn usage(limit: i64) -> Result<()> {
    let layout = Layout::discover()?;
    let entries = usage::read(&layout)?;

    let by_version = usage::by_version(&entries);
    if by_version.is_empty() {
        println!("No runtime usage recorded yet.");
        println!("Put the shim on PATH (`wrvm shell-init`) so apps that call `iwasm` are tracked.");
        return Ok(());
    }

    let now = now_epoch();
    println!("Runtime usage — recorded globally (all shells + `wrvm exec`) via the shim.");
    println!();

    let mut rollup: Vec<Vec<String>> = Vec::new();
    for u in &by_version {
        rollup.push(vec![
            u.version.clone(),
            u.count.to_string(),
            humanize_ago(now, u.last_used),
        ]);
    }
    print_table(
        &["VERSION", "RUNS", "LAST USED"],
        &rollup,
        &[Align::Left, Align::Right, Align::Left],
    );

    let recent = usage::recent(&entries, limit.max(0) as usize);
    if !recent.is_empty() {
        println!();
        println!("Recent invocations (newest first):");
        println!();
        let mut rows: Vec<Vec<String>> = Vec::new();
        for e in &recent {
            let who = e.app.as_deref().or(e.caller.as_deref()).unwrap_or("?");
            let module = e
                .module
                .as_deref()
                .map(|m| {
                    Path::new(m)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(m)
                        .to_string()
                })
                .unwrap_or_else(|| "-".to_string());
            rows.push(vec![
                humanize_ago(now, e.invoked_at),
                e.version.clone(),
                who.to_string(),
                module,
                e.cwd.clone().unwrap_or_else(|| "-".to_string()),
            ]);
        }
        print_table(
            &["WHEN", "VERSION", "WHO", "MODULE", "CWD"],
            &rows,
            &[
                Align::Left,
                Align::Left,
                Align::Left,
                Align::Left,
                Align::Left,
            ],
        );
    }
    Ok(())
}

enum Align {
    Left,
    Right,
}

fn print_table(headers: &[&str], rows: &[Vec<String>], aligns: &[Align]) {
    let ncol = headers.len();
    let mut width = vec![0usize; ncol];
    for (i, h) in headers.iter().enumerate() {
        width[i] = h.len();
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(ncol) {
            width[i] = width[i].max(cell.len());
        }
    }
    let render = |cells: &[String]| -> String {
        let mut line = String::from("  ");
        for (i, &w) in width.iter().enumerate() {
            let cell = cells.get(i).map(String::as_str).unwrap_or("");
            let last = i + 1 == ncol;
            match aligns.get(i).unwrap_or(&Align::Left) {
                Align::Right => line.push_str(&format!("{cell:>w$}")),
                Align::Left if last => line.push_str(cell),
                Align::Left => line.push_str(&format!("{cell:<w$}")),
            }
            if !last {
                line.push_str("  ");
            }
        }
        line
    };
    let header_cells: Vec<String> = headers.iter().map(|s| s.to_string()).collect();
    println!("{}", render(&header_cells));
    for row in rows {
        println!("{}", render(row));
    }
}

pub fn humanize_ago(now: i64, then: i64) -> String {
    let d = (now - then).max(0);
    if d < 60 {
        format!("{d}s ago")
    } else if d < 3600 {
        format!("{}m ago", d / 60)
    } else if d < 86400 {
        format!("{}h ago", d / 3600)
    } else {
        format!("{}d ago", d / 86400)
    }
}

pub fn apps() -> Result<()> {
    let layout = Layout::discover()?;
    let mut apps = apps_mod::read(&layout)?;
    apps.sort_by(|a, b| a.name.cmp(&b.name));
    if apps.is_empty() {
        println!("No applications registered yet.");
        println!(
            "Apps with an [app] section in wrvm.toml auto-register when they run through the shim \
             or `wrvm exec`; or register one now with `wrvm register <app-dir>`."
        );
        return Ok(());
    }

    println!("Registered applications:");
    for app in apps {
        let mut parts: Vec<String> = Vec::new();
        if !app.runtimes.is_empty() {
            let versions = app
                .runtimes
                .iter()
                .map(|v| {
                    if is_installed(&layout, v) {
                        v.clone()
                    } else {
                        format!("{v} (not installed)")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("runtimes: {versions}"));
        }
        if let Some(v) = &app.variant {
            parts.push(format!("variant: {v}"));
        }
        if let Some(p) = &app.runtime_path {
            parts.push(format!("custom runtime: {p}"));
        }
        let detail = if parts.is_empty() {
            "(no runtimes)".to_string()
        } else {
            parts.join("; ")
        };
        println!("  {}  {detail}", app.name);
        if let Some(p) = &app.path {
            println!("      at {p}");
        }
    }
    Ok(())
}

fn is_installed(layout: &Layout, version: &str) -> bool {
    layout.version_dir(WAMR, version).is_dir()
        && !discovery::installed_variants(layout, version)
            .unwrap_or_default()
            .is_empty()
}
