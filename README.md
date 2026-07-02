# agent *(name TBD)*

A native, open-source **GPU & AI-inference monitor** — written in **Rust**, with both a
GPU-accelerated **GUI** and a terminal **CLI/TUI**. See what your GPUs and inference servers are
actually doing: in a gorgeous window on your desk, live in your terminal over SSH, as `--json` you
pipe into a script, or via a `/metrics` endpoint your Prometheus already scrapes.

> **Status:** in active development — NVIDIA/NVML on Linux first. There's no published binary yet; build
> from source ([`CONTRIBUTING.md`](./CONTRIBUTING.md)). The repo is still named `agent` pending a rename.

## Why
`nvidia-smi` is a CLI snapshot; `nvtop` is a TUI with no history or inference awareness;
`dcgm-exporter` + Grafana is a server you stand up. None of them is *one fast, native, inference-aware
tool that's equally good in a window and in a terminal*. Two things set this apart:

- **Native craft, two ways.** A `wgpu`-rendered GUI *and* a `ratatui` TUI in one single binary —
  instant startup, buttery real-time, no Electron. It should feel good to leave open.
- **Inference-workload awareness.** Not just hardware counters — attribute GPU use to the
  **process → model/server** and surface **tokens/sec, queue depth, KV-cache** from
  **Ollama / vLLM / Triton**. The part `nvtop` structurally can't do.

## What it does
- **GUI** (`gui`): live charts, gauges, a sortable process table, a multi-GPU grid, inference panels.
- **TUI** (`top`): the same real-time view in your terminal — perfect for headless GPU boxes over SSH.
- **CLI** (`ps`, `ps --json`): a one-shot, scriptable snapshot of devices and processes.
- **Per-process attribution**, **multi-GPU + MIG**, **inference KPIs** (Ollama-first), **history +
  alerts**, and optional **remote/multi-host** monitoring from one screen.
- **Exporters** (`/metrics`): plug the same data into the stack you already run — **Prometheus**,
  **OTLP** (Grafana/Datadog/Honeycomb), **Splunk** — without it ever becoming a dashboard or database.

Every surface — GUI, TUI, CLI, and the exporters — is a pure view of one **headless engine**
(`collector` → `core`), so every metric is available everywhere, and the whole thing builds, tests,
and demos with **no GPU present** (a mock data source).

## Usage
```
agent              # launch the GUI (default)
agent top          # live TUI in the terminal (Phase 3)
agent ps           # one-shot snapshot table
agent ps --json    # ... as JSON, for scripts
agent serve        # thin headless collector, for remote/multi-host monitoring (Phase 10)
```

## Stack
Rust · GUI [`egui`](https://github.com/emilk/egui) on [`wgpu`](https://wgpu.rs) ·
TUI [`ratatui`](https://ratatui.rs) · GPU [`nvml-wrapper`](https://docs.rs/nvml-wrapper) (NVML),
DCGM optional. NVIDIA/Linux is the first-class target; AMD/Intel and macOS sit behind the same
vendor-neutral collector trait.

## Layout
```
crates/core       data model: snapshots, ring-buffer series, the collector + sink traits
crates/collector  sources: nvml, dcgm (optional), inference scraper, mock
crates/ui         GUI frontend (lib): wgpu/egui views & widgets
crates/cli        terminal frontend (lib): one-shot ps/--json + the live top TUI (ratatui)
crates/export     machine sinks (lib): prometheus, otlp, splunk
crates/app        the single binary: subcommand dispatch, wires collector → core → {ui | cli | sinks}
xtask             build orchestration (dev-only)
```

## Security
Report vulnerabilities **privately** — see [`SECURITY.md`](./SECURITY.md). The monitor is an
unprivileged, read-mostly tool (NVML needs no root); the notable surfaces are the optional remote
collector and parsing data from inference endpoints.

## License
[Apache-2.0](./LICENSE).

---
**Build it & contribute:** [`CONTRIBUTING.md`](./CONTRIBUTING.md). The invariants and agent guidance
live in [`.rules`](./.rules); the design in [`ARCHITECTURE.md`](./ARCHITECTURE.md); the staged build
plan in [`ROADMAP.md`](./ROADMAP.md).
