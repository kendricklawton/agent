# Roadmap — a native GPU & inference monitor (Rust): GUI + CLI

> **This roadmap commits only to `v0.1.0` — the first public release (M0 → M1).** The long-term shape is
> the *north star* in §0; the planned milestones below stop at the first public release, and a future
> revision will plan what comes after once `0.1.0` ships. The project is a **native GPU and AI-inference
> monitor** — a single Rust binary with **two first-class frontends** (a GPU-accelerated **GUI** and a
> terminal **CLI/TUI**) that plugs into the observability stack you already run. In the spirit of Zed,
> Ghostty, and Ollama: open-source, fast, single-binary, no Electron, craft-first. Supersedes the prior
> eBPF Kubernetes-agent plan.
> **Working name:** *TBD* (the repo is still `agent`; renaming is a pre-M1 task).

## §0 The spine (the north star)

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
        ▲ remote hosts (later) ───────────┘   one engine · many surfaces (human + machine) · local or remote
```

Seven keystones hold it up (the principles, even where the feature is post-`0.1.0`):

1. **Headless engine, pure-view surfaces.** A headless `collector` feeds an in-memory model (`core`);
   **every surface — GUI, TUI, one-shot CLI, and every machine sink — only *renders* `core`.** No surface
   talks to NVML directly. This is what makes the app testable without a screen, demoable without a GPU,
   scriptable, exportable, and remote-capable. If a surface reaches into a data source, the design has broken.
2. **Two first-class frontends, one binary.** A native **GUI** for the desk and a **CLI/TUI** for the
   terminal/SSH/scripting — *neither is an afterthought*. One binary, subcommands (the Docker/Ollama
   DX): `gui` (default) · `top` (live TUI) · `ps` (one-shot, `--json`). Every metric is reachable in every
   human surface. *(At `0.1.0` that's the GUI and `ps`; the live `top` TUI lands later.)*
3. **Native and fast — no Electron, ever.** GUI via `wgpu`, TUI via `ratatui`, one statically-linkable
   binary, sub-second cold start. This is the craft bar; a choice that trades it away is the wrong one.
4. **Inference-aware is the differentiator.** Model attribution + serving KPIs (Ollama/vLLM/Triton) are
   *why this exists*; generic hardware counters are table stakes.
5. **Local-first, zero-config.** Launch it → your local GPUs (and any running Ollama/vLLM) are *already
   there* — no server, account, or config file. Exporters, remote, and persistence are purely additive
   (all post-`0.1.0`).
6. **A monitor must never be a hog.** The tool that watches resource use cannot itself burn a core.
   Sample sanely, render on change (adaptive frame rate, idle when backgrounded), bound every buffer,
   and surface our *own* CPU/GPU/RAM.
7. **Integrate, don't reimplement.** The monitor plugs into the observability stack you already run —
   Prometheus `/metrics`, OTLP, Splunk (post-`0.1.0`) — as *machine-facing pure views of `core`*, the
   mirror of the human frontends. We expose data; we never become a dashboard, a time-series database,
   or an alerting platform. That's the **anti-platform** move: plug into theirs, don't rebuild it.

**The discipline test for every step:** *does this make the GPU/inference picture clearer or a surface
faster/nicer, without coupling a surface to a data source, bloating startup, making the monitor heavy,
or reimplementing something the user's stack already does?* If no, it sinks past `0.1.0`.
**Correctness and feel before features.**

The north star above is the full vision. **This roadmap commits only through `v0.1.0` (M0 → M1)** — the
first public release: one real GPU metric, live, in a window *and* in the terminal. Each milestone is a
**git tag + a working demo + green CI**; we don't start one until the prior is green. NVIDIA/NVML first;
the collector is built behind a trait so other vendors can follow without touching any surface. What
comes after `0.1.0` is sketched under [Beyond v0.1.0](#beyond-v010--the-horizon-not-planned-here).

## §0.5 The engineering contract (what M0/M1 must get right)

§0 fixes *what* and *why*; this pins the *how* that **M1 forces** — the cross-cutting decisions that,
left implicit, get re-litigated mid-milestone. They're written once here so the seams are ready for the
horizon, not just `0.1.0`. Defaults are recommended, not sacred; each is recorded in
[`ARCHITECTURE.md`](ARCHITECTURE.md) when it lands.

1. **Data flow — the engine owns the loop.** `Collector::sample(&mut self) -> Result<Vec<DeviceSample>,
   CollectError>` is **synchronous** and returns the *current readings only* — it owns **no thread and no
   clock**. The **engine** owns one sampling loop (a single background task, default ~1 Hz, configurable),
   calls `sample()`, **timestamps** it, appends to `core`'s ring buffers, and assembles an immutable
   **`Snapshot`** (devices now; processes, inference, and multi-host come later). The engine publishes the
   latest `Arc<Snapshot>` through **one `ArcSwap`/`watch` cell**; surfaces hold a cheap clone and read
   **lock-free**. `core` is `Send + Sync`; **no surface ever holds the collector.** A new snapshot *wakes*
   the surfaces (GUI `request_repaint()`, TUI loop, sinks publish) — never a busy spin. This is keystone 6
   and the sample-rate-≠-frame-rate split made concrete.
2. **The two seam traits (mirrored, synchronous, error-as-value).** In: `Collector::sample() ->
   Result<Vec<DeviceSample>, CollectError>`. Out: `Sink::publish(&mut self, snap: &Snapshot) ->
   Result<(), SinkError>` — reads a snapshot, never a source. **Both are sync at the boundary; errors are
   values, never panics** (no-panic discipline). Async (HTTP scrape, NVML) lives *inside* implementations on
   the engine's runtime, behind the sync call — it never leaks into the trait or a surface. (`Sink` is
   stubbed at M0 and unused until exporters land post-`0.1.0`, but the seam is fixed now.)
3. **Time & history — one clock.** Every sample is stamped by the **engine** (monotonic), not the source, so
   later inference + remote samples align to the same clock. Ring buffers are **fixed-count**, sized
   `history_window / sample_interval` (so "a few minutes" is config, not a leak). Time-series widgets
   **decimate to viewport pixels** (min/max per column) — never tessellate N points into P pixels.
4. **Configuration — one model, set once.** Precedence: **flags > `AGENT_*` env > TOML at the platform
   config dir > defaults.** `app` resolves one `Config` and hands it to the engine; surfaces never read
   config piecemeal. At `0.1.0` it owns the sample interval, history window, and source (mock/NVML);
   later keys (inference endpoints, sinks, remote hosts, alert rules) **add to it, never a new mechanism.**
5. **Signal states — how "no data" looks.** A monitor's value is largely *how it renders the absence of a
   signal.* One enum, rendered explicitly by **every** surface, never a panic or a blank:
   **`Ok · NoData · Asleep · Unsupported · PermissionDenied · Stale{age} · Offline{host}`**. Everywhere this
   roadmap says "degrade gracefully," it means *render one of these.*
6. **Wire-contract stability — scripts and dashboards depend on us.** `ps --json` carries a top-level
   **`schema_version`**; fields are **additive-only** within a major. Machine metrics live in a documented,
   stable namespace (`<binary>_*`). Renaming a JSON field or a metric is a **SemVer-major break** — it
   silently breaks someone's `jq` or Grafana panel. The *human* table may change freely; the *machine*
   contracts may not. (The `--json` contract starts at `0.1.0`; metric exporters come later.)

---

## Milestone index

| M | Milestone | Tag | Surface | Demo (observable outcome) |
|---|-----------|-----|---------|---------------------------|
| 0 | **Scaffold & CI** | `v0.0.0` | both | one binary; `gui` opens a window that renders at display refresh, `ps` prints a stub table; CI green; decisions recorded |
| 1 | **First metric — GUI + CLI** ⭐ _(first public release)_ | `v0.1.0` | both | one GPU's util/mem live as a GUI sparkline *and* as `ps --json` — the engine feeding two surfaces |

> **Earliest "wow" and the whole point of `0.1.0`:** M1 — one real metric, live, in *both* a window and
> the terminal — proves the entire headless-engine-many-surfaces design end to end. Everything in the
> horizon is "another metric / another surface" riding on the seam M1 establishes.

> **Versioning posture (the Docker model):** **`v0.1.0` (M1) is the first *public* release** — Docker
> itself went public at `0.1.0` and developed in the open for a year before `1.0`. `0.x` means "real and
> usable, moving fast, not yet API-stable," and shipping early is the point. A later **`v1.0.0` will mark
> *stability*** — frozen `collector`/`Sink` seams and wire contracts — **not first usefulness.** Every
> `0.x` milestone is tagged and public; 1.0 is the "you can build on this" line, not the debut.

---

## M0 — Scaffold & CI
A reproducible Rust build: one binary that dispatches subcommands, opens a GPU-accelerated window
*and* prints a CLI table, gated by CI. No real metrics yet — the skeleton and the core decisions.

- [ ] Cargo workspace + the crate split (below).
- [ ] **Rename off `agent` (R8):** once a name is chosen, rename the repo, crates (`agent-*` → `<name>-*`),
  binary, and all docs — one discrete pre-M1 commit, before external eyes. Disruptive once people depend
  on names; cheap now.
- [ ] **Binary dispatch (the DX spine):** `app` is one binary with subcommands — `gui` (default on a
  desktop) · `top` (live TUI) · `ps` (one-shot, `--json`). On a headless box with no display, bare
  invocation falls back to `top` with a hint. Docker/Ollama-style verbs.
- [ ] **Decision: the frontend stacks** → recorded in [`ARCHITECTURE.md`](ARCHITECTURE.md). **GUI:** `egui`
  on `wgpu` (immediate-mode suits live telemetry; ships fast; bespoke `wgpu`/GPUI noted for hero visuals
  later). **TUI:** `ratatui`. Both are pure views of `core`.
- [ ] **Decision: the GPU data source** → [`ARCHITECTURE.md`](ARCHITECTURE.md): **NVML** is the source of
  truth (`nvml-wrapper`); **DCGM** optional (richer/MIG, later); a **mock** source so the app builds,
  tests, and demos **with no GPU present**. Sources behind one trait.
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
> crates/collector  source implementations: nvml, mock (dcgm + inference scraper come later)
> crates/ui         GUI frontend (lib): wgpu/egui views & widgets — charts, gauges, process table, GPU grid
> crates/cli        terminal frontend (lib): one-shot `ps`/`--json` + the live `top` TUI (ratatui)
> crates/export     machine sinks (lib): prometheus, otlp, splunk — wired post-0.1.0 (the `Sink` trait lives in core)
> crates/app        the single binary: subcommand dispatch, wires collector → core → {ui | cli | sinks}, config
> xtask             build/dev orchestration (dev-only, never shipped)
> ```

