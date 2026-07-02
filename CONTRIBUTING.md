# Contributing

Thanks for your interest — this is an open-source, native **GPU & AI-inference monitor** in Rust: a
single binary with **two first-class frontends**, a GPU-accelerated **GUI** (`egui` on `wgpu`) and a
terminal **CLI/TUI** (`ratatui`), sitting on a headless data engine that also exports to
Prometheus/OTLP.

> Read [**`.rules`**](./.rules) first — the single source of truth for build commands and the
> invariants that must never be traded away (`CLAUDE.md`, `AGENTS.md`, and `GEMINI.md` all point
> there). The design is in [**`ARCHITECTURE.md`**](./ARCHITECTURE.md); the staged plan in
> [**`ROADMAP.md`**](./ROADMAP.md).

## Prerequisites

- **Rust, stable** ([install `rustup`](https://www.rust-lang.org/tools/install)). No nightly, no
  cross-compile, no `sudo`, no codegen step.
- For **real data**: an NVIDIA driver / NVML on the host (no root needed). For **no GPU at all**: the
  built-in **mock** source (`--mock`) — every command builds, runs, tests, and demos without a GPU.
  *(GPU in a Windows box? See [Real NVIDIA hardware from Windows (WSL2)](#real-nvidia-hardware-from-windows-wsl2).)*
- For the **GUI**, `wgpu` needs a working GPU *or* a software fallback; the `top` TUI and `ps` need
  neither and run anywhere, including over SSH.

## Quick start

```console
git clone <repo> && cd agent
cargo build

# Run a frontend (`-p agent` selects the app; the workspace also has the `xtask` binary):
cargo run -p agent -- gui       # the GPU-accelerated window (the default subcommand)
cargo run -p agent -- top       # the live terminal TUI (great over SSH)
cargo run -p agent -- ps        # one-shot snapshot table — works today against the mock source
cargo run -p agent -- ps --json # the same, as structured JSON for scripts

# No GPU? Drive any of the above from the mock source:
cargo run -p agent -- top --mock   # or: AGENT_SOURCE=mock cargo run -p agent -- top
```

A bare `cargo build` builds the whole workspace — nothing is cross-compiled or embedded. Build one
crate with `cargo build -p <crate>` (e.g. `-p agent-core`).

## Real NVIDIA hardware from Windows (WSL2)

The dev/CI target is **Linux** and most work runs headless on the **mock**, but exercising the real
**NVML** path needs an NVIDIA GPU. If yours is in a Windows machine, **WSL2** is the cleanest way to reach
it: you develop on Linux (the first-class target) while the card stays visible through the Windows driver.
(Native Windows compiles too, but it's unvalidated — Windows is post-1.0; WSL2 is the recommended path.)

> **One rule:** the GPU stack comes from the **Windows** NVIDIA driver — **never install an NVIDIA driver
> *inside* WSL.** Doing so breaks the libraries WSL mounts from the host. Just keep the Windows driver current.

```console
# 1. Windows PowerShell (admin): install WSL2 + Ubuntu. Make sure a recent NVIDIA Windows driver is installed.
wsl --install -d Ubuntu

# 2. Inside Ubuntu — Rust toolchain + the system libs the GUI (wgpu/winit) needs to build.
sudo apt update && sudo apt install -y \
  build-essential pkg-config curl git jq \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # rustup, stable

# 3. Confirm the card is visible — provided by the Windows driver, no install needed.
nvidia-smi

# 4. Build and run against the REAL GPU. The default source is NVML, so no --mock.
cargo run -p agent -- ps
cargo run -p agent -- ps --json | jq .     # cross-check the numbers against nvidia-smi
```

- **If NVML init fails:** the driver mounts `libnvidia-ml.so.1` under `/usr/lib/wsl/lib` (normally already
  on the loader path); prepend `LD_LIBRARY_PATH=/usr/lib/wsl/lib` if `ps` can't find it.
- **Not every counter:** WSL2's NVML exposes utilization and memory but may report some metrics as
  unsupported — those render as a clear signal state, never a crash (by design).
- **The GUI (`agent gui`):** needs **WSLg** (built into Windows 11); `sudo apt install mesa-vulkan-drivers`
  if the window won't open. If `wgpu` still won't start, smoke the GUI natively on Windows (or stick to
  `ps`/`top`) — the headless surfaces are the dependable ones under WSL.

## Before you push — the local gate

Run the same checks CI runs, in one shot:

```console
cargo xtask ci    # build + clippy + fmt + test + deny, stops at the first failure
```

…or individually:

```console
cargo test                                   # headless, no GPU needed (mock source)
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check
```

CI mirrors this on `ubuntu-latest` with stable Rust and **no GPU** — the mock source keeps the whole
pipeline headless. (Opening the `wgpu` GUI window needs a real display, so it's a manual smoke step.)

## The testing ladder

Almost everything runs **headless, with no GPU**, via the mock source; only the top two rungs need real
hardware. Most changes touch only the bottom two. **Today the suite is `cargo test` over the `core` model
+ ring buffers; the rungs below are the target that fills in as each surface lands.**

1. **Unit (headless):** the `core` model + ring buffers, parsers (NVML maps, Ollama `/api/ps`,
   Prometheus text), view-model transforms, alert rules, and sink formats (Prometheus/OTLP) —
   table-driven against committed golden fixtures. `cargo test`. *Planned (not yet wired):* property
   tests (`proptest`) for invariants like the ring buffer, and fuzzing (`cargo-fuzz`) for the parsers,
   which eat external input.
2. **View-model / output snapshots:** what each surface *would* draw or emit, without a window — GUI
   view models, the `ratatui` test backend, `ps --json` golden, the Prometheus exposition golden.
   Because every surface is a pure view of `core`, a new metric gets one model test + thin per-surface
   assertions.
3. **Manual smoke (real hardware):** the GUI and `top` on a real **NVIDIA + Linux** host — single-GPU,
   multi-GPU, MIG. Confirm it renders steadily, idles when nothing changes, and tears down cleanly.
4. **Real inference e2e:** with a local **Ollama**/**vLLM** (or recorded output), assert the inference
   panel populates next to the GPU the workload saturates, in both the GUI and the TUI.

Per the overhead budget, the monitor's own footprint is a tracked metric: benchmark startup +
steady-state, keep buffers bounded regardless of uptime, render on change (not on a spin loop), and
surface our own CPU/GPU/RAM. Regressions are bugs.

## The no-panic discipline

`unwrap`, `expect`, and `panic!` are **`deny`-lints outside tests** (re-allowed in tests via
`clippy.toml`). A monitor must degrade gracefully — an absent or asleep GPU, a missing inference
endpoint, or a dead remote host is a clear "no signal" state, never a crash. Handle the error.

## Phases & decisions

Work is organized into phases (Phase 0 → Phase 11) that ladder to the one `v0.1.0` release
(see [`ROADMAP.md`](./ROADMAP.md)). Each phase closes with a **git tag + a working demo + green CI**, and
isn't started until the prior is green. Record any significant, hard-to-reverse decision in
[`ARCHITECTURE.md`](./ARCHITECTURE.md) (Design decisions) so the *why* outlives the diff.

## Commit & PR conventions

- One logical change per commit; imperative subject ("Add the NVML utilization sampler", not "added").
- **Never add AI co-author or attribution trailers**; never commit secrets or fetched/generated data.
- Every metric must be reachable in all three human surfaces (GUI, TUI, `--json`) — frontend parity.
- Every PR must pass the full gate (`cargo xtask ci`).

## License

By contributing you agree your contributions are licensed under **Apache-2.0**, the project's license.
