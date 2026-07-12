# wrvm design

`wrvm` is a version manager for [WAMR](https://github.com/bytecodealliance/wasm-micro-runtime).
It is modelled on [`wvm`](https://github.com/tegmentum/wvm) (Wasmtime version
manager) but diverges in one substantial way: **`wrvm` is not self-hosting**.

## Why not self-hosting

`wvm` is a native bootstrapper that runs its own application as a
`wasm32-wasip2` component on a seed Wasmtime. The component exports
`wasi:cli/run@0.2.6` and imports `wasi:http/outgoing-handler@0.2.x`. This is
viable because Wasmtime has strong WASI Preview 2 and component-model support.

For WAMR, the same trick does not work today:

- **No component model in mainline.** Tracking issue
  [bytecodealliance/wasm-micro-runtime#2126](https://github.com/bytecodealliance/wasm-micro-runtime/issues/2126)
  has been open since 2023. Work is happening on the `dev/cm_wasip2` branch but
  is not merged and does not appear in a tagged release.
- **No `wasi:cli/run` entry point.** `iwasm` executes core wasm and AOT files
  only; a `wasip2` cdylib fails to load.
- **No `wasi:http` host implementation.** Downloads via `waki`/`wasi:http`
  would fail at link time. WAMR ships only its own socket-API extension.
- **No aarch64 binaries.** Even if the wasm layer worked, there is nothing to
  seed on Apple Silicon or Linux ARM.

We revisit self-hosting when `dev/cm_wasip2` lands in a tagged release with
`wasi:http` support and aarch64 assets. Until then, `wrvm` is a single native
Rust binary.

## Architecture

Single crate, one concern per module:

| Module        | Role |
| ---           | --- |
| `layout`      | Filesystem layout under `~/.tegmentum/wrvm/`. |
| `platform`    | Host arch/OS detection; WAMR asset matching by prefix + host pattern (WAMR OS-runner tokens vary release to release). |
| `spec`        | `VersionSpec`: `latest` \| `Major` \| `MajorMinor` \| `Exact` (and `Lts` as an unsupported placeholder). |
| `manifest`    | Per-installed-variant `manifest.json` (paths + digests + modes). |
| `appmanifest` | An application's `wrvm.toml` `[app]` section. |
| `apps`        | `apps.json` (registered applications). |
| `usage`       | `usage.log` JSONL (transparent invocations). |
| `discovery`   | Pin → session → default → env → PATH resolution; installed-version listing. |
| `cache`       | Remote release-list cache (`WRVM_REFRESH_INTERVAL`). |
| `http`        | Native HTTP client (ureq). |
| `archive`     | `.tar.gz` extraction with top-level stripping. |
| `hash`        | Streaming SHA-256. |
| `progress`    | Terminal-aware progress bar + spinner. |
| `shell`       | Shell integration snippet. |
| `install`     | Release resolution + install + `ensure`. |
| `commands`    | CLI command implementations. |
| `shim`        | `shims/{iwasm,wamrc}` pass-through with usage tracking. |
| `selfupdate`  | `wrvm --upgrade` (replace self). |
| `doctor`      | `wrvm doctor` diagnostic. |
| `completions` | Static shell-completion script generation. |
| `main`        | Busybox-style dispatch (wrvm vs. iwasm/wamrc shim). |

## Storage

Plain files. No database. Each `(version, variant)` install is a plain directory
under `runtimes/wamr/versions/<v>/<variant>/`. `apps.json` tracks
registrations; `usage.log` is JSONL and compacted on read.

## Variants

WAMR releases include multiple installables that are meaningful independently:

- `iwasm` — the runtime, no GC/EH extensions.
- `iwasm-gc-eh` — the runtime with GC + Exception Handling.
- `wamrc` — the AOT compiler.
- `wasi-extensions` — headers/libs for embedding (`libiwasm`).

Each variant is a first-class installable in `wrvm`. A `wrvm.toml` may pin a
specific variant; `wrvm use`, `wrvm install`, and the shim all honor
`--variant` / the `WRVM_VARIANT` env var.

## Asset matching

WAMR asset names contain a **runner-specific OS token** — e.g.
`iwasm-2.4.5-x86_64-macos-15-intel.tar.gz` — that changes with GitHub Actions
runner revisions. Rather than compute exact names, `platform.rs` returns a
prefix `<variant>-<version>-<arch>-` plus a list of substrings that should
appear in the remainder (`macos-`, `ubuntu-`, `linux`, …). `install::install`
picks the first release asset that matches. This is resilient to runner-token
changes across releases.

## Upstream gaps we accept

- **aarch64 via in-repo mirror.** WAMR upstream ships x86_64 assets only. We
  publish an aarch64 side-channel release in this repo under the tag
  `wamr-mirror-<ver>`, built by `.github/workflows/mirror-wamr.yml` from
  upstream source at the matching `WAMR-<ver>` tag. `platform.rs` sets
  `needs_mirror = true` on ARM; `install.rs` prefers the upstream asset and
  falls back to the mirror release. `WRVM_RUNTIME_MIRROR=owner/repo`
  overrides the source repo. Three of four variants are mirrored (`iwasm`,
  `iwasm-gc-eh`, `wamrc`); the wamrc job caches LLVM via `actions/cache`
  keyed on WAMR version + `build_llvm.sh` hash. `wasi-extensions` isn't
  mirrored yet — its cmake target compiles C sources that include
  `<wasi/api.h>` from wasi-libc, which requires a wasi-sdk toolchain rather
  than the native host compiler.
- **Checksums.** `install::expected_sha256` prefers the release JSON's
  `digest` field, then falls back to a sibling `<name>.sha256` asset in the
  same release. Mirror releases always publish sidecars; upstream may not.
  When neither is available, install proceeds with a warning.
- **No LTS.** `VersionSpec::Lts` parses so the CLI is uniform, but
  `matches()` always returns false. Resolution errors with a clear message.
- **No SHA256SUMS file.** WAMR shows SHA-256 in the release UI but does not
  publish it as a separate asset. When an asset carries a `digest` field on the
  release JSON, we honor it; otherwise we log a warning and continue.
- **Windows deferred.** `.zip` extraction and Windows shim/install support are
  not implemented in v0.1.
