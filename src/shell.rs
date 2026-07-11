//! Shell integration snippet emitted by both `wrvm shell-init` and the
//! installer's env file.

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
