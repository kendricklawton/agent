# Roadmap — a natural-language data query engine (Rust): pluggable LLM + data provider

Ask a question in plain English, get an answer grounded in real data — by plugging in any LLM and any data
source behind two small traits. Two first-class surfaces read the engine at parity: a **Claude-Code-grade
CLI** and a **native Python SDK**. This is the staged plan; the *why* is in
[`ARCHITECTURE.md`](./ARCHITECTURE.md), the invariants in [`.rules`](./.rules), the how-to in
[`CONTRIBUTING.md`](./CONTRIBUTING.md).

## §0 The spine
The whole engine is one **async, streaming** loop across two ports:

**question → `Model` runs a tool-use loop (plans a query, the engine executes it against the
`DataProvider`, canonical data comes back) → the engine composes and streams a grounded answer.**

- **`Model`** (an LLM adapter): drives a **tool-use loop** — it calls the engine's query tool and, once it
  has the canonical data, signals it's done; the **engine** composes and streams the grounded answer (so the
  figure can't drift from the data). **Claude, OpenAI, and Gemini ship at launch** (Claude is the reference
  that shapes the loop); local models next.
- **`DataProvider`** (a data adapter): declares its capabilities, returns data in the **canonical schema**.
  **Polygon and Yahoo Finance ship at launch** (Polygon licensed/reference; Yahoo keyless but unofficial);
  Kalshi/prediction-markets and custom sources next.
- **The engine** owns the loop, executes the query tool (the deterministic `compute`), and publishes an
  `Answer` (+ a token stream). Every surface — **CLI**, **Python SDK**, and the later **API** — is a pure
  view of the same engine.

## §0.5 The engineering contract (what every phase leans on)
1. **Ports & adapters.** A new model or source is a new adapter and nothing else; the core depends on
   neither. Adapters are feature-gated.
2. **Async + streaming.** The seams are `async` (`tokio`); object-safe via boxed futures/streams. Answers
   **stream** token-by-token to every surface — the difference between "feels like Claude" and a spinner.
3. **Canonical schema = anti-corruption layer.** Providers map raw API → canonical; the engine never sees
   raw. This contains provider **API drift**. Prices are a **decimal** type, timestamps are UTC.
4. **Drift caught in CI.** Every adapter has **contract tests over recorded fixtures** (deterministic,
   offline). A provider/LLM contract change fails CI, not production.
5. **Grounded, not advice.** Answer from fetched data, report provenance, never invent a number; a
   **grounding check** (did the answer use the data?) ships with the first real LLM, not at the end.
   Research/analysis tool, **not** financial advice.
6. **Two surfaces at parity.** Every engine capability is reachable from **both** the CLI and the Python
   SDK — never a CLI-only or SDK-only feature. Both are pure views; the wire/API contract is the third.
7. **Wire contract.** `ask --json`, the SDK types, and the later API share stable field names + exit codes —
   additive changes only, golden-tested.
8. **Keyless by default.** A permanent mock model + mock provider → build/test/demo with no keys; the basis
   for known-answer evals.
9. **No panics.** `unwrap`/`expect`/`panic!` denied outside tests; a failed model/provider call is a value.

## §0.6 Twelve-factor mapping (all twelve — how the engine stays operable)
Each factor maps to something concrete in this plan; the ✓ ones are already in the tree.
- **I Codebase.** ✓ One Cargo **workspace** in git → many deploys (the binary, the Python wheels, the
  server) from the same commit; no per-deploy forks.
- **II Dependencies.** ✓ Explicitly declared in `Cargo.toml`; builds are **`--locked`** from the committed
  `Cargo.lock`; **`cargo-deny`** gates the tree; each real adapter is **feature-gated**. No implicit system libs.
- **III Config.** ✓ Layered precedence **flags > env (`AGENT_*`) > file (TOML) > defaults**; model/provider
  selection and **API keys are config, never code**. **Secrets come from env only** — never committed,
  logged, or placed in a fixture.
- **IV Backing services.** The LLM and the data source *are* attached resources — the adapters. Swap
  Claude→OpenAI or Polygon→Yahoo by config, no code change.
- **V Build, release, run.** ✓ Separate stages: `cargo xtask ci` + the tag-triggered pipeline produce an
  **immutable, signed artifact** (checksums + SBOM); *run* just executes it with config — nothing is built
  at run time.
- **VI Processes.** **Stateless**; conversation/session state lives in a **store** (disk locally, a backing
  store for the server), not process memory.
