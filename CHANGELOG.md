# Changelog

## Unreleased

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
  (`cargo build --release`). Re-enable in a follow-up release when queue
  availability recovers.

### aarch64 support via mirror channel
- WAMR upstream ships x86_64 assets only. wrvm bridges this by publishing an
  in-repo mirror release (tag `wamr-mirror-<ver>`) built from upstream source
  by the `mirror-wamr` GitHub Actions workflow. On aarch64 hosts, `install`
  transparently resolves runtime downloads from that mirror.
- Mirrored variants (three of four): `iwasm`, `iwasm-gc-eh`, `wamrc`. The
  workflow caches LLVM across runs so the wamrc build amortizes across
  versions. `wasi-extensions` needs a wasi-sdk toolchain (its cmake sources
  include `<wasi/api.h>` from wasi-libc); tracked as follow-up.
- Install verifies each mirror asset against its `.sha256` sidecar (and
  honors an upstream `digest` field when present).
- `WRVM_RUNTIME_MIRROR=owner/repo` overrides the mirror source.

### Not supported (upstream limitation)
- **LTS designation**: WAMR has no LTS cadence; `wrvm install lts` errors.
- **Self-hosting**: WAMR mainline lacks WASI Preview 2 / component model /
  `wasi:http`, so wrvm cannot run its own logic as a wasm component on WAMR.
  wrvm is a native binary.
