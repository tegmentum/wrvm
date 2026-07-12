# Changelog

## Unreleased

### Fixed
- `wrvm --upgrade` on a Homebrew-installed binary no longer fails with a
  confusing `EACCES`/`EPERM` when trying to atomically replace the
  brew-owned Cellar file. Selfupdate now canonicalizes the running
  executable, detects paths under `/opt/homebrew/Cellar/wrvm/`,
  `/usr/local/Cellar/wrvm/`, or `/home/linuxbrew/.linuxbrew/Cellar/wrvm/`,
  and prints a friendly notice pointing at `brew upgrade wrvm`
  (or `brew update && brew upgrade wrvm` if the formula index is stale).
  The background "newer version available" notifier is also suppressed
  in this case, since the advertised command wouldn't work.

## 0.1.3 — 2026-07-12

### Fixed
- `wrvm doctor` no longer reports "hook not found" when the rc uses the
  `eval "$(wrvm shell-init)"` line that `wrvm setup` writes. It now
  accepts the shims dir, `wrvm shell-init`, or `# wrvm-managed:env` as
  evidence that shell integration is wired.
- `wrvm doctor` no longer lists the brew-linked `wrvm` binary as an
  external `iwasm` — canonicalizing the `iwasm` shim can resolve outside
  `WRVM_HOME` (e.g. into `/opt/homebrew/bin/`) even though the target
  identifies as wrvm. `detect_external` now skips binaries whose
  `--version` starts with `wrvm `.

## 0.1.2 — 2026-07-12

### Added
- **`wrvm setup`** — one-shot command that wires the shell integration into
  the user's login-shell rc file (idempotent via a `# wrvm-managed:env`
  tag). Complements `wrvm shell-init` (which just prints the snippet). The
  Homebrew caveat now points at `wrvm setup` because Homebrew sandboxes
  formula install steps and can't safely modify `$HOME`.
- Homebrew formula now installs a stable snippet at
  `#{prefix}/share/wrvm/wrvm.{sh,fish}` so users who prefer `source`
  over `wrvm setup` have a fixed path to reference.

## 0.1.1 — 2026-07-12

### Added
- **Intel macOS prebuilt binary** (`wrvm-x86_64-macos`), cross-compiled from
  Apple Silicon (`macos-14`) to sidestep the macos-13 (Intel) runner queue.
- **`wasi-extensions` in the aarch64 mirror**, built with wasi-sdk in
  `mirror-wamr.yml`. Existing mirror releases are unaffected; re-run the
  workflow for a version to add the variant.

### Fixed
- Homebrew formula chmods the release asset to `0755` before install so
  Homebrew's post-install completion generation can execute the binary.

## 0.1.0 — 2026-07-12

### Added
- Initial release. Pure-native single-binary version manager for
  [WAMR](https://github.com/bytecodealliance/wasm-micro-runtime).
- Commands: `install`, `list`, `current`, `path`, `default`, `use`, `upgrade`,
  `deactivate`, `shell-init`, `register`, `unregister`, `apps`, `usage`,
  `uninstall`, `verify`, `exec`, `completions`, `doctor`, `--upgrade`.
- Variants: `iwasm` (default), `iwasm-gc-eh`, `wamrc`, `wasi-extensions`.
- Pass-through shims (`shims/iwasm`, `shims/wamrc`) with usage tracking to
  `usage.log`.
- Storage layout under `~/.tegmentum/wrvm/`; runtime versions extracted
  directly into `runtimes/wamr/versions/<v>/<variant>/`.
- `install.sh` + Homebrew formula stub + GitHub Actions CI/release workflows.

### Not shipped as a prebuilt binary in v0.1.0
- **Intel macOS (x86_64-apple-darwin)**: GitHub Actions `macos-13` runners are
  consistently backlogged. Intel macOS users install via source
  (`cargo build --release`). Added via cross-compilation in Unreleased.

### aarch64 support via mirror channel
- WAMR upstream ships x86_64 assets only. wrvm bridges this by publishing an
  in-repo mirror release (tag `wamr-mirror-<ver>`) built from upstream source
  by the `mirror-wamr` GitHub Actions workflow. On aarch64 hosts, `install`
  transparently resolves runtime downloads from that mirror.
- Mirrored variants (three of four): `iwasm`, `iwasm-gc-eh`, `wamrc`. The
  workflow caches LLVM across runs so the wamrc build amortizes across
  versions. `wasi-extensions` added in Unreleased (needs wasi-sdk).
- Install verifies each mirror asset against its `.sha256` sidecar (and
  honors an upstream `digest` field when present).
- `WRVM_RUNTIME_MIRROR=owner/repo` overrides the mirror source.

### Not supported (upstream limitation)
- **LTS designation**: WAMR has no LTS cadence; `wrvm install lts` errors.
- **Self-hosting**: WAMR mainline lacks WASI Preview 2 / component model /
  `wasi:http`, so wrvm cannot run its own logic as a wasm component on WAMR.
  wrvm is a native binary.