## M1 — First metric — GUI + CLI ⭐ _(first public release)_
The end-to-end slice for one number, proving the engine feeds **two** surfaces. **This is `v0.1.0`.**
**Demo:** `<app> gui` shows your GPU's utilization + memory updating live as a sparkline; `<app> ps --json`
prints the same numbers for a script — both reading the identical `core`.

- [ ] **`core` model:** `DeviceSample { util, mem_used, mem_total, ts }` and a fixed-capacity
  **ring-buffer** time series per device (bounded — a monitor never grows unboundedly).
- [ ] **`collector` (NVML):** poll `nvmlDeviceGetUtilizationRates` + `nvmlDeviceGetMemoryInfo` on the
  engine's background loop (default ~1 Hz). Mock collector emits a synthetic signal so every surface is
  identical with no GPU.
- [ ] **GUI:** one panel — a number + a live sparkline bound to the ring buffer; redraw on new data,
  **idle when nothing changed** (adaptive frame rate, not a busy spin).
- [ ] **CLI:** `ps` prints a human table of current device state; `ps --json` prints structured JSON
  (the scripting contract — `schema_version`, stable field names, exit codes).
- [ ] **Decouple cleanly (§0.5):** the engine owns the sample loop and publishes an immutable `Snapshot`
  (`ArcSwap`); GUI and CLI read only `core`; swapping mock↔NVML changes neither frontend. **This wires the
  data-flow contract (R1) the whole north star assumes.**