- **VII Port binding.** The server (Phase 12) is self-contained (`axum`) and **binds a port** — no injected
  webserver.
- **VIII Concurrency.** Scale **out** via the stateless process model (the server runs horizontally); scale
  **in** via async fan-out of provider/LLM calls (`tokio`).
- **IX Disposability.** Fast start, **graceful shutdown** (the server drains in-flight; the CLI is inherently
  disposable); a crash loses nothing because state is external.
- **X Dev/prod parity.** ✓ The **mock** adapters keep dev ≈ prod — same engine, same code path, keyless —
  and the same `--locked` build runs everywhere.
- **XI Logs.** ✓ Structured `tracing` events to **stderr** (never a log file); **stdout is reserved** for the
  answer / `--json`.
- **XII Admin processes.** One-off tasks are **`xtask`** subcommands (CI, fixture capture) run against the
  same codebase + deps.

## Phase index
0. Scaffold, CI & release · 1. Engine skeleton + mock slice · 2. Async · streaming · config foundation ·
3. Real LLMs (Claude · OpenAI · Gemini) + evals · 4. Real data (Polygon + Yahoo) + schema correctness ·
5. Interactive CLI (the Claude-Code feel) · 6. Python SDK (first-class, co-shipped) · 7. Drift defense ·
8. Adapter registry + local models · 9. Richer queries & the computation model · 10. Prediction markets
(Kalshi) · 11. Caching & reliability ·
12. Server / API surface · 13. Eval suite & quality budgets · 14. Packaging & release — ships `v0.1.0`
(binary **+ wheels**).

---

## Phase 0 — Scaffold, CI & release pipeline
- [x] Cargo workspace + crate split (`core`/`models`/`providers`/`cli`/`app`/`xtask`).
- [x] CI gate (fmt, clippy `-D warnings`, build, test, feature powerset, `cargo-deny`, `ci-status`); local
  mirror via `cargo xtask ci`; scheduled advisory audit; keyless-signed + SBOM'd release pipeline.

## Phase 1 — Engine skeleton + mock vertical slice ⭐
- [x] The two trait seams + canonical schema (`Bar`/`DataQuery`/`Metric`/`Answer`) + `Capabilities` + pure
  `compute()`; `MockModel` + `MockProvider`; `agent ask --mock` (+ `--json`); a known-answer eval. Green.

## Phase 2 — Async · streaming · config foundation ✅ (architecture hardening — before any real I/O)
Did the irreversible shape decisions *before* the first HTTP adapter, so streaming, async, and config didn't
have to be retrofitted through the SDK and API later. **Complete.**
- [x] Make the seams **async** (`tokio`), object-safe via **`async-trait`** (the engine holds
  `Box<dyn Model>`/`Box<dyn DataProvider>` for runtime adapter selection); `Engine::ask` is `async`. Gate
  green, wire contract unchanged.
- [x] Reshape `Model` around a **tool-use loop** — `respond(conversation, tools) -> Step`; the engine runs a
  single `query` tool (fetch + `compute`) so grounding is structural. Replaces `plan()`/`answer()`; tool
  calls are JSON (`ToolCall`/`ToolResult`), a step budget + ungrounded-answer guard enforce termination and
  honesty. Surfaces + wire contract unchanged, gate green.
- [x] **12-factor config** (§0.6): a layered `Config` resolved **flags > env (`AGENT_*`) > file (TOML) >
  defaults**; model/provider selection is config (a name → adapter registry in the app). `tracing` logs to
  **stderr** with stdout reserved for the answer/`--json`. Gate green.
- [x] **Streaming + engine-authored answers**: `Step::Done` is a *signal* — the **engine** composes the
  grounded sentence from provenance (the model chooses the query, never the number, so the figure can't drift
  from `Answer.value`) and streams it via `Engine::ask(question, &mut dyn TokenSink)`; the `Answer` also gains
  the instrument (`symbol` + `window_days`) for auditability. The CLI streams tokens to stdout (flushing
  each); `--json` stays atomic via a `NullSink`. Gate green, keyless.

## Phase 3 — Real LLM adapters (Claude · OpenAI · Gemini) ⭐ + evals in-phase
Three real LLMs at launch, not one — the true test that the tool-use loop seam is right **before** it freezes.
- [ ] A `claude` `Model` adapter first (`reqwest`, streaming, tool-use; key from `AGENT_*` env) — the
  reference that shapes the loop. Rate limits, timeouts, failures as values; keys redacted from logs/errors.
