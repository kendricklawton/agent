# Roadmap — a native GPU & inference monitor (Rust): GUI + CLI

> **The first public release is `v0.1.0` — the full monitor, built in phases.** The project is a **native
> GPU and AI-inference monitor** — a single Rust binary with **two first-class frontends** (a
> GPU-accelerated **GUI** and a terminal **CLI/TUI**) that plugs into the observability stack you already
> run (Prometheus / OTLP / Splunk). In the spirit of Zed, Ghostty, and Ollama: open-source, fast,
> single-binary, no Electron, craft-first. Supersedes the prior eBPF Kubernetes-agent plan.
> **The work is organized into _phases_ (Phase 0 → Phase 11), not a version per step — they ship as public
> pre-releases and ladder to the one `v0.1.0` release.** **Working name:** *TBD* (the repo is still
> `agent`; renaming is a pre-Phase-1 task).

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
        ▲ remote hosts (Phase 10) ────────┘   one engine · many surfaces (human + machine) · local or remote
```

Seven keystones hold it up:

1. **Headless engine, pure-view surfaces.** A headless `collector` feeds an in-memory model (`core`);
   **every surface — GUI, TUI, one-shot CLI, and every machine sink (Prometheus/OTLP) — only *renders*
   `core`.** No surface talks to NVML directly. This is what makes the app testable without a screen,
   demoable without a GPU, scriptable, exportable, and remote-capable. If a surface reaches into a data
   source, the design has broken.
2. **Two first-class frontends, one binary.** A native **GUI** for the desk and a **CLI/TUI** for the
   terminal/SSH/scripting — *neither is an afterthought*. One binary, subcommands (the Docker/Ollama
   DX): `gui` (default) · `top` (live TUI) · `ps` (one-shot, `--json`) · `serve` (Phase 10). Every metric
   is reachable in all three human surfaces.
3. **Native and fast — no Electron, ever.** GUI via `wgpu`, TUI via `ratatui`, one statically-linkable
   binary, sub-second cold start. This is the craft bar; a choice that trades it away is the wrong one.
4. **Inference-aware is the differentiator.** Model attribution + serving KPIs (Ollama/vLLM/Triton) are
   *why this exists*; generic hardware counters are table stakes.
5. **Local-first, zero-config.** Launch it → your local GPUs (and any running Ollama/vLLM) are *already
   there* — no server, account, or config file. Exporters (Phase 9), remote (Phase 10), and persistence
   (Phase 7) are purely additive.
6. **A monitor must never be a hog.** The tool that watches resource use cannot itself burn a core.
   Sample sanely, render on change (adaptive frame rate, idle when backgrounded), bound every buffer,
   and surface our *own* CPU/GPU/RAM.
7. **Integrate, don't reimplement.** The monitor plugs into the observability stack you already run —
   a Prometheus `/metrics` endpoint, OTLP, Splunk (Phase 9) — as *machine-facing pure views of `core`*, the
   mirror of the human frontends. We expose data; we never become a dashboard, a time-series database,
   or an alerting platform. That's the **anti-platform** move: plug into theirs, don't rebuild it.

**The discipline test for every step:** *does this make the GPU/inference picture clearer or a surface
faster/nicer, without coupling a surface to a data source, bloating startup, making the monitor heavy,
or reimplementing something the user's stack already does?* If no, it sinks to a later phase.
**Correctness and feel before features.**

This roadmap delivers **`v0.1.0` — a polished, inference-aware, GUI-and-CLI, exportable,
optionally-remote GPU monitor — across Phases 0–11.** Each phase is a **public pre-release tag + a working
demo + green CI**; we don't start one until the prior is green. NVIDIA/NVML first; the collector is built
behind a trait so AMD/ROCm and Intel can follow without touching any surface.

## §0.5 The engineering contract (what every phase leans on)

§0 fixes *what* and *why*; this pins the *how* that **Phase 1 forces and every later phase inherits** —
the cross-cutting decisions that, left implicit, get re-litigated mid-phase. Defaults below are
recommended, not sacred; each is recorded in [`ARCHITECTURE.md`](ARCHITECTURE.md) when its phase lands.
(The always-on Rust *conventions* — error model, `unsafe`, units, feature flags — live in
[`.rules`](.rules) Code conventions, not here.)

1. **Data flow — the engine owns the loop.** `Collector::sample(&mut self) -> Result<Vec<DeviceSample>,
   CollectError>` is **synchronous** and returns the *current readings only* — it owns **no thread and no
   clock**. The **engine** owns one sampling loop (a single background **thread** by default — see R10; ~1 Hz, configurable),
   calls `sample()`, **timestamps** it, appends to `core`'s ring buffers, and assembles an immutable
   **`Snapshot`** (devices now; processes Phase 4, inference Phase 6, multi-host Phase 10). The engine
   publishes the latest `Arc<Snapshot>` through **one `ArcSwap`/`watch` cell**; surfaces hold a cheap clone
   and read **lock-free**. `core` is `Send + Sync`; **no surface ever holds the collector.** A new snapshot
   *wakes* the surfaces (GUI `request_repaint()`, TUI loop, sinks publish) — never a busy spin. This is
   keystone 6 and the sample-rate-≠-frame-rate split made concrete.
2. **The two seam traits (mirrored, synchronous, error-as-value).** In: `Collector::sample() ->
   Result<Vec<DeviceSample>, CollectError>`. Out: `Sink::publish(&mut self, snap: &Snapshot) ->
   Result<(), SinkError>` — reads a snapshot, never a source. **Both are sync at the boundary; errors are
   values, never panics** (no-panic discipline). Any I/O (an HTTP scrape) lives *inside* an implementation
   behind the sync call — on a **worker thread (blocking) by default**, a runtime only if a real need ever
   appears (see R10). NVML is already synchronous.
3. **Time & history — one clock.** Every sample is stamped by the **engine** (monotonic), not the source, so
   NVML + inference + remote align for the Phase 6 join and the Phase 10 merge. Ring buffers are
   **fixed-count**, sized `history_window / sample_interval` (so "a few minutes" is config, not a leak).
   Time-series widgets **decimate to viewport pixels** (min/max per column) — never tessellate N points
   into P pixels.
4. **Configuration — one model, set once.** Precedence: **flags > `AGENT_*` env > TOML at the platform
   config dir > defaults.** `app` resolves one `Config` and hands it to the engine; surfaces never read
   config piecemeal. It owns sample interval, history window, source (mock/NVML/DCGM), inference endpoints
   (Phase 6), alert rules (Phase 8), sinks (Phase 9), remote hosts (Phase 10). Introduced minimally in
   Phase 1; each phase **adds keys, never a new mechanism.**
5. **Signal states — how "no data" looks.** A monitor's value is largely *how it renders the absence of a
   signal.* One enum, rendered explicitly by **every** surface, never a panic or a blank:
   **`Ok · NoData · Asleep · Unsupported · PermissionDenied · Stale{age} · Offline{host}`**. Everywhere this
   roadmap says "degrade gracefully," it means *render one of these.*
6. **Wire-contract stability — scripts and dashboards depend on us.** `ps --json` carries a top-level
   **`schema_version`**; fields are **additive-only** within a major (`#[non_exhaustive]` keeps the Rust
   types additive; golden tests guard the JSON — see `.rules`). Prometheus metrics live in a documented **`agent_*`** namespace. Renaming a JSON field or
   a metric is a **SemVer-major break** — it silently breaks someone's `jq` or Grafana panel. The *human*
   table may change freely; the *machine* contracts may not.

