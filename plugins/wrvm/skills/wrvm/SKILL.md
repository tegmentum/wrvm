---
name: wrvm
description: >-
  How to use wrvm, the WAMR (WebAssembly Micro Runtime) Version Manager, to
  install, pin, switch, run, and manage versioned WAMR runtimes. Use when the
  user wants to install or manage WAMR / iwasm / wamrc versions, pin a project
  to a runtime, run a .wasm module through a managed runtime, set up the wrvm
  shim or shell integration, inspect runtime usage, or update wrvm itself.
---

# Using wrvm

`wrvm` manages [WAMR](https://github.com/bytecodealliance/wasm-micro-runtime)
runtimes the way nvm/rustup manage their toolchains: install multiple versions,
select one per project or per shell, and run modules against the selected one.
Unlike its sibling `wvm` (Wasmtime), wrvm is a **single native binary** — WAMR
mainline lacks WASI Preview 2 / component model / `wasi:http` support, so
self-hosting isn't viable today.

## Install wrvm

```sh
curl -fsSL https://raw.githubusercontent.com/tegmentum/wrvm/main/install.sh | sh
# or:  brew tap tegmentum/wrvm https://github.com/tegmentum/wrvm && brew install wrvm
```

The installer sets up `PATH`, installs shell completions, and wires the shim +
`wrvm use` hook into a sourced env file — so `iwasm`, `wamrc`, `wrvm use`, and
completions work in new shells with no extra steps.

**aarch64 note.** WAMR upstream ships x86_64 assets only. On aarch64 hosts wrvm
resolves runtime downloads from an in-repo mirror channel (releases tagged
`wamr-mirror-<version>` on `tegmentum/wrvm`, overridable with
`WRVM_RUNTIME_MIRROR=owner/repo`). A version is only installable on ARM once its
mirror release exists — trigger the `mirror-wamr` GitHub Actions workflow to
build one.

## Version specs (the core concept)

Every version argument accepts a **spec** — a floating channel or an exact pin.
`default`/`use`/pins store the *spec*, so a floating one keeps tracking its line
and auto-installs a newer match at activation.

| Spec              | Means                | Resolves to        |
| ---               | ---                  | ---                |
| `latest`          | newest overall       | e.g. `2.4.5`       |
| `2` (or `2.x`)    | latest major line    | newest `2.*`       |
| `2.4` (or `2.4.x`)| latest major/minor   | newest `2.4.*`     |
| `2.4.5`           | exact / frozen       | exactly `2.4.5`    |

WAMR does not designate LTS releases; `wrvm install lts` errors with a clear
message.

## Variants

WAMR ships several installables per release; wrvm treats each as a separate
installable pinned via `--variant`:

| Variant           | What it is |
| ---               | --- |
| `iwasm` (default) | Runtime (interpreter + JIT). |
| `iwasm-gc-eh`     | Same runtime, with GC + Exception Handling proposals. |
| `wamrc`           | AOT compiler. |
| `wasi-extensions` | Headers + libs for embedding `libiwasm` (upstream x86_64 only for now — see aarch64 note below). |

Multiple variants of the same version coexist under
`~/.tegmentum/wrvm/runtimes/wamr/versions/<v>/<variant>/`.

## Common tasks

```sh
wrvm list                          # all available versions; installed/default marked
wrvm install 2.4.5                 # install a specific version (default variant: iwasm)
wrvm install 2.4.5 --variant iwasm-gc-eh   # install a specific variant
wrvm install 2.4.5 --variant wamrc         # install the AOT compiler
wrvm default 2                     # persistent default (floats within 2.x)
wrvm use 2.4                       # switch THIS shell only (needs the shell hook)
wrvm deactivate                    # drop the per-shell override
wrvm current                       # print the effective version (resolves the spec)
wrvm path 2.4.5 --variant wamrc    # filesystem path of a variant
wrvm upgrade                       # pull the newest match for the default's floating line
wrvm upgrade --all                 # bump every installed major line
wrvm uninstall 2.4.5               # remove all variants of a version; --force past app deps
wrvm uninstall 2.4.5 --variant wamrc   # remove one variant only
wrvm verify                        # check installed variants against their manifests
```

**Selection order** (pin → session → default → env → PATH): a project pin
wins, then `WRVM_VERSION`/`WRVM_VARIANT` (set by `wrvm use`), then the default,
then `IWASM_HOME`/`WAMR_HOME`, then PATH. `WRVM_VERBOSE=1` prints which runtime
was chosen and why.

Pin a project by creating `wrvm.toml` (searched upward from the cwd):

```toml
[wrvm]
runtime = "2"                # a spec; floats within 2.x
variant = "iwasm-gc-eh"      # optional; defaults to iwasm
```

## Running a module

Two equivalent ways; both honor the selection order and record usage.

```sh
# 1. Transparent — after install, `iwasm` on PATH IS the wrvm shim:
iwasm module.wasm

# 2. Explicit:
wrvm exec -- module.wasm            # runs the resolved iwasm
wrvm exec --wamrc -- module.wasm -o module.aot   # runs wamrc instead
```

The shim resolves the active version, records the run, and execs the real
runtime. An app just calls `iwasm` and needs to know nothing about wrvm.

## Usage tracking

Every run through the shim or `wrvm exec` is recorded (version, variant,
runtime path, module + absolute path + sha256, full argv, app, caller, cwd,
time).

```sh
wrvm usage                    # per-version counts + recent invocations
wrvm usage --limit 50
```

Opt a run out with a leading `--no-usage` flag or `WRVM_NO_USAGE=1`. Set
`WRVM_APP=<name>` in an app's environment for clean attribution. A large-module
hashing warning (interactive only) points at the opt-outs; `WRVM_HASH_WARN_MB`
tunes the threshold.

## App integration (loose coupling)

An application declares the runtimes it was tested against in its own
`wrvm.toml`; it works with **no wrvm installed** and never depends on wrvm at
runtime.

```toml
[app]
name = "my-app"
runtimes = ["2.4.5"]                        # WAMR versions
variant = "iwasm-gc-eh"                      # optional
# runtime-path = "/opt/my-app/bin/iwasm"     # OR a custom runtime it ships
```

Running such an app through the shim / `wrvm exec` **auto-registers** it, so
`wrvm apps` lists it and `wrvm uninstall` refuses to remove a runtime an app
still needs (`--force` overrides). `wrvm register <dir>` / `wrvm unregister
<name>` do it manually.

## Managing wrvm itself

- `wrvm --version` — print the wrvm version.
- `wrvm --upgrade [--check]` — **self-update the wrvm binary** (`--check` only
  reports). Distinct from `wrvm upgrade <spec>`, which updates managed
  *runtimes*.
- `wrvm completions <bash|zsh|fish>` — print a completion script (the installer
  does this automatically).
- `wrvm shell-init` — print the shell integration (PATH + `use` hook) as a
  snippet you can source or paste into your rc.
- `wrvm setup` — wire the shell integration into your login-shell rc for
  you (idempotent). Run this once after `brew install wrvm`; the
  `curl | sh` installer does it automatically.
- `wrvm doctor` — diagnose the install, shell integration, and PATH.

## Gotchas

- **`wrvm --upgrade` (binary) vs `wrvm upgrade <spec>` (runtimes)** — the dash
  matters.
- **aarch64 needs a mirror release.** On ARM hosts, a version is only
  installable once its `wamr-mirror-<ver>` release exists in this repo (or in
  the repo pointed to by `WRVM_RUNTIME_MIRROR`). Not every upstream version has
  been mirrored — trigger the `mirror-wamr` workflow to add one.
- The shim only sees runtimes reached through `PATH`. An app that hardcodes an
  absolute runtime path is invisible to usage tracking — that is what app
  registration is for.
- `wrvm use` can't mutate the parent shell directly; it relies on the hook the
  installer set up (or `wrvm shell-init`). Without it, `use` prints guidance.
- Floating auto-install uses a cached release list (`WRVM_REFRESH_INTERVAL`
  seconds, default 3600; `0` stays offline).
- WAMR upstream sometimes publishes checksums via the release JSON's `digest`
  field, sometimes not. wrvm honors that field first, then falls back to a
  sibling `<name>.sha256` asset in the same release (which the mirror always
  publishes). If neither is present, install prints a warning and proceeds.
