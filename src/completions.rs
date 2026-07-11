//! Static shell-completion script generation.

use crate::discovery;
use crate::layout::Layout;
use anyhow::{bail, Result};

const COMMANDS: &[(&str, &str)] = &[
    ("install", "Install a runtime"),
    ("list", "List available versions"),
    ("current", "Show the effective runtime version"),
    ("path", "Print a runtime's filesystem path"),
    ("default", "Set the persistent default"),
    ("use", "Switch the runtime for this shell"),
    ("upgrade", "Pull the newest match for a floating line"),
    ("deactivate", "Clear the per-shell override"),
    ("shell-init", "Print the shell hook for `use`"),
    ("register", "Record an app's runtime dependency"),
    ("unregister", "Drop an application registration"),
    ("apps", "List registered applications"),
    ("usage", "Show runtime invocations"),
    ("uninstall", "Remove an installed runtime"),
    ("verify", "Validate installation integrity"),
    ("doctor", "Diagnose install, PATH, and shell integration"),
    ("completions", "Print a shell completion script"),
    ("help", "Show help"),
];

const SELECT_CMDS: &[&str] = &["use", "default", "path", "upgrade"];
const INSTALL_CMDS: &[&str] = &["install"];
const REMOVE_CMDS: &[&str] = &["uninstall"];

const INSTALLED_CMD: &str = "wrvm completions --installed 2>/dev/null";

pub fn print(shell: Option<&str>) -> Result<()> {
    match shell {
        Some("bash") => print!("{}", bash()),
        Some("zsh") => print!("{}", zsh()),
        Some("fish") => print!("{}", fish()),
        Some(other) => bail!("unsupported shell `{other}` (expected: bash, zsh, or fish)"),
        None => bail!("usage: wrvm completions <bash|zsh|fish>"),
    }
    Ok(())
}

pub fn installed() -> Result<()> {
    let layout = Layout::discover()?;
    for v in discovery::installed_versions(&layout).unwrap_or_default() {
        println!("{v}");
    }
    Ok(())
}

fn command_names() -> String {
    COMMANDS
        .iter()
        .map(|(c, _)| *c)
        .collect::<Vec<_>>()
        .join(" ")
}

fn alt(cmds: &[&str]) -> String {
    cmds.join("|")
}

fn bash() -> String {
    format!(
        r#"# bash completion for wrvm
_wrvm() {{
    local cur prev
    cur="${{COMP_WORDS[COMP_CWORD]}}"
    prev="${{COMP_WORDS[COMP_CWORD-1]}}"
    if [ "$COMP_CWORD" -eq 1 ]; then
        COMPREPLY=( $(compgen -W "{cmds} --version --upgrade --help" -- "$cur") )
        return
    fi
    case "$prev" in
        {select}) COMPREPLY=( $(compgen -W "latest $({installed})" -- "$cur") ); return ;;
        {install}) COMPREPLY=( $(compgen -W "latest" -- "$cur") ); return ;;
        {remove}) COMPREPLY=( $(compgen -W "$({installed})" -- "$cur") ); return ;;
        completions) COMPREPLY=( $(compgen -W "bash zsh fish" -- "$cur") ); return ;;
    esac
}}
complete -F _wrvm wrvm
"#,
        cmds = command_names(),
        select = alt(SELECT_CMDS),
        install = alt(INSTALL_CMDS),
        remove = alt(REMOVE_CMDS),
        installed = INSTALLED_CMD,
    )
}

fn zsh() -> String {
    format!(
        r#"#compdef wrvm
# zsh completion for wrvm
(( $+functions[compdef] )) || {{ autoload -Uz compinit && compinit -C }}
_wrvm() {{
    local -a cmds
    cmds=({cmds})
    if (( CURRENT == 2 )); then
        _describe -t commands 'wrvm command' cmds
        return
    fi
    case "${{words[2]}}" in
        {select}) compadd latest ${{(f)"$({installed})"}} ;;
        {install}) compadd latest ;;
        {remove}) compadd ${{(f)"$({installed})"}} ;;
        completions) compadd bash zsh fish ;;
    esac
}}
compdef _wrvm wrvm
"#,
        cmds = command_names(),
        select = alt(SELECT_CMDS),
        install = alt(INSTALL_CMDS),
        remove = alt(REMOVE_CMDS),
        installed = INSTALLED_CMD,
    )
}

fn fish() -> String {
    let mut out = String::from("# fish completion for wrvm\ncomplete -c wrvm -f\n");
    for (cmd, desc) in COMMANDS {
        out.push_str(&format!(
            "complete -c wrvm -n __fish_use_subcommand -a {cmd} -d '{desc}'\n"
        ));
    }
    out.push_str(&format!(
        "complete -c wrvm -n '__fish_seen_subcommand_from {}' -a 'latest'\n",
        [SELECT_CMDS, INSTALL_CMDS].concat().join(" ")
    ));
    out.push_str(&format!(
        "complete -c wrvm -n '__fish_seen_subcommand_from {}' -a '({INSTALLED_CMD})'\n",
        [SELECT_CMDS, REMOVE_CMDS].concat().join(" ")
    ));
    out.push_str(
        "complete -c wrvm -n '__fish_seen_subcommand_from completions' -a 'bash zsh fish'\n",
    );
    out
}