- [ ] **Tests:** `core` ring-buffer semantics (bounded, ordered, wraps); the mock collector drives a
  headless loop and asserts the model advances; `ps --json` output is golden-tested — all without a GPU.
- [ ] Tag `v0.1.0` and ship it — the first public release.

---

## Beyond v0.1.0 — the horizon (not planned here)

> **Direction, not commitment.** These are the north star from §0, *not* committed milestones — a future
> roadmap revision will plan them (with demos, tags, and tests) once `0.1.0` ships and is real. They're
> listed so the shape is clear and so §0.5 keeps the seams ready; none is scheduled here.

- **Single-GPU dashboard** — the full real-time GUI view: temperature, power, clocks, occupancy, PCIe, history.
- **Live TUI (`top`)** — the second first-class frontend, for SSH and headless boxes.
- **Per-process attribution** — which PID/process holds how much VRAM and GPU time.
- **Multi-GPU + MIG** — a whole box at a glance, each device drillable.
- **Inference-workload awareness** ⭐ — *the wedge*: tokens/sec, KV-cache, queue depth from
  Ollama/vLLM/Triton, joined to the GPU each model saturates.
- **History, alerts & scripting** — persisted history, thresholds → notifications, `ps --json --watch`.
- **Exporters** — Prometheus `/metrics`, OTLP, Splunk; *integrate, don't reimplement*.
- **Remote & multi-host** — `serve` a thin headless collector; watch several machines from one screen.
- **`v1.0.0` — stability** — frozen `collector`/`Sink` seams + wire contracts, signed releases, packaging.

