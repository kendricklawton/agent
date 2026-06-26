# Roadmap — a native GPU & inference monitor (Rust): GUI + CLI

> **This roadmap supersedes the prior eBPF Kubernetes-agent plan.** The project is a **native GPU and
> AI-inference monitor** — a single Rust binary with **two first-class frontends**: a GPU-accelerated
> **GUI** *and* a terminal **CLI/TUI** — and it plugs into the observability stack you already run
> (Prometheus / OTLP / Splunk). In the spirit of Zed, Ghostty, and Ollama: open-source, fast,
> single-binary, no Electron, craft-first.
> **Working name:** *TBD* (the repo is still `agent`; renaming the repo + crates is a pre-M1 task).

## §0 The spine

**One tool that shows — beautifully and in real time — what your GPUs and inference servers are
actually doing, however you want to look at it: a gorgeous native window on your desk, a live view in
your terminal over SSH, a one-shot `--json` you pipe into a script, or a `/metrics` endpoint your
Prometheus already scrapes.** `nvidia-smi` is a CLI snapshot; `nvtop` is a TUI with no history or
inference awareness; `dcgm-exporter` + Grafana is a server you stand up. None of them is *one fast,
native, inference-aware tool that's equally good in a window, in a terminal, and in your dashboards*.
That gap is the whole project.

The wedge — why build this and not just run `nvtop`:

1. **Native craft, two ways.** A `wgpu`-rendered **GUI** *and* a `ratatui` **TUI**, one single binary,
   instant startup, buttery real-time. The Zed/Ghostty bar for the window; the htop/Ghostty bar for the
   terminal. It should feel *good* whether it's on a second monitor or in an SSH session.
2. **Inference-workload awareness.** Not just hardware counters — attribute GPU use to the
   **process → model/server**, and surface **tokens/sec, queue depth, KV-cache, batch size** from
   Ollama/vLLM/Triton. That's the part `nvtop` structurally can't do.

The shape it builds toward — **one headless engine, many surfaces (human and machine):**

```
   data sources                core (the headless engine)         surfaces — human + machine
 ┌──────────────────┐  sample  ┌─────────────────────┐  read    ┌───────────────────────────────┐
 │ NVML  (values)   │ ───────▶ │  device/process     │ ───────▶ │ GUI   wgpu/egui window          │
 │ DCGM  (optional) │          │  snapshots +        │  ├─────▶ │ TUI   `top`  (ratatui)          │
 │ Ollama/vLLM/...  │          │  ring-buffer series │  ├─────▶ │ CLI   `ps --json`  (one-shot)   │
 │ mock  (no GPU)   │          │  + inference join   │  └─────▶ │ Sinks Prometheus · OTLP · Splunk │
 └──────────────────┘          └──────────┬──────────┘          └───────────────────────────────┘
        ▲ remote hosts (M9) ──────────────┘   one engine · many surfaces (human + machine) · local or remote
```

Seven keystones hold it up:

1. **Headless engine, pure-view surfaces.** A headless `collector` feeds an in-memory model (`core`);
   **every surface — GUI, TUI, one-shot CLI, and every machine sink (Prometheus/OTLP) — only *renders*
   `core`.** No surface talks to NVML directly. This is what makes the app testable without a screen,
   demoable without a GPU, scriptable, exportable, and remote-capable. If a surface reaches into a data
   source, the design has broken.
2. **Two first-class frontends, one binary.** A native **GUI** for the desk and a **CLI/TUI** for the
   terminal/SSH/scripting — *neither is an afterthought*. One binary, subcommands (the Docker/Ollama
   DX): `gui` (default) · `top` (live TUI) · `ps` (one-shot, `--json`) · `serve` (M9). Every metric is
   reachable in all three human surfaces.
3. **Native and fast — no Electron, ever.** GUI via `wgpu`, TUI via `ratatui`, one statically-linkable
   binary, sub-second cold start. This is the craft bar; a choice that trades it away is the wrong one.
4. **Inference-aware is the differentiator.** Model attribution + serving KPIs (Ollama/vLLM/Triton) are
   *why this exists*; generic hardware counters are table stakes.
5. **Local-first, zero-config.** Launch it → your local GPUs (and any running Ollama/vLLM) are *already
   there* — no server, account, or config file. Exporters (M8), remote (M9), and persistence (M7) are
   purely additive.
