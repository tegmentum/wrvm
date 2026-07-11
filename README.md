# wrvm: the WAMR Version Manager

`wrvm` is a lightweight version manager for
[WAMR](https://github.com/bytecodealliance/wasm-micro-runtime) — WebAssembly
Micro Runtime, the Bytecode Alliance's small-footprint runtime. It installs,
selects, discovers, validates, and executes versioned WAMR runtimes so that
WAMR becomes an implementation detail rather than a prerequisite.

Sibling project: [`wvm`](https://github.com/tegmentum/wvm), the same idea for
Wasmtime.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/tegmentum/wrvm/main/install.sh | sh
```

Or with Homebrew:

```sh
brew tap tegmentum/wrvm https://github.com/tegmentum/wrvm
brew install wrvm
```

Supported on macOS and Linux (x86_64 + aarch64). WAMR upstream ships x86_64
binaries only; on aarch64 hosts wrvm resolves runtime downloads from a mirror
channel in this repo (see [aarch64 mirror](#aarch64-mirror) below). Use
`wrvm doctor` to see host support.

## Quickstart

```sh
wrvm list                # available versions (installed ones marked)
wrvm install latest      # download + verify + install
wrvm default latest      # default runtime for new shells
wrvm exec -- --help      # run the selected iwasm
```

## Variants

WAMR ships several installable artifacts per release; `wrvm` treats each as a
separate installable pinned via `--variant`:

| Variant           | What it is |
| ---               | --- |
| `iwasm` (default) | The `iwasm` runtime (interpreter + JIT). |
| `iwasm-gc-eh`     | Same runtime, built with GC + Exception Handling proposals. |
| `wamrc`           | The AOT compiler (`wamrc`). |
| `wasi-extensions` | Headers and libraries for embedding (`libiwasm`). |

```sh
wrvm install 2.4.5 --variant iwasm-gc-eh
wrvm install 2.4.5 --variant wamrc
wrvm path 2.4.5 --variant wamrc
```

Multiple variants of the same version coexist under
`runtimes/wamr/versions/<v>/<variant>/`.

## Version specifiers

`install`, `default`, `use`, `path`, and project pins accept a **spec**:

| Spec        | Locks to             | Resolves to        |
| ---         | ---                  | ---                |
| `latest`    | newest overall       | e.g. `2.4.5`       |
| `2` / `2.x` | latest major line    | newest `2.*`       |
| `2.4`       | latest major/minor   | newest `2.4.*`     |
| `2.4.5`     | exact                | exactly `2.4.5`    |

`default`/`use` store the **spec**, so a floating default keeps tracking newer
matches. WAMR does not designate LTS releases; `lts` errors at resolution.

## Default vs. per-shell version

- `wrvm default <version>` sets the persistent default used by new shells.
- `wrvm use <version>` switches the runtime for the current shell only via the
  `WRVM_VERSION` environment variable.

Because `wrvm` is a binary it can't change its parent shell directly; the
`curl | sh` installer wires the shim + `use` hook for you. Manually:

```sh
wrvm shell-init >> ~/.zshrc
```

## Runtime discovery

`wrvm exec` and the shim resolve in this order:

1. **Project pin** — nearest `wrvm.toml` walking up from the working directory:
   ```toml
   [wrvm]
   runtime = "2"
   variant = "iwasm-gc-eh"   # optional
   ```
2. **Session** — `WRVM_VERSION` / `WRVM_VARIANT`, set per shell by `wrvm use`.
3. **Default** — persistent default set by `wrvm default`.
4. **Environment override** — `IWASM_HOME` or `WAMR_HOME`.
5. **System / PATH** — an `iwasm` already on `PATH`.

Set `WRVM_VERBOSE=1` to print which runtime was selected.

## Application registration

Apps can declare which WAMR runtime(s) they need:

```toml
[app]
name = "tegmentum-foo"
runtimes = ["2.4.5"]
variant = "iwasm-gc-eh"
# runtime-path = "/opt/foo/bin/iwasm"   # OR bring your own
```

```sh
wrvm register ./my-app
wrvm apps
```

Registration is advisory: `wrvm uninstall` refuses to remove a runtime a
registered app still depends on (unless `--force`).

## Transparent usage tracking

`wrvm shell-init` puts `shims/` on `PATH`; `shims/iwasm` and `shims/wamrc` are
`wrvm` under another name. An app that calls `iwasm` routes through wrvm,
which:

1. resolves the active version,
2. appends a JSON line to `usage.log` (version, binary path, argv, module + its
   sha256, `WRVM_APP`, caller, cwd, time),
3. execs the real runtime.

```sh
wrvm usage             # per-version rollups + recent invocations
```

Opt out with `WRVM_NO_USAGE=1` or a leading `--no-usage`.

## aarch64 mirror

WAMR upstream (`bytecodealliance/wasm-micro-runtime`) publishes x86_64 assets
only. On aarch64 hosts, wrvm looks up a **side-channel release** in this repo
tagged `wamr-mirror-<version>`, which carries `iwasm` and `iwasm-gc-eh`
compiled from upstream source at the matching `WAMR-<version>` tag. Building
and publishing that release is driven by the `mirror-wamr` GitHub Actions
workflow, triggered manually with the upstream version as input.

The workflow builds all four variants (`iwasm`, `iwasm-gc-eh`, `wamrc`,
`wasi-extensions`) with `.sha256` sidecars that wrvm verifies on install.
The `wamrc` build caches LLVM across runs, so the ~30-minute LLVM step lands
only on the first version bump. The mirror is opt-in per version: if
`wamr-mirror-<version>` does not exist yet, `wrvm install <version>` on
aarch64 prints a clear error pointing at the workflow.
`WRVM_RUNTIME_MIRROR=owner/repo` overrides the source repo.

## Storage layout

Under `~/.tegmentum/wrvm/` (override with `WRVM_HOME`):

```text
~/.tegmentum/wrvm/
  bin/wrvm                                # installer target
  runtimes/wamr/versions/2.4.5/
    iwasm/{bin/iwasm, manifest.json, LICENSE, ...}
    iwasm-gc-eh/...
    wamrc/bin/wamrc
    wasi-extensions/...
  runtimes/wamr/default                   # spec, e.g. "latest" or "2"
  shims/{iwasm, wamrc}                    # links to the wrvm binary
  apps.json                               # app registrations
  usage.log                               # observed invocations (JSONL)
  cache/releases.json                     # cached remote release list
  downloads/
```

## Environment variables

| Variable | Purpose |
| --- | --- |
| `WRVM_HOME` | wrvm root (default `~/.tegmentum/wrvm`). |
| `WRVM_VERSION` | Per-shell version override (set by `wrvm use`). |
| `WRVM_VARIANT` | Per-shell variant override. |
| `WRVM_VERBOSE` | `1` prints which runtime was selected. |
| `WRVM_REFRESH_INTERVAL` | Seconds to cache the remote release list (default `3600`; `0` stays offline). |
| `WRVM_STALE_DAYS` | Days before `wrvm list` flags a runtime as unused (default `90`). |
| `WRVM_APP` | Application name recorded in usage for the current process. |
| `WRVM_NO_USAGE` | `1` skips usage recording. |
| `WRVM_HASH_WARN_MB` | Module size (MiB) above which hashing warns (default `100`; `0` disables). |
| `IWASM_HOME`, `WAMR_HOME` | External runtime location used as discovery fallback. |
| `WRVM_RUNTIME_MIRROR` | Repo hosting the aarch64 WAMR mirror (default `tegmentum/wrvm`). |
| `HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY` | Proxy for downloads. |

## Uninstalling

```sh
rm -rf ~/.tegmentum/wrvm
grep -v '# wrvm-managed' ~/.zshrc > ~/.zshrc.tmp && mv ~/.zshrc.tmp ~/.zshrc
```

## Build from source

```sh
cargo build --release       # target/release/wrvm
cargo test
cargo clippy --all-targets -- -D warnings
```

## Claude Code plugin

This repo doubles as a Claude Code plugin marketplace, so wrvm usage guidance
is available in **any** project (not just a clone of this repo):

```
/plugin marketplace add tegmentum/wrvm
/plugin install wrvm@tegmentum
```

The plugin ships a skill that teaches Claude how to install, pin, switch, run,
and manage runtimes with wrvm. Contributors working inside this repo get the
same skill automatically via `.claude/skills/wrvm/` (a symlink to the plugin's
canonical copy, so the two never drift).

## License

Apache-2.0.