> Each carries its own open decisions (NVML per-process util, persistence engine, remote protocol/auth,
> inference metric-name drift, MIG enumeration) — recorded when that milestone is planned, **not**
> pre-committed here.

---

## Cross-cutting standards (apply to M0, M1, and every milestone after)

**Frontend parity & DX (the GUI/CLI contract):** one binary, Docker/Ollama-style subcommands. **Every
metric must be reachable in every human surface** — they're all pure views of `core`, so a surface-only
metric is a design smell. At `v0.1.0` that's the **GUI** and **`ps`/`--json`**; the live `top` TUI and the
machine sinks join later and the parity rule binds them too. The CLI is **scriptable, not a toy**: stable
JSON field names, `schema_version`, meaningful exit codes. Adding a metric means updating the model once
and the thin renderers, never duplicating logic.

**Platform & vendor support** — maintained in [`ARCHITECTURE.md`](ARCHITECTURE.md) (Platform support):
- **NVIDIA/NVML, Linux** is the first-class target. DCGM (richer + MIG), macOS/Windows, and AMD/Intel come
  later behind the collector trait — documented, not faked.
- The **mock collector** is a permanent first-class source: every surface must build, test, run, and
  *demo* with **no GPU and no driver present**.

**Testing ladder (through `0.1.0`):**
- Unit (headless, no GPU): `core` model + ring buffers, NVML value maps, view-model transforms,
  `ps --json` shape — table-driven via the mock collector.
- Snapshot tests of view models (what each surface *would* draw/emit) without a window: the GUI view model
  and the `ps --json` golden output.
- Manual smoke on real hardware: the `gui` window on an NVIDIA/Linux box.
- *(Later milestones add the TUI render-buffer, Prometheus exposition golden, and inference e2e rungs.)*

**Performance & overhead budget (keystone 6):** the monitor's own footprint is a tracked metric.
Targets: negligible idle CPU, render-on-change (no busy frame loop), idle when backgrounded, bounded
memory regardless of uptime. Benchmark startup + steady-state; surface our own usage in-app; regressions
are bugs.

