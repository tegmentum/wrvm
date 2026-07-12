# Changelog

## Unreleased

## 0.1.4 — 2026-07-12

### Added
- **Windows (x86_64) prebuilt binary** (`wrvm-x86_64-windows.exe`).
  Release matrix builds it on `windows-latest` with a PowerShell
  `Get-FileHash` step for the `.sha256` sidecar. On Windows the shim
  strategy is a **copy** of the wrvm exe to `shims\iwasm.exe` /
  `shims\wamrc.exe` (Windows symlinks need Developer Mode or admin);
  argv[0] dispatch handles the rest. `.zip` archives are now
  extracted end-to-end. CI matrix gains `windows-latest`. Not yet
  covered: usage-log caller detection (Linux `/proc` and macOS `ps`
  only) and `wrvm setup` for cmd.exe / PowerShell rc files — the
  shim + shims-on-PATH is enough for scripted use.
- **`wasi-extensions` installable on x86_64 hosts from upstream.** WAMR
  ships one arch-less `wamr-wasi-extensions-<ver>.zip` per release; wrvm
  now recognizes that shape (`Platform::matches_wasi_extensions_asset`)
  and dispatches to a new `archive::extract_zip` when the picked asset
  has a `.zip` extension. Zero effect on the aarch64 mirror path, which
  already ships a per-runner tarball.

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
- `shim::exec_or_run` on non-unix platforms was missing the
  `anyhow::Context` import — a latent bug that only compiled through on
  Windows once CI actually built `windows-latest`. Fixed.

### Changed
- Test coverage grew from 5 (spec parsing only) to 49 by covering the
  file-touching modules directly.
- Homebrew formula now `chmod +x`'s the release asset before
  `bin.install` so the completion-generation step can execute the
  freshly installed binary.

### Infrastructure
- `mirror-wamr-sync.yml` scheduled workflow (Mon+Thu 06:00 UTC + manual
  dispatch) auto-mirrors new upstream WAMR releases: lists
  `bytecodealliance/wasm-micro-runtime` tags, filters to real
  `MAJOR.MINOR.PATCH` shape, and dispatches `mirror-wamr.yml` for any
  version missing a `wamr-mirror-<ver>` release on this repo.
  Rate-limited to 3/run, oldest-first.
- `release.yml` gains an `update-formula` job that auto-patches
  `Formula/wrvm.rb` with real `.sha256` values after each release
  (replaces the manual sed dance done at each of v0.1.0–v0.1.3).
- `ci.yml` runs a smoke test on the built binary (`--version`,
  `--help`, `completions`, `doctor`) so runtime regressions surface
  before they ship.

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