---

## Phase index

| Phase | Goal | Surface | Demo (observable outcome) |
|-------|------|---------|---------------------------|
| 0 | **Scaffold, CI & release pipeline** | both | one binary; `gui` opens a window at display refresh, `ps` prints a stub table; CI green; a signed artifact builds on tag |
| 1 | **First metric — GUI + CLI** ⭐ | both | one GPU's util/mem live as a GUI sparkline *and* as `ps --json` — the engine feeding two surfaces |
| 2 | **Single-GPU dashboard** | GUI (+`ps`) | a polished real-time GUI view of one GPU: util, mem, temp, power, clocks, occupancy, PCIe + history |
| 3 | **Live TUI — `top`** ⭐ | TUI | `<app> top` over SSH: a beautiful live terminal dashboard, the same model as the GUI |
| 4 | **Per-process attribution** | both | which PID/process holds how much VRAM + GPU time — sortable in GUI and TUI, listable via `ps` |
| 5 | **Multi-GPU + MIG** | both | an 8-GPU box (and MIG slices) at a glance — a GUI grid and a TUI list |
| 6 | **Inference-workload awareness** ⭐ | both | Ollama/vLLM tokens/sec + KV-cache shown next to the GPU it's saturating, joined by process |
| 7 | **History & persistence** | both | persisted history that survives restarts; scrub back over hours in GUI and TUI |
| 8 | **Alerts & scripting** | both | thresholds → desktop/CLI alerts; `ps --json --watch \| jq` streams live metrics to a script |
| 9 | **Exporters & integrations** | sinks | Prometheus scrapes `/metrics`; the same GPU+inference data lands in your existing Grafana / Splunk |
| 10 | **Remote & multi-host** | both | `serve` a thin headless collector; watch several machines from one GUI/TUI |
| 11 | **Packaging & release** ⭐ | all | `brew install` / AppImage → it just runs; signed releases — **ships as `v0.1.0`** |