6. **A monitor must never be a hog.** The tool that watches resource use cannot itself burn a core.
   Sample sanely, render on change (adaptive frame rate, idle when backgrounded), bound every buffer,
   and surface our *own* CPU/GPU/RAM.
7. **Integrate, don't reimplement.** The monitor plugs into the observability stack you already run —
   a Prometheus `/metrics` endpoint, OTLP, Splunk (M8) — as *machine-facing pure views of `core`*, the
   mirror of the human frontends. We expose data; we never become a dashboard, a time-series database,
   or an alerting platform. That's the **anti-platform** move: plug into theirs, don't rebuild it.

**The discipline test for every step:** *does this make the GPU/inference picture clearer or a surface
faster/nicer, without coupling a surface to a data source, bloating startup, making the monitor heavy,
or reimplementing something the user's stack already does?* If no, it sinks to a later milestone.
**Correctness and feel before features.**

This roadmap ladders to **`v1.0.0` — a polished, inference-aware, GUI-and-CLI, exportable,
optionally-remote GPU monitor.** Each milestone `M0…M10` maps to a tag and is a **git tag + a working
demo + green CI**; we don't start one until the prior is green. NVIDIA/NVML first; the collector is
built behind a trait so AMD/ROCm and Intel can follow without touching any surface.

---

## Milestone index

| M | Milestone | Tag | Surface | Demo (observable outcome) |
|---|-----------|-----|---------|---------------------------|
| 0 | **Scaffold & CI** | `v0.0.0` | both | one binary; `gui` opens a 60-fps window, `ps` prints a stub table; CI green; decisions recorded |
| 1 | **First metric — GUI + CLI** ⭐ | `v0.1.0` | both | one GPU's util/mem live as a GUI sparkline *and* as `ps --json` — the engine feeding two surfaces |
| 2 | **Single-GPU dashboard** | `v0.2.0` | GUI (+`ps`) | a polished real-time GUI view of one GPU: util, mem, temp, power, clocks, occupancy, PCIe + history |
| 3 | **Live TUI — `top`** ⭐ | `v0.3.0` | TUI | `<app> top` over SSH: a beautiful live terminal dashboard, the same model as the GUI |
| 4 | **Per-process attribution** | `v0.4.0` | both | which PID/process holds how much VRAM + GPU time — sortable in GUI and TUI, listable via `ps` |
| 5 | **Multi-GPU + MIG** | `v0.5.0` | both | an 8-GPU box (and MIG slices) at a glance — a GUI grid and a TUI list |
| 6 | **Inference-workload awareness** ⭐ | `v0.6.0` | both | Ollama/vLLM tokens/sec + KV-cache shown next to the GPU it's saturating, joined by process |
| 7 | **History, alerts & scripting** | `v0.7.0` | both | persisted history + desktop/CLI alerts; `ps --json --watch` for piping; the GUI craft pass |
| 8 | **Exporters & integrations** | `v0.8.0` | sinks | Prometheus scrapes `/metrics`; the same GPU+inference data lands in your existing Grafana / Splunk |
| 9 | **Remote & multi-host** | `v0.9.0` | both | `serve` a thin headless collector; watch several machines from one GUI/TUI |
| 10 | **1.0 — packaging & release** ⭐ | `v1.0.0` | all | `brew install` / AppImage → it just runs; signed releases; `collector` + `sink` traits stable |