- [ ] Then `openai` and `gemini` adapters over the **same** `respond()` loop, each mapping its own
  tool-calling shape (Anthropic `tool_use` · OpenAI `tool_calls` · Gemini `functionCall`) → the canonical
  `Step`. If the seam can't absorb all three, fix it now — mock-only cost, pre-freeze.
- [ ] Contract tests over **recorded fixtures** (offline, per adapter) **and** a **grounding check** — assert
  the answer actually used the fetched data — so hallucination is caught from day one, not at Phase 13.
- [ ] Known-answer evals extended to the real streamed tool-use path (recorded) for all three.lets do a review of our implementation of the following tasks > [@ROADMAP.md (85:109)](file:///home/k-henry/repos/agent/ROADMAP.md#L85:109) Lets ensure the task make sense up to this point and then review the code

## Phase 4 — Real data providers (Polygon + Yahoo Finance) + schema correctness
Two real sources at launch: Polygon (licensed reference) and Yahoo Finance (**keyless** — real data with no
signup, the best first-run demo).
- [ ] A `polygon` `DataProvider` (`reqwest`; key from env) mapping raw aggregates → canonical `Bar` — the
  licensed reference source, with clean, license-checked fixtures.
- [ ] A `yahoo` `DataProvider` over the **unofficial** `query1.finance.yahoo.com` endpoint (**no key**).
  Treat it as best-effort: **synthetic, hand-authored fixtures** matching its documented JSON shape (never
  commit captured Yahoo data — redistribution/ToS), live path **opt-in**, "unofficial" stated in its adapter
  docs. Its volatility is exactly what the anti-corruption layer + contract tests exist to absorb.
- [ ] **Get the data correct** (the #1 bug class — see LEARN-PRODUCT.md): **decimal** prices, explicit
  **adjusted-vs-unadjusted**, symbology, trading-calendar/timezone handling; a `Capabilities` descriptor per
  provider (they differ — e.g. adjusted-close availability).
- [ ] Pagination, retries with backoff; recorded-fixture contract tests; real end-to-end (opt-in, needs keys).

## Phase 5 — Interactive CLI: the Claude-Code feel ⭐
Not one-shot `ask` bolted on — a first-class interactive experience.
- [ ] `agent chat` — a **streaming REPL** over a bounded, multi-turn `Conversation` (each turn keeps the
  question + grounded answer + data used); follow-ups resolve against prior turns.
- [ ] **TTY-aware rendering** (rich/colored interactive; plain/`--json` when piped — like `claude -p`);
  status while fetching/reasoning; **sessions** persisted + resumable (`--continue`/`--resume`, `--list`).
- [ ] In-REPL **slash commands** (`/help`, `/model`, `/clear`, …); actionable errors + exit codes; a
  `config` command. `ask` remains the scriptable one-shot form.

## Phase 6 — Python SDK: first-class, co-shipped ⭐
The adoption surface for the data/quant/ML audience — introduced now (engine is real + streaming), then held
at **CLI↔SDK parity** (§0.5-6) forever after; ships with `v0.1.0`, not after.
- [ ] A `crates/py` binding (PyO3, `abi3` → one wheel across versions) wrapping `Engine`: typed `Answer`,
  errors → Python exceptions, and **async + streaming** exposed Pythonically (async iteration over tokens).
- [ ] `maturin` + `pyproject.toml`; a **separate CI job** builds/tests the wheel (the pure-Rust gate excludes
  the py crate via `extension-module` feature-gating, staying green + keyless).
- [ ] Python smoke + contract tests mirroring the CLI's; promotes `Engine`/`Answer` to a committed,
  SemVer-tracked surface (recorded in ARCHITECTURE).

## Phase 7 — Drift defense
- [ ] A record/replay ("VCR") harness (`xtask` fixture capture); drift shows as a diff and **fails CI**.
- [ ] Capability negotiation: the engine declines (clearly) queries a provider can't serve.
- [ ] Canonical **schema versioning** + additive-evolution rules; goldens for `ask --json` and the SDK types.

## Phase 8 — Adapter registry + local models
- [ ] The seams are already proven across three LLMs + two providers (Phases 3–4); here add a **local /
  self-hosted model** behind the same `respond()` loop — the offline, no-vendor option.
- [ ] Adapter registry; select model + provider via config (§0.6); feature-gate each real adapter.

## Phase 9 — Richer queries & the computation model
- [ ] **Decide the computation model** (the open-ended-question landmine): a small **query/expression DSL**
  the LLM targets (composable metrics over the canonical schema), not an ever-growing `Metric` enum — or a
  deliberately scoped set, stated plainly. This is what lets "ask anything" actually mean something.
- [ ] Grow the schema + query shapes (ranges, option chains, fundamentals); formalize the wire contract.

## Phase 10 — Prediction markets (Kalshi) ⭐ (the differentiator)
- [ ] An `Event`/`Market` canonical type with an **implied probability**; a Kalshi `DataProvider`.
- [ ] The signature query: **LLM-estimated probability vs the market-implied probability**, grounded.

## Phase 11 — Caching & reliability
- [ ] Cache expensive provider/LLM calls (a **backing service**, §0.6-IV; in-memory → pluggable); shared
  retry/timeout/rate-limit middleware. Cost + latency budgets surfaced per answer.
- [ ] **Respect data-source redistribution terms** in what the cache stores and what any surface re-emits.

## Phase 12 — Server / API surface (the open-core boundary)
- [ ] An HTTP API (`axum`) rendering the same engine, with **streaming** (SSE) responses — CLI, SDK, and API
  now three pure views. **Stateless** processes; sessions in a backing store (§0.6-VI); **port-bound**,
  graceful shutdown (§0.6-VII/IX). AuthN/Z, config; strictly additive to the local CLI.

## Phase 13 — Eval suite & quality budgets
- [ ] Broaden the known-answer + grounding evals into a suite; **regression gates** on answer correctness,
  cost, and latency in CI.

## Phase 14 — Packaging & release ⭐ (ships `v0.1.0`)
- [ ] Ship the **binary and the Python wheels together** (co-first-class); install docs, a demo, the
  `ask --json`/SDK/API contract reference.
- [ ] Signed, reproducible release + checksums + SBOM (the Phase-0 pipeline); wheels to **PyPI** (trusted
  publishing). Tag **`v0.1.0`** — the first public release.

---

## Cross-cutting standards (apply to every phase)
- **Async, streaming, non-panicking.** `tokio` I/O; answers stream; errors are values (`thiserror` in libs,
  `anyhow` at the app; `#![forbid(unsafe_code)]`).
- **Twelve-factor throughout** (§0.6, all twelve): env-layered config + secrets-from-env, `--locked`
  declared deps, backing-service adapters, stateless processes, separate build/release/run, `tracing` to
  stderr / stdout for output, **dev/prod parity** via the mock adapters.
- **Two surfaces at parity.** A capability isn't done until it's in **both** the CLI and the Python SDK.
- **Typed canonical schema across boundaries** (decimal prices; `#[non_exhaustive]`); MSRV pinned + CI-guarded.
- **Every phase closes with a git tag + a working demo + green CI**, and isn't started until the prior is green.

## Risks & open decisions (the dragons)
- **Open-ended questions vs a fixed compute vocabulary.** Addressed by the Phase-9 computation model
  (query DSL) — the alternative (an endless `Metric` enum) never covers what users ask.
- **LLM reliability / hallucination.** Grounding check + evals ship **with the first real LLM** (Phase 3),
  not at the end; a wrong number must be impossible-to-miss.
- **Sync→async / streaming retrofit.** Defused up front in Phase 2, before adapters/SDK/API harden.
- **Provider API drift & *licensing*.** The anti-corruption layer + fixtures handle drift; redistribution
  terms are a **business-model gate** on caching (11) and the API (12), not a footnote — read the TOS early.
  **Yahoo Finance is unofficial** (no supported API, ToS gray area): keyless and great for onboarding, but
  fixtures are **synthetic-only**, the live path is **opt-in**, and we never redistribute its data.
- **Cost & latency.** Caching (11) + per-answer budgets (11/13).
- **Crowded space.** The wedge is craft + Claude-Code-grade DX + first-class SDK + the pluggable/keyless
  self-hostable core + the Kalshi angle.

## Architectural invariants (never traded away)
Headless engine, pure-view surfaces · **CLI and Python SDK at parity** · the two trait seams
(`Model` + `DataProvider`) · **async + streaming** seams · canonical schema (decimal, UTC) as the
drift-defeating anti-corruption layer · capabilities declared, contract-tests-in-CI · grounded-not-advice
(grounding check in CI) · **twelve-factor throughout** (all twelve; §0.6) · keyless mock-first · secrets out
of the repo · stable additive wire/SDK contract · one-way open-core boundary.

[`Plan`]: ./crates/core/src/lib.rs
