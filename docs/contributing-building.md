# Building

Everything needed to build `agent` and the eBPF object it embeds. If a build fails, this is the first
page to check.

## Prerequisites

### The Rust toolchains (note: plural)

This repo deliberately uses **two** toolchains, split by directory — you don't pick one, `rustup`
honors both automatically:

- The **workspace root** pins **stable** (`rust-toolchain.toml`) — `agent`, `common`, and `xtask`
  build here, under the full `deny`-unwrap/expect/panic lint gate.
- **`crates/ebpf`** pins **nightly** with `rust-src` (`crates/ebpf/rust-toolchain.toml`) — the BPF
  target (`bpfel-unknown-none`) still requires nightly's `-Z build-std=core`.

[Install `rustup`](https://www.rust-lang.org/tools/install); it reads the pin files. Confirm the
split:

```console
rustup show active-toolchain                 # in the repo root → stable
cd crates/ebpf && rustup show active-toolchain   # → nightly
```

Why this exists and why you must not "simplify" it away:
[ADR-0003](./adr/0003-stable-root-nightly-ebpf-toolchain-split.md).

### `bpf-linker`

The eBPF object is linked by `bpf-linker`:

```console
cargo install bpf-linker
```

### Platform requirements

The agent **loads** on Linux only; the host build of the eBPF crate needs Linux too. To actually load
and run (tier 3+ testing), the kernel must have:

- **kernel ≥ 5.8** (ring buffer — [ADR-0001](./adr/0001-ring-buffer-over-perf-buffer.md)),
- **BTF** (`CONFIG_DEBUG_INFO_BTF=y`, i.e. `/sys/kernel/btf/vmlinux` present — for CO-RE,
  [ADR-0002](./adr/0002-co-re-btf-over-compile-per-kernel.md)),
- **cgroup v2** (`/sys/fs/cgroup/cgroup.controllers` present).

You can build (and run the host-only tests) on macOS, but loading eBPF requires a Linux box. The full
per-feature matrix (including M6's BPF-LSM needs) lives in [support-matrix.md](./support-matrix.md);
the agent's [boot preflight](../crates/agent/src/preflight.rs) checks these at startup.

### Distro packages

You only need these for **`cargo xtask codegen`** (regenerating the kernel bindings) — normal builds
don't:

```console
# Arch
sudo pacman -S bpf                                   # provides bpftool; clang/libclang for bindgen
cargo install bindgen-cli
cargo install --git https://github.com/aya-rs/aya -- aya-tool

# Debian/Ubuntu — bpftool packaging varies by release (try `bpftool` first, else the linux-tools pkgs)
sudo apt install libclang-dev
sudo apt install bpftool || sudo apt install linux-tools-common "linux-tools-$(uname -r)"
cargo install bindgen-cli
cargo install --git https://github.com/aya-rs/aya -- aya-tool
```

## Building

The canonical entrypoint cross-compiles the eBPF crate under nightly, then embeds the resulting object
into the agent via `include_bytes_aligned!`:

```console
cargo xtask build              # debug
cargo xtask build --release    # optimized
```

> ⚠️ **Do not run `cargo build --workspace` / `--all`.** The `ebpf` crate is excluded from
> `default-members` and only compiles for the BPF target — a host build of it fails. A bare
> `cargo build` (default-members) or `cargo build -p <crate>` is fine; the eBPF crate is built for you
> by `build.rs`/`xtask`.

To build a single userspace crate directly:

```console
cargo build -p agent
cargo build -p agent-common
```

## Running

Loading eBPF needs `CAP_BPF`/`CAP_PERFMON`, so `xtask run` uses `sudo`:

```console
cargo xtask run -- --once      # load, attach, then exit (the M0 smoke test)
cargo xtask run                # load and stay attached until Ctrl-C
```

This is **tier 3** testing — see [Testing](./contributing-testing.md).

## Regenerating kernel bindings (CO-RE)

`crates/ebpf/src/vmlinux.rs` holds the Rust definitions of the kernel structs the probes read. It's
**committed** (so normal builds need neither BTF nor `bpftool`) and regenerated only via:

```console
cargo xtask codegen            # needs bpftool + aya-tool (see prerequisites above)
```

CO-RE relocates field offsets at load time, so a file generated on one kernel stays portable across
others ([ADR-0002](./adr/0002-co-re-btf-over-compile-per-kernel.md)). Never hand-edit the generated
file, and never hand-roll kernel structs.

## Where to next

- [Testing](./contributing-testing.md) — what to run before you push.
- [Architecture](./contributing-architecture.md) — what the crates are and how they fit.