> **Earliest "wow":** M1 — one metric, live, in *both* a window and the terminal — proves the whole
> headless-engine-many-surfaces design. **The hook is M6** — tokens/sec next to GPU saturation is the
> thing no other local tool shows. The signature demo rides on **M4** (the model↔GPU join uses process
> attribution), so the fastest credible path is **M0→M1→M2→M3→M4→M6**; M5 (multi-GPU) is the main piece
> you can defer for a single-box demo. (M6's Ollama-only beachhead view can land with lighter
> attribution, since `/api/ps` reports the model's GPU directly — see M6.)

---

## M0 — Scaffold & CI
A reproducible Rust build: one binary that dispatches subcommands, opens a GPU-accelerated window
*and* prints a CLI table, gated by CI. No real metrics yet — the skeleton and the core decisions.

- [ ] Cargo workspace + the crate split (below). Rename the repo/crates off `agent`.
- [ ] **Binary dispatch (the DX spine):** `app` is one binary with subcommands — `gui` (default on a
  desktop) · `top` (live TUI) · `ps` (one-shot, `--json`) · `serve` (M9). On a headless box with no
  display, bare invocation falls back to `top` with a hint. Docker/Ollama-style verbs.
- [ ] **Decision: the frontend stacks** → recorded in [`ARCHITECTURE.md`](ARCHITECTURE.md). **GUI:** `egui` on `wgpu` (immediate-mode suits live
  telemetry; ships fast; bespoke `wgpu`/GPUI noted for hero visuals later). **TUI:** `ratatui`. Both
  are pure views of `core`.
- [ ] **Decision: the GPU data source** → [`ARCHITECTURE.md`](ARCHITECTURE.md) (carries the old hybrid design): **NVML** is the source
  of truth (`nvml-wrapper`); **DCGM** optional (richer/MIG); a **mock** source so the app builds,
  tests, and demos **with no GPU present**. Three sources behind one trait.
- [ ] **Headless-engine split** wired as empty crates: `collector` produces samples, `core` holds the
  model (incl. the stubbed `Collector` *and* `Sink` traits — the in/out seams), `ui`/`cli` render —
  with the mock collector already feeding fake data so both the window and the `ps` table show something.
- [ ] CI: `cargo build`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `cargo deny check`, and a
  headless test run (mock collector → CI needs no GPU). `gui` window-open is a manual smoke step; `ps`
  and `top` are CI-testable headlessly.
- [ ] Record the foundational decisions in [`ARCHITECTURE.md`](ARCHITECTURE.md) (native binary, headless
  engine, two frontends, frontend stacks, GPU source) — the *why* that outlives the diff.
- [ ] Tag `v0.0.0` (`gui` opens + renders steadily; `ps` prints a stub table; both close cleanly).

> **Crate layout (the target):**
> ```
> crates/core       the data model: device/process snapshots, ring-buffer time series, the collector + sink traits. The spine.
> crates/collector  source implementations: nvml, dcgm (optional), inference scraper (M6), mock
> crates/ui         GUI frontend (lib): wgpu/egui views & widgets — charts, gauges, process table, GPU grid
> crates/cli        terminal frontend (lib): one-shot `ps`/`--json` + the live `top` TUI (ratatui)
> crates/export     machine sinks (lib): prometheus, otlp, splunk — wired in M8 (the `Sink` trait lives in core)
> crates/app        the single binary: subcommand dispatch, wires collector → core → {ui | cli | sinks}, config
> xtask             build/dev orchestration (dev-only, never shipped)
> ```

## M1 — First metric — GUI + CLI ⭐
The end-to-end slice for one number, proving the engine feeds **two** surfaces. **Demo:** `<app> gui`
shows your GPU's utilization + memory updating live as a sparkline; `<app> ps --json` prints the same
numbers for a script — both reading the identical `core`.

- [ ] **`core` model:** `DeviceSample { util, mem_used, mem_total, ts }` and a fixed-capacity
  **ring-buffer** time series per device (bounded — a monitor never grows unboundedly).
- [ ] **`collector` (NVML):** poll `nvmlDeviceGetUtilizationRates` + `nvmlDeviceGetMemoryInfo` on a
  background task (default ~1 Hz); push into `core`. Mock collector emits a synthetic signal so every
  surface is identical with no GPU.
- [ ] **GUI:** one panel — a number + a live sparkline bound to the ring buffer; redraw on new data,
  **idle when nothing changed** (adaptive frame rate, not a busy spin).
- [ ] **CLI:** `ps` prints a human table of current device state; `ps --json` prints structured JSON
  (the scripting contract — stable field names, exit codes).
- [ ] **Decouple cleanly:** GUI and CLI read only `core`; swapping mock↔NVML changes neither frontend.
- [ ] **Tests:** `core` ring-buffer semantics (bounded, ordered, wraps); the mock collector drives a
  headless loop and asserts the model advances; `ps --json` output is golden-tested — all without a GPU.

## M2 — Single-GPU dashboard
Turn the one metric into the full real-time picture of one GPU — the first genuinely lovable view.
**Demo:** a clean, responsive `gui` dashboard for one GPU with live charts and a few minutes of history.

- [ ] Extend `DeviceSample` to the full set: utilization, memory, **temperature, power, clocks (SM/mem),
  SM occupancy, PCIe throughput, fan**. (NVML covers all of these — and `ps`/`--json` get them for free.)
- [ ] **Charts that feel good:** smooth live line charts + a rolling few-minutes window; gauges for
  instantaneous values; tasteful layout and typography (the GUI craft pass).
- [ ] Handle the GPU being absent/asleep gracefully (no panic; a clear "no signal" state).
- [ ] **Self-overhead panel:** show the app's *own* CPU/GPU/RAM — keystone-6 honesty and a forcing
  function to stay light.
- [ ] **Tests:** sample → model → renderable-state transforms (golden snapshots of the view model);
  history bounds; degraded-device state.

> **Note on surface:** M2's metrics land in `core`, so `ps`/`ps --json` gain them immediately; only the
> *rich GUI dashboard* is GUI-specific. The TUI brings the same metrics to the terminal in M3.

## M3 — Live TUI — `top` ⭐
The second first-class frontend: a beautiful live terminal dashboard for servers, SSH, and people who
live in the terminal. **Demo:** `ssh gpubox; <app> top` → a real-time `ratatui` view of the GPU,
updating in place — the same `core` the GUI uses, no window required.

- [ ] **`ratatui` TUI:** a live full-screen terminal view — util/mem/temp/power gauges + sparklines,
  refreshing in place; keyboard quit/scroll; resizes cleanly.
- [ ] **Same model, different renderer:** the TUI reads `core` exactly like the GUI — proving the
  pure-view keystone. No metric is GUI-only.
- [ ] **Terminal-grade craft:** works over SSH, in tmux, on a 256-color or truecolor terminal; sane
  fallback on a dumb terminal; low redraw cost (don't spin the remote CPU).
- [ ] **Tests:** TUI view-model from mock samples (assert the rendered buffer/grid headlessly via
  `ratatui`'s test backend); resize + no-GPU states.

> **Why the TUI early (M3, not late):** headless GPU boxes and SSH are *the* place people need a
> monitor and *can't* run a GUI. Establishing the TUI now makes "both surfaces" a real discipline for
> every later metric, instead of a GUI-first app with a bolted-on CLI.

## M4 — Per-process attribution
Answer "what's *using* my GPU?" — the question that matters when VRAM is full — in **both** surfaces,
and the backbone of the M6 model↔GPU join. **Demo:** a sortable process table in the GUI and the TUI,
and `<app> ps --procs --json` for scripts.

- [ ] **NVML compute processes:** `nvmlDeviceGetComputeRunningProcesses` /
  `...GraphicsRunningProcesses` → per-PID **GPU memory**; per-PID **utilization** via
  `nvmlDeviceGetProcessUtilizationSamples` (or DCGM where available).
- [ ] Resolve PID → process name / cmdline (read `/proc`); a sortable, filterable table; link each
  process to the device(s) it's on. **Lands in GUI, TUI, and `ps` together.**
- [ ] Bounded + cheap: cache PID→name, prune dead PIDs, never stat the whole table per frame.
- [ ] **Tests:** process-table model from mock; sort/filter; PID lifecycle; `ps --procs --json` golden.

## M5 — Multi-GPU + MIG
Scale from one GPU to a whole box, in both surfaces. **Demo:** an 8-GPU server (and MIG slices) as a
GUI grid and a TUI list, each drillable to the full single-GPU view.

- [ ] Enumerate all devices; **GUI grid** (tiles → expand to the M2 view) and a **TUI multi-device list**.
- [ ] **MIG:** detect and represent MIG instances as first-class devices (memory/compute per slice).
- [ ] Topology/NVLink awareness where NVML exposes it (optional).
- [ ] Smooth at N=8+ updating live — virtualize/throttle redraws in both frontends.
- [ ] **Tests:** N-device model; MIG enumeration from mock; grid + TUI view-models under churn.

## M6 — Inference-workload awareness ⭐ (the differentiator)
The wedge: tie raw GPU activity to the **AI workload** driving it — and **lead with Ollama**, whose
users are exactly this tool's audience. **Demo:** with Ollama (or vLLM) running, see its loaded models
+ VRAM split (Ollama) or **tokens/sec, queue depth, KV-cache** (vLLM) in a panel next to the GPU it's
saturating, in both the GUI and the TUI.

- [ ] **Ollama first (the beachhead):** auto-detect a local Ollama (`:11434`) and read `/api/ps` →
  loaded models, VRAM, GPU-vs-CPU placement. Zero-config: if Ollama's running, you see it. **This view
  needs only light attribution** (Ollama reports the model's device directly), so it can land even
  ahead of the full M4 process join.
- [ ] **vLLM / Triton / TGI:** scrape the Prometheus endpoint → tokens/sec, queue depth, KV-cache %,
  running/waiting requests, batch size. (Scraping beats fragile uprobes; the runtimes expose this.)
- [ ] **Join model ↔ GPU (the general case):** correlate the serving process (M4) with its device(s)
  (M5) → a "model card" showing both inference KPIs *and* the GPU telemetry it's responsible for — in
  GUI and TUI. **Depends on M4** for runtimes that don't self-report their device.
- [ ] **Config + discovery:** auto-discover common ports; let users register endpoints; a mock
  inference source so M6 is demoable with no real server.
- [ ] **The signature view:** the per-model panel — tokens/sec, KV-cache, queue depth, next to GPU
  util/mem/power for the same device. This is the screenshot the project is *for*.
- [ ] **Tests:** Ollama `/api/ps` + Prometheus-text parsing (golden fixtures); the model↔GPU join;
  inference view-model from mock (GUI + TUI).

> **Dragon — opaque/heterogeneous runtimes:** you can't reliably uprobe the framework, and metric
> names differ (Ollama's REST `/api/ps` vs vLLM's Prometheus). Lean on each runtime's **published
> interface**, normalize into one model, and degrade gracefully — show what's there, label what isn't.

## M7 — History, alerts & scripting
Make it a tool you *leave running* and *automate against*. **Demo:** overnight, a persisted history
chart and a notification that GPU 3 went idle at 02:14; and `<app> ps --json --watch | jq` streams live
metrics to a script.

- [ ] **Persisted history:** spill ring buffers to a small on-disk store (embedded TS / SQLite) so
  history survives restarts; scrub back over hours in GUI and TUI.
- [ ] **Alerts:** user-set thresholds (idle > N min, temp, power, mem nearing OOM, tokens/sec dropping)
  → native desktop notifications **and** CLI exit codes / stderr (so `cron`/scripts can act on them).
- [ ] **Scriptable CLI:** `ps --json` (one-shot) and `--watch` (a stream of JSON lines) — a stable,
  documented contract for piping into `jq`, dashboards, or alerting. (This is the lowest-common-
  denominator export; the structured Prometheus/OTLP sinks land in M8.)
- [ ] **GUI craft pass:** keyboard-driven nav, command palette, themes, arrangeable layouts, remembered
  window state — the Zed/Ghostty feel milestone.
- [ ] **Tests:** persistence round-trip + retention bounds; alert-rule evaluation (golden); `--watch`
  stream format; no-loss when backgrounded.

## M8 — Exporters & observability integrations
Plug the monitor into the stack you already run — **integrate, don't reimplement.** **Demo:** point
Prometheus at the monitor's `/metrics` and graph GPU + inference metrics in your *existing* Grafana; or
ship them to Splunk/Datadog via OTLP — without the monitor ever becoming a dashboard or a database.

- [ ] **The `Sink` trait (in `core`):** the mirror of `Collector` — a machine-facing pure view that
  reads the model and publishes it downstream. Like the frontends, a sink never touches a data source.
  (Stubbed in M0; implemented here.)
- [ ] **Prometheus `/metrics` (the anchor):** an HTTP exposition endpoint rendering `core` in Prometheus
  text format — per-device util/mem/temp/power + per-model inference KPIs, labeled by device/model/host.
  A drop-in for `dcgm-exporter`, but with a GUI. **M8 introduces a small embedded HTTP server** that
  runs alongside any mode (GUI open *and* `/metrics` live at once); M9's `serve` later reuses it.
- [ ] **OTLP exporter (the universal pipe):** push metrics over OpenTelemetry — one integration reaches
  **Grafana, Splunk, Datadog, Honeycomb, and any OTel collector**. Optional native **Splunk HEC** sink.
- [ ] **Composable + opt-in:** sinks run *alongside* the frontends and are strictly optional — with
  none configured the tool is fully useful. A sink failure (unreachable endpoint, encode error)
  degrades quietly and never disrupts a frontend.
- [ ] **Tests:** Prometheus exposition format (golden), OTLP payload shape, the `Sink` contract from the
  mock source; a failing sink doesn't break the engine or the frontends.

> **Boundary — integrate, don't reimplement:** we *expose* data; Prometheus/Grafana/Splunk do storage,
> dashboards, and alerting. This is the **anti-platform** move — plug into theirs, don't build ours. If
> a feature starts to look like a time-series database or a dashboard, it's on the wrong side of the line.

## M9 — Remote & multi-host
Watch machines that aren't your laptop, from either frontend. **Demo:** one GUI/TUI aggregating several
remote hosts. **Local-first stays intact** — remote is opt-in.

- [ ] **`serve` — the thin headless collector:** the same binary in a headless mode (`<app> serve`)
  reuses `collector` + `core` to sample a host and serve snapshots (small gRPC / stream) — minimal
  footprint, the Ollama-style invisible daemon. Built on the **M8 HTTP server**, now also streaming
  snapshots, not just `/metrics`.
- [ ] **Aggregate in both frontends:** add remote hosts by address; the GUI and TUI render local +
  remote devices uniformly (the headless-engine split pays off — frontends don't care where samples
  originate). Model the world as **hosts → devices**, with localhost as host #1.
- [ ] **Secure + resilient:** authenticated transport; a dead host degrades to a clear "offline" tile,
  never hangs a frontend; auto-reconnect.
- [ ] **Tests:** `serve` returns correct snapshots; multi-host model merge; host-down/reconnect.

> **Note:** "watch *my handful of machines* from one screen" — a desktop app talking to thin
> collectors *you* run on your own hosts. Local-first: the data stays on your machines.

## M10 — 1.0 — packaging & release ⭐
Ship it like a real product. **Demo:** a new user installs via their package manager (or one file) and
it runs — local GPUs visible in seconds, in the window *or* the terminal *or* their Grafana.

- [ ] **Packaging:** Linux first — AppImage / Flatpak / `.deb`; Homebrew + a `.dmg` for macOS if the
  NVML/Metal story allows; document the matrix honestly (NVIDIA/Linux is first-class).
- [ ] **Stable seams frozen:** the `collector` *and* `sink` traits are stable enough that a new GPU
  vendor (**AMD/ROCm**, **Intel**) or a new exporter is a drop-in, no `core`/`ui`/`cli` change.
  (Implementations can be post-1.0.)
- [ ] **Signed, reproducible releases** + checksums; a CI release pipeline; SBOM.
- [ ] **Docs site:** install, a GUI screenshot tour + a TUI asciinema, the `ps --json` + `/metrics`
  reference, configuring inference endpoints, remote setup, the support matrix, the contribution guide.
- [ ] **Tests:** the full headless suite green across supported targets; a smoke install test.
- [ ] Tag `v1.0.0`.

---

## Cross-cutting standards (apply to every milestone)

**Frontend parity & DX (the GUI/CLI contract):** one binary, Docker/Ollama-style subcommands (`gui` ·
`top` · `ps` · `serve`). **Every metric must be reachable in all three human surfaces** — GUI, TUI, and
`--json` — because they're all pure views of `core`; a GUI-only metric is a design smell. The machine
sinks (M8) get every metric for the same reason. The CLI is **scriptable, not a toy**: stable JSON
field names, meaningful exit codes, `--watch` streaming. Adding a metric means updating the model once
and the thin renderers/sinks, never duplicating logic.

**Platform & vendor support** — maintained in [`ARCHITECTURE.md`](ARCHITECTURE.md) (Platform support):
- **NVIDIA/NVML, Linux** is the first-class target. DCGM is an optional richer source (and the MIG
  path). macOS/Windows and AMD/Intel are post-1.0 behind the collector trait — documented, not faked.
- The **mock collector** is a permanent first-class source: every surface must build, test, run, and
  *demo* with **no GPU and no driver present**.

**Testing ladder:**
- Unit (headless, no GPU): `core` model + ring buffers, metric parsers (NVML maps, Ollama `/api/ps`,
  Prometheus text), view-model transforms, alert rules, **sink formats (Prometheus/OTLP)** — table-driven
  via the mock collector.
- Snapshot tests of view models (what each surface *would* draw/emit): GUI view models, the TUI render
  buffer (`ratatui` test backend), `ps --json` golden output, **Prometheus exposition golden** — all
  without a window.
- Manual smoke on real hardware (single-GPU, multi-GPU, MIG): GUI + `top` over SSH.
- Real inference e2e: a local Ollama/vLLM (or recorded output) → assert the inference panel populates.

**Performance & overhead budget (keystone 6):** the monitor's own footprint is a tracked metric.
Targets: negligible idle CPU, render-on-change (no busy frame loop, no remote-CPU-spinning TUI),
throttle/idle when backgrounded, bounded memory regardless of uptime. Benchmark startup + steady-state;
show our own usage in-app; regressions are bugs.

**Build/dev graph:**
```
M0 ─▶ M1 ─▶ M2 ─▶ M3 ─▶ M4 ─▶ M5 ─▶ M6 ─▶ M7 ─▶ M8 ─▶ M9 ─▶ M10 (1.0)
       │      └─ GUI ─┘  └TUI┘    └─ M4 feeds the M6 model↔GPU join ─┘
       └─ both surfaces established by M3; every metric after lands in GUI + TUI + ps (+ sinks at M8)
```
Mostly linear, with two real dependencies: **M6 (inference) needs M4 (process attribution)** for the
general model↔GPU join, and **M8 (exporters) + M9 (remote)** share the same `serve` HTTP server (do M8
first; M9 reuses it). M6 is the differentiator — get to it as early as M4 allows.

### Scope & where the lines go (an estimate, not a target)
A focused native app with two frontends and machine exporters — realistically **~25–50k lines of Rust**
at 1.0, depending mostly on how bespoke the GUI gets (`egui` low end; custom `wgpu` higher) and whether
remote (M9) is in. Rough shape: `core` small and central; `collector` moderate (NVML + DCGM + inference
+ mock); `ui` (GUI) the largest single piece; `cli` (TUI + one-shot) moderate; `export` thin (sinks are
pure views); tests ~30%. **The number is an estimate to size the work, never a goal** — the unit of
progress is the milestone tag + demo + green CI, and a smaller line count is a win. (The CLI/TUI and the
sinks all add little *because* they reuse `core` — that's the headless-engine dividend.)

---

## Architectural invariants (never traded away)

- **Headless engine; every surface is a pure view.** GUI, TUI, one-shot CLI, and machine sinks all read
  only `core` — none calls NVML/DCGM/Ollama/Prometheus directly. This keeps the app testable headless,
  demoable without a GPU, scriptable, exportable, and remote-capable. A surface reaching into a data
  source means the design has broken.
- **Two first-class frontends, one binary.** A native GUI *and* a CLI/TUI, shipped together, at parity —
  every metric in all surfaces. The CLI is real (stable `--json`, exit codes, `--watch`), not a toy.
- **Native, no Electron, single binary.** GUI via `wgpu`, TUI via `ratatui`, fast cold start, statically
  linkable where the driver allows. The craft bar is the reason to build this over a web app.
- **Integrate, don't reimplement (sinks are pure views).** Exporters — Prometheus, OTLP, Splunk (M8) —
  are machine-facing views of `core`, opt-in and composable with the frontends; the monitor never
  stores, dashboards, or alerts on its own behalf. Plug into the user's stack; don't rebuild it.
- **Local-first, zero-config.** Launch → local GPUs (and any running Ollama/vLLM) visible, no
  server/account/config required. Exporters (M8), remote (M9), and persistence (M7) are additive;
  disabling them leaves a fully useful tool.
- **A monitor must never be a hog.** Sample sanely, render on change, idle when backgrounded, bound
  every buffer, surface our own footprint. The tool that watches usage cannot cause it.
- **Inference awareness is the wedge** — model attribution and serving KPIs (Ollama-first) are why this
  exists; raw hardware counters alone are commodity.
- **Vendor-neutral behind a trait.** NVIDIA/NVML first, but `core`/`ui`/`cli` never assume NVIDIA — a
  new GPU vendor is a new `collector` implementation, nothing else.
- **The mock collector is permanent.** Every build/test/demo path works with no GPU present.
- **SemVer + a git tag per milestone**; releases are signed and reproducible (from M10; informally earlier).