> **Earliest "wow":** Phase 1 — one metric, live, in *both* a window and the terminal — proves the whole
> headless-engine-many-surfaces design. **The hook is Phase 6** — tokens/sec next to GPU saturation is the
> thing no other local tool shows. The signature demo rides on **Phase 4** (the model↔GPU join uses process
> attribution), so the fastest credible path is **Phase 0→1→2→3→4→6**; Phase 5 (multi-GPU) is the main
> piece you can defer for a single-box demo.

> **Public from Phase 1 (the Docker model, done right):** every phase closes with a **public pre-release
> tag** — `0.1.0-alpha.N` (preview), not a private checkpoint — so you ship early and iterate on real
> feedback through the whole run. Phases are the *build structure*; the alpha tags are the *public cadence*;
> the polished **`v0.1.0`** final lands at **Phase 11**. A **later `v1.0.0` will mark *stability*** — frozen
> `collector`/`Sink` seams and wire contracts — **not first usefulness.**

---

## Phase 0 — Scaffold, CI & release pipeline
A reproducible Rust build: one binary that dispatches subcommands, opens a GPU-accelerated window
*and* prints a CLI table, gated by CI — **and a release pipeline that exists from day one.**

- [ ] Cargo workspace + the crate split (below).
- [ ] **Rename off `agent` (R8):** once a name is chosen, rename the repo, crates (`agent-*` → `<name>-*`),
  binary, and all docs — one discrete pre-Phase-1 commit, before external eyes.
- [ ] **Binary dispatch (the DX spine):** `app` is one binary with subcommands — `gui` (default on a
  desktop) · `top` (live TUI) · `ps` (one-shot, `--json`); `serve` is added in Phase 10. On a headless box
  with no display, bare invocation falls back to `top` with a hint. Docker/Ollama-style verbs.
- [ ] **Decision: the frontend stacks** → recorded in [`ARCHITECTURE.md`](ARCHITECTURE.md). **GUI:** `egui`
  on `wgpu`; **TUI:** `ratatui`. Both are pure views of `core`. (Bespoke `wgpu`/GPUI noted for hero visuals later.)
- [ ] **Decision: the GPU data source** → [`ARCHITECTURE.md`](ARCHITECTURE.md): **NVML** is the source of
  truth (`nvml-wrapper`); **DCGM** optional (richer/MIG); a **mock** source so the app builds, tests, and
  demos **with no GPU present**. Three sources behind one trait.
- [ ] **Headless-engine split** wired as empty crates: `collector` produces samples, `core` holds the
  model (incl. the stubbed `Collector` *and* `Sink` traits — the in/out seams), `ui`/`cli` render —
  with the mock collector already feeding fake data so both the window and the `ps` table show something.
- [ ] CI: `cargo build`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `cargo deny check`, a headless
  test run (mock collector → no GPU), and the **feature matrix** (`--no-default-features` + powerset). `gui`
  window-open is a manual smoke step; `ps`/`top` are CI-testable headlessly.
