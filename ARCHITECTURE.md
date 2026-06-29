# Architecture

The design of the monitor and the decisions behind it. The staged plan is in
[`ROADMAP.md`](./ROADMAP.md); the build commands + invariants in [`.rules`](./.rules); how to build and
contribute in [`CONTRIBUTING.md`](./CONTRIBUTING.md).

## The shape — one headless engine, many surfaces

A headless `collector` samples data **sources** and feeds an in-memory model (`core`); **every
surface — GUI, TUI, one-shot CLI, and the machine sinks (Prometheus / OTLP / Splunk) — only *renders*
`core`.** No surface talks to NVML, DCGM, Ollama, or Prometheus directly. This is the load-bearing
invariant: it's what makes the app testable without a screen, demoable without a GPU, scriptable,
exportable, and remote-capable. If a surface reaches into a data source, the design has broken.

```
   data sources                core (the headless engine)         surfaces — human + machine
 ┌──────────────────┐  sample  ┌─────────────────────┐  read    ┌───────────────────────────────┐
 │ NVML  (values)   │ ───────▶ │  device/process     │ ───────▶ │ GUI   wgpu/egui window          │
 │ DCGM  (optional) │          │  snapshots +        │  ├─────▶ │ TUI   `top`  (ratatui)          │
 │ Ollama/vLLM/...  │          │  ring-buffer series │  ├─────▶ │ CLI   `ps --json`  (one-shot)   │
 │ mock  (no GPU)   │          │  + inference join   │  └─────▶ │ Sinks Prometheus · OTLP · Splunk │
 └──────────────────┘          └─────────────────────┘          └───────────────────────────────┘
   (remote hosts, Phase 10, are just another source feeding the same model)
```

