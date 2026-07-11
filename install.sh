#!/bin/sh
# wrvm installer.
#
#   curl -fsSL https://raw.githubusercontent.com/tegmentum/wrvm/main/install.sh | sh
#
# Fetches the `wrvm` binary from the GitHub release, verifies its checksum,
# wires shell integration, and installs completions. wrvm itself downloads and
# manages WAMR runtimes on demand.

set -eu

REPO="${WRVM_REPO:-tegmentum/wrvm}"
WRVM_HOME="${WRVM_HOME:-$HOME/.tegmentum/wrvm}"
BIN_DIR="$WRVM_HOME/bin"

say() { printf '%s\n' "$*"; }
err() { printf 'error: %s\n' "$*" >&2; exit 1; }

wrvm_version() {
    [ -x "$1" ] || return 0
    "$1" --version 2>/dev/null | awk '{print $2}'
}

detect_target() {
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os" in
        Linux) os="linux" ;;
        Darwin) os="macos" ;;
        *) err "unsupported OS: $os" ;;
    esac
    case "$arch" in
        x86_64 | amd64) arch="x86_64" ;;
        arm64 | aarch64) arch="aarch64" ;;
        *) err "unsupported architecture: $arch" ;;
    esac
    printf '%s-%s' "$arch" "$os"
}

verify_checksum() {
    file="$1"
    sumurl="$2"
    expected="$(curl -fsSL "$sumurl" 2>/dev/null | awk '{print $1}')" || return 0
    [ -n "$expected" ] || return 0
    if command -v sha256sum >/dev/null 2>&1; then
        actual="$(sha256sum "$file" | awk '{print $1}')"
    elif command -v shasum >/dev/null 2>&1; then
        actual="$(shasum -a 256 "$file" | awk '{print $1}')"
    else
        say "  (no sha256 tool found; skipping checksum verification)"
        return 0
    fi
    [ "$expected" = "$actual" ] || err "checksum mismatch for $file"
    say "  verified checksum"
}

install_from_release() {
    target="$1"
    asset="wrvm-$target"
    base="https://github.com/$REPO/releases/latest/download"
    say "Fetching $asset ..."
    mkdir -p "$BIN_DIR"
    tmp="$BIN_DIR/.wrvm.download"
    if curl -fL --progress-bar "$base/$asset" -o "$tmp"; then
        verify_checksum "$tmp" "$base/$asset.sha256"
        chmod +x "$tmp"
        mv -f "$tmp" "$BIN_DIR/wrvm"
        return 0
    fi
    rm -f "$tmp"
    return 1
}

install_from_source() {
    command -v cargo >/dev/null 2>&1 || return 1
    say "No prebuilt binary available; building from source ..."
    tmp="$(mktemp -d)"
    git clone --depth 1 "https://github.com/$REPO" "$tmp/wrvm" >/dev/null 2>&1 || return 1
    ( cd "$tmp/wrvm" && cargo build --release ) || return 1
    mkdir -p "$BIN_DIR"
    cp "$tmp/wrvm/target/release/wrvm" "$BIN_DIR/wrvm"
    chmod +x "$BIN_DIR/wrvm"
    rm -rf "$tmp"
    return 0
}

write_env_file() {
    cat > "$WRVM_HOME/env" <<EOF
#!/bin/sh
# wrvm shell setup. Prepends the wrvm bin directory to PATH.
case ":\${PATH}:" in
    *:"$BIN_DIR":*) ;;
    *) export PATH="$BIN_DIR:\$PATH" ;;
esac
EOF
    if [ -x "$BIN_DIR/wrvm" ]; then
        "$BIN_DIR/wrvm" shell-init >> "$WRVM_HOME/env" 2>/dev/null || true
    fi
    cat > "$WRVM_HOME/env.fish" <<EOF
# wrvm shell setup. Prepends the wrvm bin and shim directories to PATH.
if not contains "$BIN_DIR" \$PATH
    set -gx PATH "$BIN_DIR" \$PATH
end
if not contains "$WRVM_HOME/shims" \$PATH
    set -gx PATH "$WRVM_HOME/shims" \$PATH
end
EOF
}

WRVM_MARKER="# wrvm-managed"

wire_rc() {
    body="$1"
    file="$2"
    tag="$3"
    verb="${4:-updated}"
    marker="$WRVM_MARKER:$tag"
    new_line="$body $marker"
    [ -e "$file" ] || { mkdir -p "$(dirname "$file")" && : > "$file"; }
    if grep -qxF -- "$new_line" "$file" 2>/dev/null; then
        return 0
    fi
    tmp="$file.wrvm.$$"
    grep -vF -- "$marker" "$file" > "$tmp" 2>/dev/null || : > "$tmp"
    printf '%s\n' "$new_line" >> "$tmp"
    mv "$tmp" "$file"
    say "  $verb $file"
    CONFIG_CHANGED=1
    return 0
}