- [ ] **Release pipeline from day one (don't defer packaging):** a tag-triggered CI workflow that builds,
  **signs**, checksums, and publishes an artifact, plus a `CHANGELOG.md` (keep-a-changelog) and
  conventional-commit-friendly history. It ships nothing real yet — but from Phase 1 on, every phase emits
  a signed **`0.1.0-alpha.N`**, so packaging/signing breaks surface early, not at Phase 11.
- [ ] Record the foundational decisions in [`ARCHITECTURE.md`](ARCHITECTURE.md) — the *why* that outlives the diff.
- [ ] Tag the phase (`gui` opens + renders steadily; `ps` prints a stub table; the release workflow runs).

> **Crate layout (the target):**
> ```
> crates/core       the data model: device/process snapshots, ring-buffer time series, the collector + sink traits. The spine.
> crates/collector  source implementations: nvml, dcgm (optional), inference scraper (Phase 6), mock
> crates/ui         GUI frontend (lib): wgpu/egui views & widgets — charts, gauges, process table, GPU grid
> crates/cli        terminal frontend (lib): one-shot `ps`/`--json` + the live `top` TUI (ratatui)
> crates/export     machine sinks (lib): prometheus, otlp, splunk — wired in Phase 9 (the `Sink` trait lives in core)
> crates/app        the single binary: subcommand dispatch, wires collector → core → {ui | cli | sinks}, config
> xtask             build/dev orchestration (dev-only, never shipped)
> ```

## Phase 1 — First metric — GUI + CLI ⭐
The end-to-end slice for one number, proving the engine feeds **two** surfaces. **Demo:** `<app> gui`
shows your GPU's utilization + memory updating live as a sparkline; `<app> ps --json` prints the same
numbers for a script — both reading the identical `core`.

- [ ] **`core` model:** `DeviceSample { util, mem_used, mem_total, ts }` (unit newtypes — see `.rules`) and a
  fixed-capacity **ring-buffer** time series per device (bounded — a monitor never grows unboundedly).
- [ ] **`collector` (NVML):** poll `nvmlDeviceGetUtilizationRates` + `nvmlDeviceGetMemoryInfo` on the
  engine's background loop (default ~1 Hz). Mock collector emits a synthetic signal so every surface is
  identical with no GPU.
- [ ] **GUI:** one panel — a number + a live sparkline bound to the ring buffer; redraw on new data,
  **idle when nothing changed** (adaptive frame rate, not a busy spin).
- [ ] **CLI:** `ps` prints a human table; `ps --json` prints structured JSON (the scripting contract —
  `schema_version`, stable field names, exit codes).
- [ ] **Decouple cleanly (§0.5):** the engine owns the sample loop and publishes an immutable `Snapshot`
  (`ArcSwap`); GUI and CLI read only `core`; swapping mock↔NVML changes neither frontend. **This wires the
  data-flow contract (R1) and settles the concurrency primitive (R10).**
- [ ] **Tests:** `core` ring-buffer semantics (bounded, ordered, wraps) — `proptest`; the mock collector
  drives a headless loop and asserts the model advances; `ps --json` output is golden-tested — all without a GPU.

## Phase 2 — Single-GPU dashboard
Turn the one metric into the full real-time picture of one GPU — the first genuinely lovable view.
**Demo:** a clean, responsive `gui` dashboard for one GPU with live charts and a few minutes of history.

- [ ] Extend `DeviceSample` to the full set: utilization, memory, **temperature, power, clocks (SM/mem),
  SM occupancy, PCIe throughput, fan**. (NVML covers all of these — and `ps`/`--json` get them for free.)
- [ ] **Charts that feel good:** smooth live line charts + a rolling few-minutes window; gauges for
  instantaneous values; tasteful layout and typography.
- [ ] Handle the GPU being absent/asleep gracefully (no panic; a clear signal state).
- [ ] **Self-overhead panel:** show the app's *own* CPU/GPU/RAM — keystone-6 honesty and a forcing
  function to stay light; wire the `criterion` overhead bench here (R9).
- [ ] **Tests:** sample → model → renderable-state transforms (golden snapshots of the view model);
  history bounds; degraded-device state.

> **Note on surface:** Phase 2's metrics land in `core`, so `ps`/`ps --json` gain them immediately; only the
> *rich GUI dashboard* is GUI-specific. The TUI brings the same metrics to the terminal in Phase 3.

## Phase 3 — Live TUI — `top` ⭐
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

> **Why the TUI early (Phase 3, not late):** headless GPU boxes and SSH are *the* place people need a
> monitor and *can't* run a GUI. Establishing the TUI now makes "both surfaces" a real discipline for
> every later metric, instead of a GUI-first app with a bolted-on CLI.

## Phase 4 — Per-process attribution
Answer "what's *using* my GPU?" — the question that matters when VRAM is full — in **both** surfaces,
and the backbone of the Phase 6 model↔GPU join. **Demo:** a sortable process table in the GUI and the TUI,
and `<app> ps --procs --json` for scripts.

- [ ] **NVML compute processes:** `nvmlDeviceGetComputeRunningProcesses` /
  `...GraphicsRunningProcesses` → per-PID **GPU memory**; per-PID **utilization** via
  `nvmlDeviceGetProcessUtilizationSamples` (or DCGM where available — see R2).
- [ ] Resolve PID → process name / cmdline (read `/proc`); a sortable, filterable table; link each
  process to the device(s) it's on. **Lands in GUI, TUI, and `ps` together.**
- [ ] Bounded + cheap: cache PID→name, prune dead PIDs, never stat the whole table per frame.
- [ ] **Tests:** process-table model from mock; sort/filter; PID lifecycle; `ps --procs --json` golden.

## Phase 5 — Multi-GPU + MIG
Scale from one GPU to a whole box, in both surfaces. **Demo:** an 8-GPU server (and MIG slices) as a
GUI grid and a TUI list, each drillable to the full single-GPU view.

- [ ] Enumerate all devices; **GUI grid** (tiles → expand to the Phase 2 view) and a **TUI multi-device list**.
- [ ] **MIG:** detect and represent MIG instances as first-class devices (memory/compute per slice — see R7).
- [ ] Topology/NVLink awareness where NVML exposes it (optional).
- [ ] Smooth at N=8+ updating live — virtualize/throttle redraws in both frontends.
- [ ] **Tests:** N-device model; MIG enumeration from mock; grid + TUI view-models under churn.

## Phase 6 — Inference-workload awareness ⭐ (the differentiator)
The wedge: tie raw GPU activity to the **AI workload** driving it — and **lead with Ollama**, whose
users are exactly this tool's audience. **Demo:** with Ollama (or vLLM) running, see its loaded models
+ VRAM split (Ollama) or **tokens/sec, queue depth, KV-cache** (vLLM) in a panel next to the GPU it's
saturating, in both the GUI and the TUI.

- [ ] **Ollama first (the beachhead):** auto-detect a local Ollama (`:11434`) and read `/api/ps` →
  loaded models, VRAM, GPU-vs-CPU placement. Zero-config: if Ollama's running, you see it. **This view
  needs only light attribution** (Ollama reports the model's device directly), so it can land even
  ahead of the full Phase 4 process join.
- [ ] **vLLM / Triton / TGI:** scrape the Prometheus endpoint → tokens/sec, queue depth, KV-cache %,
  running/waiting requests, batch size. (Scraping beats fragile uprobes; the runtimes expose this.)
- [ ] **Join model ↔ GPU (the general case):** correlate the serving process (Phase 4) with its device(s)
  (Phase 5) → a "model card" showing both inference KPIs *and* the GPU telemetry it's responsible for — in
  GUI and TUI. **Depends on Phase 4** for runtimes that don't self-report their device.
- [ ] **Config + discovery:** auto-discover common ports; let users register endpoints; a mock
  inference source so Phase 6 is demoable with no real server.
- [ ] **The signature view:** the per-model panel — tokens/sec, KV-cache, queue depth, next to GPU
  util/mem/power for the same device. This is the screenshot the project is *for*.
- [ ] **Tests:** Ollama `/api/ps` + Prometheus-text parsing (golden fixtures **+ `cargo-fuzz`**, R6); the
  model↔GPU join; inference view-model from mock (GUI + TUI).

> **Dragon — opaque/heterogeneous runtimes:** you can't reliably uprobe the framework, and metric
> names differ (Ollama's REST `/api/ps` vs vLLM's Prometheus). Lean on each runtime's **published
> interface**, normalize into one model, and degrade gracefully — show what's there, label what isn't.

## Phase 7 — History & persistence
Make it a tool you *leave running* and look back over. **Demo:** overnight, a persisted history chart you
can scrub back hours in both the GUI and the TUI — survives a restart.

- [ ] **Persisted history:** spill ring buffers to a small on-disk store (**SQLite via `rusqlite`** — R3,
  ubiquitous + queryable; `redb` if a pure-Rust/no-C store is preferred) so history survives restarts.
- [ ] **Scrub-back:** a time cursor over hours of history in GUI and TUI, reading the same `core` model.
- [ ] **Bounded on disk too:** retention windows + compaction; the store never grows without bound (keystone 6).
- [ ] **Tests:** persistence round-trip + retention bounds; scrub-back view-model; no-loss when backgrounded.

## Phase 8 — Alerts & scripting
Make it a tool you *automate against*. **Demo:** a notification fires when GPU 3 goes idle at 02:14; and
`<app> ps --json --watch | jq` streams live metrics into a script.

- [ ] **Alerts:** user-set thresholds (idle > N min, temp, power, mem nearing OOM, tokens/sec dropping)
  → native desktop notifications **and** CLI exit codes / stderr (so `cron`/scripts can act on them).
  Rules live in `Config` (§0.5).
- [ ] **Scriptable CLI:** `ps --json` (one-shot) and `--watch` (a stream of JSON lines) — a stable,
  documented contract for piping into `jq`, dashboards, or alerting. (This is the lowest-common-
  denominator export; the structured Prometheus/OTLP sinks land in Phase 9.)
- [ ] **Tests:** alert-rule evaluation (golden); `--watch` stream format; exit-code contract.

## Phase 9 — Exporters & observability integrations
Plug the monitor into the stack you already run — **integrate, don't reimplement.** **Demo:** point
Prometheus at the monitor's `/metrics` and graph GPU + inference metrics in your *existing* Grafana; or
ship them to Splunk/Datadog via OTLP — without the monitor ever becoming a dashboard or a database.

- [ ] **The `Sink` trait (in `core`):** the mirror of `Collector` — a machine-facing pure view that
  reads the model and publishes it downstream. Like the frontends, a sink never touches a data source.
  (Stubbed in Phase 0; implemented here.)
- [ ] **Prometheus `/metrics` (the anchor):** an HTTP exposition endpoint rendering `core` in Prometheus
  text format — per-device util/mem/temp/power + per-model inference KPIs, labeled by device/model/host.
  A drop-in for `dcgm-exporter`, but with a GUI. **Phase 9 introduces a small embedded HTTP server** that
  runs alongside any mode (GUI open *and* `/metrics` live at once); Phase 10's `serve` later reuses it.
- [ ] **OTLP exporter (the universal pipe):** push metrics over OpenTelemetry — one integration reaches
  **Grafana, Splunk, Datadog, Honeycomb, and any OTel collector**. Optional native **Splunk HEC** sink.
- [ ] **Composable + opt-in:** sinks run *alongside* the frontends and are strictly optional — with
  none configured the tool is fully useful. A sink failure degrades quietly and never disrupts a frontend.
- [ ] **Tests:** Prometheus exposition format (golden), OTLP payload shape, the `Sink` contract from the
  mock source; a failing sink doesn't break the engine or the frontends.

> **Boundary — integrate, don't reimplement:** we *expose* data; Prometheus/Grafana/Splunk do storage,
> dashboards, and alerting. This is the **anti-platform** move — plug into theirs, don't build ours. If
> a feature starts to look like a time-series database or a dashboard, it's on the wrong side of the line.

## Phase 10 — Remote & multi-host
Watch machines that aren't your laptop, from either frontend. **Demo:** one GUI/TUI aggregating several
remote hosts. **Local-first stays intact** — remote is opt-in.

- [ ] **`serve` — the thin headless collector:** the same binary in a headless mode (`<app> serve`)
  reuses `collector` + `core` to sample a host and serve snapshots by **reusing the Phase 9 HTTP server** —
  newline-delimited JSON / SSE stream, **not** gRPC (R4: heavier, no win at this scale) — minimal
  footprint, the Ollama-style invisible daemon. Streams snapshots, not just `/metrics`.
- [ ] **Aggregate in both frontends:** add remote hosts by address; the GUI and TUI render local +
  remote devices uniformly (the headless-engine split pays off — frontends don't care where samples
  originate). Model the world as **hosts → devices**, with localhost as host #1.
- [ ] **Secure + resilient:** **bearer-token auth, TLS optional** (R4); off by default (local-first); a
  dead host degrades to a clear `Offline{host}` tile (§0.5), never hangs a frontend; auto-reconnect.
- [ ] **Tests:** `serve` returns correct snapshots; multi-host model merge; host-down/reconnect.

> **Note:** "watch *my handful of machines* from one screen" — a desktop app talking to thin
> collectors *you* run on your own hosts. Local-first: the data stays on your machines.

## Phase 11 — Packaging & release ⭐ (ships `v0.1.0`)
Ship it like a real product, on the pipeline that's existed since Phase 0. **Demo:** a new user installs
via their package manager (or one file) and it runs — local GPUs visible in seconds, in the window *or*
the terminal *or* their Grafana. **This is where `v0.1.0`, the first public release, is tagged.**

- [ ] **Packaging:** Linux first — AppImage / Flatpak / `.deb`; Homebrew + a `.dmg` for macOS if the
  NVML/Metal story allows; document the matrix honestly (NVIDIA/Linux is first-class).
- [ ] **Seams coherent (full freeze is a later `1.0`):** the `collector` *and* `sink` traits are settled
  enough that a new GPU vendor (**AMD/ROCm**, **Intel**) or a new exporter is a drop-in, no `core`/`ui`/`cli`
  change. As a `0.x` release they may still evolve; the hard SemVer freeze comes with a later `v1.0.0`.
- [ ] **Signed, reproducible final release** + checksums + SBOM (the Phase-0 pipeline, now cutting a real tag).
- [ ] **Docs site:** install, a GUI screenshot tour + a TUI asciinema, the `ps --json` + `/metrics`
  reference, configuring inference endpoints, remote setup, the support matrix, the contribution guide.
- [ ] **Tests:** the full headless suite green across supported targets; a smoke install test.
- [ ] Tag **`v0.1.0`** — the first public release.

---

## Cross-cutting standards (apply to every phase)

**Definition of Done (every phase closes only when all hold):**
- Green gate — `fmt` · `clippy -D warnings` · `test` · `deny` (the `cargo xtask ci` bundle), incl. the feature matrix.
- A working **demo** of the phase's observable outcome.
- Docs updated where touched (README / ARCHITECTURE / `.rules`) **+ a `CHANGELOG.md` entry**.
- The **craft pass** — no rough edges in the surfaces this phase touched, *and* the GUI **feel** accrues
  here: keyboard-driven nav, themes, remembered window + layout state, and (as surfaces pile up) a
  **command palette** — the Zed/Ghostty polish, **complete by Phase 11**. Craft is *continuous*, not a one-off phase.
- **No new `clippy` `allow`s**, no new `unwrap`/`expect`/`panic` outside tests.
- The **overhead budget** (R9 `criterion` bench) hasn't regressed.
- A signed tag from the Phase-0 pipeline — **public `0.1.0-alpha.N` previews from Phase 1 on** (Phase 0
  proves the pipeline; it isn't a public preview yet).

**Engineering conventions** — the always-on Rust rules (error model, `unsafe` policy, unit newtypes,
`#[non_exhaustive]`, feature flags, MSRV) live in [`.rules`](.rules) (Code conventions); every phase follows
them. This roadmap doesn't restate them.

**Frontend parity & DX (the GUI/CLI contract):** one binary, Docker/Ollama-style subcommands (`gui` ·
`top` · `ps` · `serve`). **Every metric must be reachable in all three human surfaces** — GUI, TUI, and
`--json` — because they're all pure views of `core`; a GUI-only metric is a design smell. The machine
sinks (Phase 9) get every metric for the same reason. The CLI is **scriptable, not a toy**: stable JSON
field names, meaningful exit codes, `--watch` streaming.

**Platform & vendor support** — maintained in [`ARCHITECTURE.md`](ARCHITECTURE.md) (Platform support):
- **NVIDIA/NVML, Linux** is the first-class target. DCGM is an optional richer source (and the MIG
  path). macOS/Windows and AMD/Intel are later, behind the collector trait — documented, not faked.
- The **mock collector** is a permanent first-class source: every surface must build, test, run, and
  *demo* with **no GPU and no driver present**.

**Testing ladder:**
- Unit (headless, no GPU): `core` model + ring buffers, metric parsers (NVML maps, Ollama `/api/ps`,
  Prometheus text), view-model transforms, alert rules, **sink formats (Prometheus/OTLP)** — table-driven
  via the mock collector.
- **Property-based (`proptest`):** ring-buffer invariants (bounded/ordered/wraps) and view-model
  transforms — laws, not just examples.
- **Fuzz (`cargo-fuzz`):** the parsers consume *external* input (NVML maps, `/api/ps`, Prometheus text) —
  fuzz them for panics/UB.
- Snapshot tests of view models (what each surface *would* draw/emit): GUI view models, the TUI render
  buffer (`ratatui` test backend), `ps --json` golden, **Prometheus exposition golden** — all without a window.
- Manual smoke on real hardware (single-GPU, multi-GPU, MIG): GUI + `top` over SSH.
- Real inference e2e: a local Ollama/vLLM (or recorded output) → assert the inference panel populates.

**Performance & overhead budget (keystone 6):** the monitor's own footprint is a tracked metric.
Targets: negligible idle CPU, render-on-change (no busy frame loop, no remote-CPU-spinning TUI),
throttle/idle when backgrounded, bounded memory regardless of uptime. The `criterion` startup +
steady-state bench is part of the Definition of Done; regressions are bugs.

**Build/dev graph:**
```
P0 ─▶ P1 ─▶ P2 ─▶ P3 ─▶ P4 ─▶ P5 ─▶ P6 ─▶ P7 ─▶ P8 ─▶ P9 ─▶ P10 ─▶ P11 → v0.1.0
 │     │      └─ GUI ─┘  └TUI┘    └─ P4 feeds the P6 model↔GPU join ─┘
 │     └─ both surfaces established by P3; every metric after lands in GUI + TUI + ps (+ sinks at P9)
 └─ release pipeline up-front → every phase emits a signed public 0.1.0-alpha.N
```
Mostly linear, with two real dependencies: **Phase 6 (inference) needs Phase 4 (process attribution)** for
the general model↔GPU join, and **Phase 9 (exporters) + Phase 10 (remote)** share the same `serve` HTTP
server (do Phase 9 first; Phase 10 reuses it). Phase 6 is the differentiator — get to it as early as Phase 4
allows.

### Scope & where the lines go (an estimate, not a target)
A focused native app with two frontends and machine exporters — realistically **~25–50k lines of Rust**
for `v0.1.0`, depending mostly on how bespoke the GUI gets (`egui` low end; custom `wgpu` higher) and
how far the remote/multi-host layer (Phase 10) grows. Rough shape: `core` small and central; `collector` moderate (NVML + DCGM +
inference + mock); `ui` (GUI) the largest single piece; `cli` (TUI + one-shot) moderate; `export` thin
(sinks are pure views); tests ~30%. **The number is an estimate to size the work, never a goal** — the unit
of progress is the phase's Definition of Done, and a smaller line count is a win.

---

## Risks & open decisions (the dragons)

A roadmap that hides its forks isn't dense, it's vague. These are the unresolved decisions and known
landmines — each with a **recommended default** and the **phase that must close it**. Resolutions are
recorded in [`ARCHITECTURE.md`](ARCHITECTURE.md) as they land.

| # | Decision / risk | Recommended default | Close by |
|---|-----------------|---------------------|----------|
| R1 | Snapshot publication (engine → surfaces) | `ArcSwap<Arc<Snapshot>>` + per-surface wake (§0.5) | Phase 1 |
| R2 | NVML per-process util is driver-/permission-gated and flaky (`nvmlDeviceGetProcessUtilizationSamples`) | VRAM-per-PID always; util-per-PID best-effort with a signal state; DCGM where present | Phase 4 |
| R3 | Persistence engine | **SQLite** (`rusqlite`); `redb` if pure-Rust/no-C is required | Phase 7 |
| R4 | Remote protocol + auth | reuse the Phase 9 HTTP server: **NDJSON/SSE + bearer token, TLS optional**; not gRPC | Phase 10 |
| R5 | wgpu GUI smoke in headless CI | GUI smoke stays **manual**; CI covers `ps`/`top`/sinks headlessly; `lavapipe` software-fallback as a smoke only, never a perf gate | Phase 0 / ongoing |
| R6 | Inference metric-name drift across runtimes/versions | normalize at the parser into one model; pinned fixtures + `cargo-fuzz`; label what's missing | Phase 6 |
| R7 | MIG enumeration + NVML API-level skew | MIG instances are first-class devices; probe capabilities, degrade per signal state; document the min driver | Phase 5 |
| R8 | The repo/crate **rename off `agent`** to the chosen name is disruptive | one discrete pre-Phase-1 commit, before external eyes | pre-Phase-1 |
| R9 | The overhead budget has no enforcement | a `criterion` startup + steady-state bench in the Definition of Done; the in-app self-overhead panel (Phase 2) | Phase 2 |
| R10 | Concurrency primitive — do we need an async runtime (`tokio`) at all? | **No by default** — `std::thread` for the engine loop + blocking `ureq` for the ~1 Hz scrape; a runtime only if a real need appears (keeps binary + compile small — the never-a-hog tax) | Phase 1 / Phase 6 |

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
- **Integrate, don't reimplement (sinks are pure views).** Exporters — Prometheus, OTLP, Splunk (Phase 9) —
  are machine-facing views of `core`, opt-in and composable with the frontends; the monitor never
  stores, dashboards, or alerts on its own behalf. Plug into the user's stack; don't rebuild it.
- **Local-first, zero-config.** Launch → local GPUs (and any running Ollama/vLLM) visible, no
  server/account/config required. Exporters (Phase 9), remote (Phase 10), and persistence (Phase 7) are
  additive; disabling them leaves a fully useful tool.
- **A monitor must never be a hog.** Sample sanely, render on change, idle when backgrounded, bound
  every buffer, surface our own footprint. The tool that watches usage cannot cause it.
- **Inference awareness is the wedge** — model attribution and serving KPIs (Ollama-first) are why this
  exists; raw hardware counters alone are commodity.
- **Vendor-neutral behind a trait.** NVIDIA/NVML first, but `core`/`ui`/`cli` never assume NVIDIA — a
  new GPU vendor is a new `collector` implementation, nothing else.
- **The mock collector is permanent.** Every build/test/demo path works with no GPU present.
- **Public pre-release tags from Phase 1**; the `v0.1.0` release is signed and reproducible (full seam-freeze
  + the SemVer-stability guarantee harden into a later `v1.0.0`).