The flow is one-directional: **`collector` → `core` → {GUI, TUI, CLI, sinks}`**. The **engine** owns one
sampling loop — it calls the collector's synchronous `sample()` at a sane interval (the collector owns no
thread or clock), timestamps the readings, and appends them to `core`'s bounded, fixed-capacity ring
buffers; surfaces read an immutable snapshot and render or emit it. Adding a metric means updating the
model **once** and the thin renderers/sinks — never duplicating logic, never a GUI-only metric (frontend
parity). *(The full data-flow contract — snapshot publication, the seam traits, signal states — is pinned
in [`ROADMAP.md`](./ROADMAP.md) §0.5.)*

## The crates

> Scaffolded in **Phase 0** — not all exist or are full yet; this is the target layout.

| Crate | Role |
|-------|------|
| `crates/core` | The data model: device/process snapshots, ring-buffer time series, the `Collector` + `Sink` traits. **The spine.** |
| `crates/collector` | Source implementations: `nvml`, `dcgm` (optional, richer/MIG), the inference scraper (Phase 6), and `mock` |
| `crates/ui` | GUI frontend (lib): `wgpu`/`egui` views & widgets — charts, gauges, process table, GPU grid |
| `crates/cli` | Terminal frontend (lib): one-shot `ps`/`--json` + the live `top` TUI (`ratatui`) |
| `crates/export` | Machine sinks (lib): Prometheus, OTLP, Splunk — wired in Phase 9; the `Sink` trait lives in `core` |
| `crates/app` | The single binary: subcommand dispatch (`gui`/`top`/`ps`; `serve` in Phase 10), wires `collector → core → {ui｜cli｜sinks}` |
| `xtask` | Build/dev orchestration (dev-only, never shipped) |

## Design decisions (the *why*)

The load-bearing, hard-to-reverse choices. Record new ones here when you make them.

1. **Native single binary, no Electron.** Ship one self-contained Rust binary — GUI via `wgpu`, TUI via
   `ratatui` — no web stack, fast cold start, statically linkable where the driver allows. *Why:* craft
   and startup speed are the reason to build this over a web dashboard. *Rejected:* Electron/Tauri
   (heavy, slow start); separate GUI and CLI binaries.
2. **Headless engine, pure-view surfaces.** A headless `collector` feeds `core`; every surface only
   renders `core` and never calls a data source. *Why:* testable headless, demoable with no GPU,
   scriptable, exportable, remote-capable; the parsing/trust surface lives in one place. *Rejected:*
   each surface querying NVML itself (untestable, duplicated, can't go remote).
3. **Two first-class frontends, one binary.** GUI and CLI/TUI at parity, dispatched by subcommand
   (Docker/Ollama DX): `gui` (default) · `top` · `ps`/`ps --json` · `serve` (Phase 10). The CLI is a real
   contract — stable `--json` field names, exit codes, `--watch` streaming — not a toy.
4. **Frontend stacks: `egui`/`wgpu` (GUI) + `ratatui` (TUI).** Immediate-mode suits live telemetry and
   ships fast. *Deferred:* bespoke `wgpu`/GPUI for hero visualizations (must not block `v0.1.0`).
5. **GPU source: NVML + DCGM + mock, behind one trait.** NVML (`nvml-wrapper`, `dlopen`, no root) is the
   source of truth; DCGM optional for richer counters and MIG; a permanent **mock** so everything
   builds/tests/demos with no GPU. The `collector` trait is vendor-neutral — AMD/ROCm and Intel are new
   implementations, nothing above changes. *Rejected:* shelling out to `nvidia-smi`; DCGM-only.
6. **Integrate, don't reimplement (the `Sink` seam).** Exporters — Prometheus, OTLP, Splunk (Phase 9) — are
   *machine-facing pure views of `core`*, the mirror of the human frontends. We expose data; we never
   become a dashboard, a time-series database, or an alerting platform. The **anti-platform** move:
   plug into the user's stack, don't rebuild it.
7. **Release pipeline from day one; keyless signing.** A tag-triggered workflow builds (from the committed
   `Cargo.lock`, `--locked`), checksums, **keyless-signs** (sigstore `cosign` over GitHub OIDC — no
   long-lived private key to manage or leak), and publishes — wired in Phase 0, before there's anything to
   ship. Phase previews tag `0.1.0-alpha.N`; the final is `v0.1.0`. *Why:* deferring release/packaging to
   the end is the classic way to discover signing and packaging breaks when they're most expensive — so we
   exercise the whole path on the first preview tag. *Rejected:* long-lived GPG/private keys; packaging only at 1.0.

Unprivileged userspace — no root, no kernel modules. For real data you need a GPU + its driver; the
**mock** source works everywhere with none.

- **GPU:** NVIDIA via **NVML** is first-class (current target). **DCGM** optional (richer counters +
  MIG). **AMD/ROCm** and **Intel** are post-1.0 behind the `collector` trait. The **mock** source is
  permanent.
- **OS:** **Linux `x86_64`** first-class; **`arm64`** intended (Graviton/Jetson); **macOS (Metal)** and
  **Windows** post-1.0 behind the trait.
- **Inference runtimes (Phase 6):** **Ollama** (`/api/ps`, the first integration), **vLLM / Triton / TGI**
  (Prometheus). Optional, auto-discovered or opt-in.
- **Exporters (Phase 9):** **Prometheus** `/metrics`, **OTLP** (→ Grafana/Datadog/Honeycomb/any OTel
  collector), **Splunk** (OTLP or HEC). Optional, composable with the frontends.
- **GUI vs. headless:** the GUI needs a display + a `wgpu`-capable GPU (or a software fallback); the
  `top` TUI and `ps` run anywhere, including over SSH on headless boxes.

## Extension model & non-goals

How you build *on* agent — and the lines we won't cross. All three fall out of the headless-engine split.

- **Extend via the two traits, not a public API.** The crates are library-shaped (`core`/`collector`/`ui`/
  `cli`/`export` are libs; `app` is a thin shell), so the engine *is* a library internally. But the
  **committed** extension surface is the **`Collector` and `Sink` traits** plus the **wire contracts**
  (`ps --json`, `/metrics`, OTLP) — **not** an embeddable Rust API. A new GPU vendor is a `Collector`; a
  new exporter is a `Sink`; a consumer reads the wire contracts in any language. `agent-core`'s Rust API
  is **not** SemVer-guaranteed pre-1.0; promoting it to a supported embeddable library is a deliberate
  post-1.0 decision, only if real demand appears. *Why:* a public API is a heavy SemVer/maintenance
  burden, and "integrate, don't reimplement" means people consume our **data**, not our crates.
- **Daemon mode is opt-in and additive — never the default.** agent is local-first and zero-config:
  launch it, see your GPUs, no service required. Running headless as a background worker — the
  exporter (`/metrics` alongside any mode, Phase 9) or the `serve` collector (the Ollama-style invisible
  daemon, managed by `systemd`/a container, Phase 10) — is purely additive; disabling it leaves a fully useful
  foreground tool. *Why:* never-a-hog and local-first; a monitor you must stand up as a service has lost
  the plot.
- **Non-goals (the anti-platform line).** agent is **not** a time-series database, a dashboard, or an
  alerting platform — it *exposes* data to the stack you already run (Phase 9). It is **not** a
  general-purpose GPU-telemetry library — it's an application with a trait-based extension SDK. And it is
  **not** a hosted/SaaS product: **local-first and peer-to-peer, nothing leaves your infrastructure
  without explicit opt-in.** Any cloud/SaaS variant would be a **separate project in its own repo** — never
  this one. If a feature starts to look like storage, dashboards, a public engine API, or a hosted backend,
  it's on the wrong side of the line.

## Invariants

The non-negotiables live in [`.rules`](./.rules) (Invariants) and the
[Architectural invariants](./ROADMAP.md) section of the roadmap. In short: the headless-engine/pure-view
split, two first-class frontends at parity, native/no-Electron/single-binary, integrate-don't-reimplement
(sinks are pure views), local-first/zero-config, never-a-hog, inference-awareness as the wedge,
vendor-neutral behind a trait, and the permanent mock source.