**Build/dev graph (committed scope):**
```
M0 (v0.0.0, scaffold) ─▶ M1 (v0.1.0, first public release)
                           └─ the engine feeds GUI + ps; every later metric/surface reuses the same core
```
Linear and short: M0 stands the skeleton up; M1 puts one real metric through it to two surfaces. Beyond
M1 is the [horizon](#beyond-v010--the-horizon-not-planned-here), planned later.

### Scope & where the lines go (an estimate, not a target)
Through **`v0.1.0`** the surface is small: `core` (model + one ring buffer), `collector` (NVML + mock), a
one-panel `ui`, and `ps` in `cli` — on the order of a few thousand lines, most of it reusable by the whole
horizon. (For reference, the *full* north star is a focused native app realistically in the **~25–50k
lines** range at 1.0 — a sizing note, never a goal.) The unit of progress is the milestone tag + demo +
green CI; a smaller line count is a win — the later surfaces all add little *because* they reuse `core`.

---

## Risks & open decisions (through v0.1.0)

The unresolved forks and known landmines **for the committed scope** — each with a **recommended default**
and where it closes. Resolutions are recorded in [`ARCHITECTURE.md`](ARCHITECTURE.md) as they land.

| # | Decision / risk | Recommended default | Close by |
|---|-----------------|---------------------|----------|
| R1 | Snapshot publication (engine → surfaces) | `ArcSwap<Arc<Snapshot>>` + per-surface wake (§0.5) | M1 |
| R5 | wgpu GUI smoke in headless CI | GUI smoke stays **manual**; CI covers `ps`/`top`/tests headlessly; `lavapipe` software-fallback as a smoke only, never a perf gate | M0 / ongoing |
| R8 | The repo/crate **rename off `agent`** to the chosen name is disruptive | one discrete pre-M1 commit, before external eyes | pre-M1 |

> Risks for the horizon (NVML per-process util flakiness, the persistence engine, remote protocol/auth,
> inference metric-name drift, MIG enumeration) are real but **recorded when those milestones are planned**,
> not pre-decided here.

---

## Architectural invariants (never traded away)

- **Headless engine; every surface is a pure view.** GUI, TUI, one-shot CLI, and machine sinks all read
  only `core` — none calls NVML/DCGM/Ollama/Prometheus directly. This keeps the app testable headless,
  demoable without a GPU, scriptable, exportable, and remote-capable. A surface reaching into a data
  source means the design has broken.
- **Two first-class frontends, one binary.** A native GUI *and* a CLI/TUI, shipped together, at parity —
  every metric in all surfaces. The CLI is real (stable `--json`, exit codes), not a toy. *(GUI + `ps` at
  `0.1.0`; the live TUI follows.)*
- **Native, no Electron, single binary.** GUI via `wgpu`, TUI via `ratatui`, fast cold start, statically
  linkable where the driver allows. The craft bar is the reason to build this over a web app.
- **Integrate, don't reimplement (sinks are pure views).** Exporters — Prometheus, OTLP, Splunk
  (post-`0.1.0`) — are machine-facing views of `core`, opt-in and composable with the frontends; the
  monitor never stores, dashboards, or alerts on its own behalf. Plug into the user's stack; don't rebuild it.
- **Local-first, zero-config.** Launch → local GPUs (and any running Ollama/vLLM) visible, no
  server/account/config required. Exporters, remote, and persistence are additive (post-`0.1.0`);
  disabling them leaves a fully useful tool.
- **A monitor must never be a hog.** Sample sanely, render on change, idle when backgrounded, bound
  every buffer, surface our own footprint. The tool that watches usage cannot cause it.
- **Inference awareness is the wedge** — model attribution and serving KPIs (Ollama-first) are why this
  exists; raw hardware counters alone are commodity.
- **Vendor-neutral behind a trait.** NVIDIA/NVML first, but `core`/`ui`/`cli` never assume NVIDIA — a
  new GPU vendor is a new `collector` implementation, nothing else.
- **The mock collector is permanent.** Every build/test/demo path works with no GPU present.
- **SemVer + a git tag per milestone**; releases are signed and reproducible (from `1.0`; informally earlier).