configure_shell() {
    write_env_file
    shell_name="$(basename "${SHELL:-}")"
    posix_line=". \"$WRVM_HOME/env\""
    SOURCE_CMD=""
    CONFIG_CHANGED=0
    case "$shell_name" in
        bash)
            for rc in "$HOME/.bashrc" "$HOME/.bash_profile" "$HOME/.profile"; do
                [ -e "$rc" ] && wire_rc "$posix_line" "$rc" env
            done
            [ -e "$HOME/.bashrc" ] || wire_rc "$posix_line" "$HOME/.bashrc" env created
            SOURCE_CMD="source \"$WRVM_HOME/env\""
            ;;
        zsh)
            zdir="${ZDOTDIR:-$HOME}"
            wire_rc "$posix_line" "$zdir/.zshrc" env
            SOURCE_CMD="source \"$WRVM_HOME/env\""
            ;;
        fish)
            confd="${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d"
            mkdir -p "$confd"
            wire_rc "source \"$WRVM_HOME/env.fish\"" "$confd/wrvm.fish" env
            SOURCE_CMD="source \"$WRVM_HOME/env.fish\""
            ;;
        *)
            wire_rc "$posix_line" "$HOME/.profile" env
            SOURCE_CMD=". \"$WRVM_HOME/env\""
            ;;
    esac
    [ "$CONFIG_CHANGED" -eq 0 ] && say "  shell already configured for wrvm"
    return 0
}

install_completions() {
    wrvm_bin="$BIN_DIR/wrvm"
    [ -x "$wrvm_bin" ] || return 0
    comp_dir="$WRVM_HOME/completions"
    mkdir -p "$comp_dir"
    case "$shell_name" in
        bash)
            "$wrvm_bin" completions bash > "$comp_dir/wrvm.bash" 2>/dev/null || return 0
            wire_rc "source \"$comp_dir/wrvm.bash\"" "$HOME/.bashrc" completions
            ;;
        zsh)
            "$wrvm_bin" completions zsh > "$comp_dir/_wrvm" 2>/dev/null || return 0
            wire_rc "source \"$comp_dir/_wrvm\"" "${ZDOTDIR:-$HOME}/.zshrc" completions
            ;;
        fish)
            fdir="${XDG_CONFIG_HOME:-$HOME/.config}/fish/completions"
            mkdir -p "$fdir"
            "$wrvm_bin" completions fish > "$fdir/wrvm.fish" 2>/dev/null || return 0
            say "  installed fish completions to $fdir/wrvm.fish"
            ;;
    esac
}

main() {
    command -v curl >/dev/null 2>&1 || err "curl is required"
    target="$(detect_target)"

    prev_version="$(wrvm_version "$BIN_DIR/wrvm")"
    if [ -n "$prev_version" ]; then
        say "Found wrvm $prev_version in $BIN_DIR; fetching the latest release ..."
    else
        say "Installing wrvm for $target into $BIN_DIR"
    fi

    if ! install_from_release "$target"; then
        install_from_source || err "could not install a prebuilt binary or build from source"
    fi

    new_version="$(wrvm_version "$BIN_DIR/wrvm")"
    say ""
    if [ -z "$prev_version" ]; then
        say "wrvm ${new_version:+$new_version }installed to $BIN_DIR/wrvm"
    elif [ "$prev_version" = "$new_version" ]; then
        say "wrvm $new_version reinstalled (already up to date)"
    else
        say "wrvm upgraded $prev_version -> ${new_version:-unknown}"
    fi

    say "Configuring your shell ..."
    configure_shell
    install_completions
    case ":$PATH:" in
        *":$BIN_DIR:"*)
            say "  wrvm is already on your PATH"
            ;;
        *)
            say ""
            say "To start using wrvm, restart your shell or run:"
            say "    $SOURCE_CMD"
            ;;
    esac

    # WAMR upstream ships x86_64 only. On ARM, wrvm falls back to the mirror
    # channel in this repo (tagged wamr-mirror-<ver>); a version is only
    # installable once its mirror release exists.
    host_arch="$(uname -m)"
    case "$host_arch" in
        arm64 | aarch64)
            say ""
            say "NOTE: WAMR upstream publishes x86_64 binaries only. On this host ($host_arch)"
            say "      wrvm resolves runtime downloads from the aarch64 mirror channel"
            say "      published in the wrvm repo (tag: wamr-mirror-<version>). Not every"
            say "      upstream version has a mirror release yet; see \`wrvm doctor\`."
            ;;
    esac

    say ""
    say "Next:"
    say "    wrvm install latest    # download and install a WAMR runtime"
    say "    wrvm default latest"
    say "    wrvm exec -- --help"
}

main "$@"
