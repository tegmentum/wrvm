//! wrvm CLI entry point.

mod appmanifest;
mod apps;
mod archive;
mod cache;
mod commands;
mod completions;
mod discovery;
mod doctor;
mod hash;
mod http;
mod install;
mod layout;
mod manifest;
mod platform;
mod progress;
mod selfupdate;
mod shell;
mod shim;
mod spec;
mod usage;
mod util;

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

use crate::layout::{Layout, DEFAULT_VARIANT};

fn main() {
    let invoked_as = std::env::args()
        .next()
        .and_then(|p| {
            Path::new(&p)
                .file_name()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
        .unwrap_or_default();

    let result = match invoked_as.as_str() {
        "iwasm" | "iwasm.exe" => shim::run("iwasm"),
        "wamrc" | "wamrc.exe" => shim::run("wamrc"),
        _ => run(),
    };
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if matches!(
        args.first().map(String::as_str),
        Some("--version") | Some("-V")
    ) {
        println!("wrvm {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("--upgrade") {
        let check_only = args.iter().any(|a| a == "--check");
        return selfupdate::run(check_only);
    }
    if args.first().map(String::as_str) == Some("completions") {
        if args.get(1).map(String::as_str) == Some("--installed") {
            return completions::installed();
        }
        return completions::print(args.get(1).map(String::as_str));
    }
    if args.first().map(String::as_str) == Some("shell-init") {
        let layout = Layout::discover()?;
        print!("{}", shell::integration(&layout.shims_dir()));
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("setup") {
        let layout = Layout::discover()?;
        return shell::setup(&layout);
    }

    let layout = Layout::discover()?;
    std::fs::create_dir_all(&layout.root)
        .with_context(|| format!("creating {}", layout.root.display()))?;

    let _ = ensure_shims(&layout);

    if args.first().map(String::as_str) == Some("exec") {
        return exec_runtime(&layout, &args[1..]);
    }

    selfupdate::notify(&layout);

    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let positional = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(String::as_str);
    let flag = |name: &str| args.iter().skip(1).any(|a| a == name);

    match cmd {
        "install" => {
            let variant =
                flag_str(&args, "--variant").unwrap_or_else(|| DEFAULT_VARIANT.to_string());
            let make_default = flag("--default") || flag("--use");
            match (positional, flag_str(&args, "--from")) {
                (Some(v), Some(from)) => install::install_from(v, &variant, &from, make_default),
                (Some(v), None) => install::install(v, &variant, make_default),
                (None, _) => missing_arg("install <version> [--variant <v>]"),
            }
        }
        "list" => {
            let variant =
                flag_str(&args, "--variant").unwrap_or_else(|| DEFAULT_VARIANT.to_string());
            commands::list(flag("--all"), &variant)
        }
        "current" => commands::current(),
        "path" => {
            let variant =
                flag_str(&args, "--variant").unwrap_or_else(|| DEFAULT_VARIANT.to_string());
            commands::path(positional, &variant)
        }
        "default" => {
            let variant =
                flag_str(&args, "--variant").unwrap_or_else(|| DEFAULT_VARIANT.to_string());
            match positional {
                Some(v) => commands::set_default(v, &variant),
                None => missing_arg("default <version> [--variant <v>]"),
            }
        }
        "use" => {
            let variant =
                flag_str(&args, "--variant").unwrap_or_else(|| DEFAULT_VARIANT.to_string());
            match positional {
                Some(v) => commands::use_version(v, &variant),
                None => missing_arg("use <version> [--variant <v>]"),
            }
        }
        "upgrade" => {
            let variant =
                flag_str(&args, "--variant").unwrap_or_else(|| DEFAULT_VARIANT.to_string());
            commands::upgrade(positional, flag("--all"), &variant)
        }
        "deactivate" => commands::deactivate(),
        "register" => match positional {
            Some(dir) => commands::register(dir),
            None => missing_arg("register <app-dir>"),
        },
        "unregister" => match positional {
            Some(name) => commands::unregister(name),
            None => missing_arg("unregister <name>"),
        },
        "apps" => commands::apps(),
        "usage" => {
            let limit = flag_i64(&args, "--limit").unwrap_or(20);
            commands::usage(limit)
        }
        "uninstall" => {
            let variant = flag_str(&args, "--variant");
            match positional {
                Some(v) => commands::uninstall(v, variant.as_deref(), flag("--force")),
                None => missing_arg("uninstall <version> [--variant <v>]"),
            }
        }
        "verify" => commands::verify(positional),
        "doctor" => doctor::run(&layout),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => {
            eprintln!("error: unknown command `{other}`");
            print_help();
            std::process::exit(2);
        }
    }
}

fn exec_runtime(layout: &Layout, raw: &[String]) -> Result<()> {
    let mut no_usage = false;
    let mut binary_name = "iwasm";
    let mut rest = raw;
    loop {
        match rest.first().map(String::as_str) {
            Some("--no-usage") => {
                no_usage = true;
                rest = &rest[1..];
            }
            Some("--wamrc") => {
                binary_name = "wamrc";
                rest = &rest[1..];
            }
            Some("--") => {
                rest = &rest[1..];
                break;
            }
            _ => break,
        }
    }
    let forwarded = rest;
    let cwd = std::env::current_dir().context("getting current directory")?;

    let resolved = discovery::resolve(layout, &cwd, binary_name)?;
    if std::env::var_os("WRVM_VERBOSE").is_some() {
        eprintln!(
            "wrvm: using {binary_name} from {} [{}]",
            resolved.binary.display(),
            resolved.source
        );
    }

    if !no_usage {
        shim::record_invocation(layout, &resolved, Some(&cwd), forwarded);
    }

    shim::ensure_executable(&resolved.binary);
    let mut cmd = Command::new(&resolved.binary);
    cmd.args(forwarded);
    shim::exec_or_run(cmd, &resolved.binary)
}

#[cfg(unix)]
fn ensure_shims(layout: &Layout) -> Result<()> {
    let exe = std::env::current_exe().context("locating the wrvm binary")?;
    let shims_dir = layout.shims_dir();
    std::fs::create_dir_all(&shims_dir)?;
    for name in ["iwasm", "wamrc"] {
        let shim = layout.shim_bin(name);
        match std::fs::read_link(&shim) {
            Ok(target) if target == exe => continue,
            _ => {
                let _ = std::fs::remove_file(&shim);
            }
        }
        std::os::unix::fs::symlink(&exe, &shim)
            .with_context(|| format!("linking shim {}", shim.display()))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_shims(_layout: &Layout) -> Result<()> {
    Ok(())
}

fn missing_arg(usage: &str) -> Result<()> {
    bail!("usage: wrvm {usage}")
}

fn flag_str(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().cloned();
        }
        if let Some(rest) = a.strip_prefix(name).and_then(|r| r.strip_prefix('=')) {
            return Some(rest.to_string());
        }
    }
    None
}

fn flag_i64(args: &[String], name: &str) -> Option<i64> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().and_then(|v| v.parse().ok());
        }
        if let Some(rest) = a.strip_prefix(name).and_then(|r| r.strip_prefix('=')) {
            return rest.parse().ok();
        }
    }
    None
}

fn print_help() {
    println!("wrvm — WAMR (WebAssembly Micro Runtime) Version Manager");
    println!();
    println!("Commands:");
    println!("  install <spec> [--variant <v>] [--default|--use]");
    println!("                       Install a runtime (spec: latest, 2, 2.4, or 2.4.5)");
    println!(
        "    install <ver> --variant <v> --from <archive>   Install offline from a local .tar.gz"
    );
    println!("  list [--all] [--variant <v>]  List all available versions (installed ones marked)");
    println!("  current              Show the effective runtime version (session or default)");
    println!("  path [spec] [--variant <v>]   Print a runtime's filesystem path");
    println!(
        "  default <spec> [--variant <v>]   Set the persistent default (floats: latest/2/2.4)"
    );
    println!("  use <spec> [--variant <v>]    Switch the runtime for the current shell (needs shell-init)");
    println!(
        "  upgrade [spec] [--all] [--variant <v>]   Pull the newest match for a floating line now"
    );
    println!("  deactivate           Clear the per-shell override (revert to default)");
    println!("  shell-init           Print the shell hook enabling per-shell `use`");
    println!("  setup                Wire the shell hook into your login-shell rc (idempotent)");
    println!("  uninstall <version> [--variant <v>] [--force]   Remove an installed runtime");
    println!("  register <app-dir>   Record an app's runtime dependency (reads its wrvm.toml)");
    println!("  unregister <name>    Drop an application's registration");
    println!("  apps                 List registered applications and their runtimes");
    println!("  usage [--limit N]    Show runtime invocations observed via the shim");
    println!(
        "  exec [--no-usage] [--wamrc] [--] <args>   Run the selected runtime, forwarding args"
    );
    println!("  verify [version]     Validate installation integrity");
    println!("  completions <shell>  Print a completion script (bash, zsh, fish)");
    println!("  doctor               Diagnose the install, shell integration, and PATH");
    println!();
    println!("Variants: iwasm (default), iwasm-gc-eh, wamrc, wasi-extensions.");
    println!();
    println!("Self-management:");
    println!("  --version, -V        Print the wrvm version");
    println!(
        "  --upgrade [--check]  Update wrvm itself to the latest release (--check only reports)"
    );
}
