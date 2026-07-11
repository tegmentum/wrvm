# AGENTS.md

Guidance for AI coding agents working in this repo. (`CLAUDE.md` is a symlink to
this file.)

`wrvm` is a version manager for [WAMR](https://github.com/bytecodealliance/wasm-micro-runtime)
(WebAssembly Micro Runtime). It installs, selects, discovers, validates, and
executes versioned WAMR runtimes so that WAMR becomes an implementation detail
rather than a prerequisite.

Unlike its sibling `wvm` (Wasmtime version manager), `wrvm` is **not
self-hosting**: WAMR mainline lacks WASI Preview 2, component-model execution,
`wasi:cli`, and `wasi:http`, so there is no way to run wrvm's own logic as a
wasm component on WAMR. `wrvm` is a single native Rust binary.

## Build

```sh
cargo build --release        # target/release/wrvm
cargo test
cargo clippy -- -D warnings
```

## Verify (run before committing)

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Architecture

Single crate. Each module owns one concern:

| Module          | Role |
| ---             | --- |
| `layout`        | `~/.tegmentum/wrvm/…` filesystem paths |
| `platform`      | Host arch/OS detection; WAMR asset naming |
| `spec`          | `VersionSpec` (latest / major / major.minor / exact) |
| `manifest`      | Per-installed-version `manifest.json` (files + digests) |
| `appmanifest`   | An application's `wrvm.toml` `[app]` section |
| `apps`          | `apps.json` (registered apps) |
| `usage`         | `usage.log` (JSONL, transparent shim tracking) |
| `discovery`     | Runtime resolution: pin → session → default → env → PATH |
| `cache`         | Cached remote release list |
| `http`          | Native HTTP client (ureq) with progress |
| `archive`       | `.tar.gz` extraction |
| `hash`          | Streaming SHA-256 |
| `progress`      | Terminal-aware spinner + progress bar |
| `shell`         | Shell integration snippet (POSIX + fish) |
| `install`       | Release resolution + install + `ensure` |
| `commands`      | CLI command implementations |
| `shim`          | `shims/iwasm` pass-through with usage recording |
| `selfupdate`    | `wrvm --upgrade` (replace self) |
| `doctor`        | `wrvm doctor` diagnostic |
| `completions`   | Static shell-completion script generation |
| `main`          | Busybox-style dispatch (wrvm vs. iwasm shim) |

**Storage is plain files** — no database. Each runtime version + variant is
extracted into `runtimes/wamr/versions/<v>/<variant>/`; `apps.json` holds
registrations and `usage.log` (JSONL) holds observed usage.

## WAMR-specific caveats

- **No ARM64 releases upstream.** WAMR ships x86_64 assets only. wrvm bridges
  this by publishing an aarch64 **mirror channel** in this repo — the
  `mirror-wamr` GitHub Actions workflow builds all four variants (`iwasm`,
  `iwasm-gc-eh`, `wamrc`, `wasi-extensions`) from upstream source and uploads
  them under a `wamr-mirror-<ver>` release tag with `.sha256` sidecars. On
  aarch64 hosts `install` resolves the mirror automatically. Override the
  source repo with `WRVM_RUNTIME_MIRROR=owner/repo`. The `wamrc` build caches
  LLVM (via `actions/cache`, keyed on WAMR version + build-script hash) so
  first-run cost lands only once per version.
- **Checksum verification** first honors the release JSON's `digest` field
  (upstream sometimes fills it), then falls back to a sibling `<name>.sha256`
  asset in the same release (the mirror publishes these; upstream may not).
  When neither is present, install prints a warning and proceeds.
- **No LTS designation.** `VersionSpec::Lts` parses but resolves empty and
  errors with a clear message.
- **Multiple variants per release.** A release ships `iwasm` (plain),
  `iwasm-gc-eh` (GC + Exception Handling), `wamrc` (AOT compiler), and
  `wamr-wasi-extensions` (headers/libs). `wrvm install <ver>` takes an optional
  `--variant`; each variant is tracked as its own installable.
- **Version tags are `WAMR-2.4.5`** on GitHub; the leading `WAMR-` is stripped.
- **Archive shape:** Linux/macOS assets are `.tar.gz`; Windows is `.zip`. v0.1
  supports tar.gz only (Windows deferred, matching wvm's scope).

## Claude Code plugin + skill

This repo is also a Claude Code plugin marketplace (`.claude-plugin/marketplace.json`,
`plugins/wrvm/`). The canonical skill lives at
`plugins/wrvm/skills/wrvm/SKILL.md`; `.claude/skills/wrvm/SKILL.md` is a symlink
pointing at it, so contributors working in this repo get the same skill as
plugin users and the two never drift.

## Conventions

- **Commits:** [Conventional Commits](https://conventionalcommits.org). No
  emojis. Do not reference the assistant/AI in commit messages.
- Match surrounding style; keep comments at the density of the file you're in.
- Commit or push only when asked.
