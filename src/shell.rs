//! Shell integration snippet emitted by both `wrvm shell-init` and the
//! installer's env file, plus `wrvm setup` which wires it into the user's
//! login-shell rc file. `setup` exists because Homebrew sandboxes formula
//! install steps and can't safely modify `$HOME`; calling wrvm directly is
//! the cleanest way for a brew-installed wrvm to auto-wire on first run.

use crate::layout::Layout;
use anyhow::{Context, Result};
use std::path::Path;

/// The `wrvm` wrapper function: for `use`/`deactivate` it eval's the command's
/// stdout so the override lands in the live shell; everything else forwards
/// untouched. bash/zsh compatible (uses `local`).
pub const HOOK: &str = r#"wrvm() {
  case "$1" in
    use|deactivate)
      local __wrvm_out
      __wrvm_out="$(command wrvm "$@")" || return $?
      [ -n "$__wrvm_out" ] && eval "$__wrvm_out"
      ;;
    *)
      command wrvm "$@" ;;
  esac
}
"#;

/// POSIX shell integration: prepend the shim dir to PATH and define the hook.
pub fn integration(shims_dir: &Path) -> String {
    format!(
        "# wrvm shell integration: route `iwasm` through wrvm and enable `wrvm use`.\n\
         export PATH=\"{}:$PATH\"\n{HOOK}",
        shims_dir.display()
    )
}

/// Marker tagging every line `setup` writes, so re-runs are no-ops and the
/// uninstall recipe is a one-liner.
const MANAGED_TAG: &str = "# wrvm-managed:env";

/// `wrvm setup` — append the shell integration to the user's login-shell rc.
/// Idempotent (via `MANAGED_TAG`) so re-running is safe. Matches the
/// `install.sh` convention so uninstall is `grep -v '# wrvm-managed' rc >
/// tmp && mv tmp rc`, regardless of how wrvm was installed.
pub fn setup(_layout: &Layout) -> Result<()> {
    let home = std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .context("HOME is not set")?;
    let home = Path::new(&home);

    let shell = std::env::var("SHELL").unwrap_or_default();
    let shell_base = Path::new(&shell)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let (rc, line) = match shell_base {
        "zsh" => {
            let zdir = std::env::var_os("ZDOTDIR")
                .filter(|v| !v.is_empty())
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| home.to_path_buf());
            (
                zdir.join(".zshrc"),
                format!("eval \"$(wrvm shell-init)\" {MANAGED_TAG}"),
            )
        }
        "bash" => (
            home.join(".bashrc"),
            format!("eval \"$(wrvm shell-init)\" {MANAGED_TAG}"),
        ),
        "fish" => (
            home.join(".config/fish/config.fish"),
            format!("wrvm shell-init | source {MANAGED_TAG}"),
        ),
        _ => (
            home.join(".profile"),
            format!("eval \"$(wrvm shell-init)\" {MANAGED_TAG}"),
        ),
    };

    if let Some(parent) = rc.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let existing = std::fs::read_to_string(&rc).unwrap_or_default();
    if existing.contains(MANAGED_TAG) {
        eprintln!(
            "wrvm setup: shell integration already present in {}",
            rc.display()
        );
        return Ok(());
    }

    let mut updated = existing.clone();
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&line);
    updated.push('\n');
    std::fs::write(&rc, updated).with_context(|| format!("writing {}", rc.display()))?;
    println!("wrvm setup: added shell integration to {}", rc.display());
    println!("Restart your shell — or run: eval \"$(wrvm shell-init)\"");
    Ok(())
}
