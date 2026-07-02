# Roadmap — a natural-language data query engine (Rust): pluggable LLM + data provider

Ask a question in plain English, get an answer grounded in real data — by plugging in any LLM and any data
source behind two small traits. This is the staged plan; the *why* is in [`ARCHITECTURE.md`](./ARCHITECTURE.md),
the invariants in [`.rules`](./.rules), the how-to in [`CONTRIBUTING.md`](./CONTRIBUTING.md).

## §0 The spine
The whole engine is one loop across two ports:

**question → `Model` plans a structured query → `DataProvider` returns canonical data → engine computes the
metric → `Model` composes a grounded answer.**

- **`Model`** (an LLM adapter): NL → a structured [`Plan`]; then compose a grounded answer. Claude first;
  OpenAI/local next.
- **`DataProvider`** (a data adapter): declares its capabilities, returns data in the **canonical schema**.
  Polygon first; Kalshi/prediction-markets and custom sources next.
- **The engine** owns the flow and the one piece of real arithmetic (`compute`). Every surface (CLI now,
  API later) is a pure view of the resulting `Answer`.

## §0.5 The engineering contract (what every phase leans on)
1. **Ports & adapters.** A new model or source is a new adapter and nothing else; the core depends on
   neither. Adapters are feature-gated.
2. **Canonical schema = anti-corruption layer.** Providers map raw API → canonical; the engine never sees
   raw. This is what contains provider **API drift**.
3. **Drift caught in CI.** Every adapter has **contract tests over recorded fixtures** (deterministic,
   offline). A provider/LLM contract change fails CI, not production.
4. **Capabilities.** Adapters declare what they can serve; the engine only plans answerable queries.
5. **Grounded, not advice.** Answer from fetched data, report provenance, never invent a number. Research/
   analysis tool, **not** financial advice.
6. **Wire contract.** `ask --json` (and the later API) have stable field names + exit codes — additive
   changes only, golden-tested.
7. **Keyless by default.** A permanent mock model + mock provider → build/test/demo with no keys; the basis
   for known-answer evals.
8. **Secrets out of the repo.** API keys via env/config only; never committed, logged, or in fixtures.
9. **No panics.** `unwrap`/`expect`/`panic!` denied outside tests; a failed model/provider call is a value.

## Phase index
0. Scaffold, CI & release pipeline · 1. Engine skeleton + mock vertical slice · 2. Real LLM (Claude) ·
3. Real data provider (Polygon) · 4. Drift defense (fixtures + contract tests) · 5. More adapters + config ·
6. Richer queries & the formal wire contract · 7. Prediction markets (Kalshi) · 8. Caching & reliability ·
9. Server/API surface (open-core boundary) · 10. Eval suite & quality budgets · 11. Packaging & release ·
12. Python SDK (PyO3/maturin) — *after* the Rust `v0.1.0`.

---

## Phase 0 — Scaffold, CI & release pipeline
- [x] Cargo workspace + crate split (`core`/`models`/`providers`/`cli`/`app`/`xtask`).
- [x] CI: fmt, clippy `-D warnings`, build, test, feature powerset (`cargo-hack`), supply chain (`cargo-deny`),
  a single `ci-status` gate; local mirror via `cargo xtask ci`.
- [x] Scheduled advisory audit; keyless-signed + SBOM'd release pipeline (tag-triggered).

## Phase 1 — Engine skeleton + mock vertical slice ⭐
- [x] The two trait seams (`Model`, `DataProvider`) + the canonical schema (`Bar`, `DataQuery`, `Metric`,
  `Answer`) + `Capabilities` + the pure `compute()`.
- [x] `MockModel` (deterministic NL→`Plan`) + `MockProvider` (known price series).
- [x] `agent ask "…" --mock` → grounded answer; `--json` wire output; clean error on no ticker.
- [x] A **known-answer eval** test (avg close = 101.0). Everything builds/tests offline, gate green.

## Phase 2 — Real LLM adapter (Claude) ⭐
- [ ] A `claude` `Model` adapter (`reqwest`; API key from env) using **tool-calling / structured output** to
  turn NL into a `Plan`, then compose a grounded answer from the computed value + data.
- [ ] Handle rate limits, timeouts, and failures as values (no panics); redact keys from logs/errors.
- [ ] Contract tests over **recorded fixtures** so the adapter is testable with no key/network.
- [ ] Extend the known-answer evals to cover the real plan→answer path (recorded).

## Phase 3 — Real data provider (Polygon)
- [ ] A `polygon` `DataProvider` (`reqwest`; key from env) that maps raw aggregates → canonical `Bar`
  (handle adjusted-vs-unadjusted explicitly), with a `Capabilities` descriptor.
- [ ] Pagination, rate limits, retries with backoff; recorded-fixture contract tests.
- [ ] `agent ask "…"` end-to-end against real Claude + Polygon (opt-in, needs keys).

## Phase 4 — Drift defense
- [ ] A record/replay ("VCR") harness: capture a real response once, replay offline; drift shows as a diff
  and **fails CI**.
- [ ] Capability negotiation: the engine declines (clearly) queries a provider can't serve.
- [ ] Canonical **schema versioning** + additive-evolution rules; a golden for the `ask --json` shape.

## Phase 5 — More adapters + config/registry
- [ ] A second model (OpenAI or a local model) and a second data provider — proving the seams generalize.
- [ ] Select model + provider via config/flags/env; an adapter registry; feature-gate each real adapter.

## Phase 6 — Richer queries & the formal wire contract
- [ ] Grow the canonical schema + query shapes: date ranges, more metrics, option chains, fundamentals.
- [ ] Formalize the wire contract (`schema_version`, `#[non_exhaustive]`, golden-tested) for `ask --json`.

## Phase 7 — Prediction markets (Kalshi) ⭐ (the differentiator)
- [ ] An `Event`/`Market` canonical type with an **implied probability**; a Kalshi `DataProvider`.
- [ ] The signature query: **LLM-estimated probability vs the market-implied probability**, with the
  divergence surfaced and grounded.

## Phase 8 — Caching & reliability
- [ ] Cache expensive provider/LLM calls (in-memory → pluggable); shared retry/timeout/rate-limit middleware.
- [ ] Cost + latency budgets surfaced per answer.

## Phase 9 — Server / API surface (the open-core boundary)
- [ ] An HTTP API (`axum`) rendering the same engine — the boundary the commercial layer builds on.
- [ ] AuthN/Z, config, graceful shutdown; strictly additive to the local CLI.

## Phase 10 — Eval suite & quality
- [ ] A broader **known-answer eval** suite + grounding checks (did the answer use the fetched data?).
- [ ] Regression gates on answer correctness, cost, and latency.

## Phase 11 — Packaging & release ⭐ (ships `v0.1.0`)
- [ ] Packaging (Linux first), install docs, a demo, the `ask --json`/API contract reference.
- [ ] Signed, reproducible release + checksums + SBOM (the Phase-0 pipeline, cutting a real tag).
- [ ] Tag **`v0.1.0`** — the first public release.

## Phase 12 — Python SDK (PyO3/maturin) — *after the Rust `v0.1.0` is done*
The adoption surface for the data/quant/ML audience: `pip install`, `import`, in-process — the same
Rust-core + native-binding pattern as `polars`/`pydantic-core`/`ruff`. Built **after** the Rust version so
it binds a real engine, not a mock.
- [ ] A `crates/py` binding (PyO3, `abi3` → one wheel across Python versions) wrapping `Engine`/`ask`, with
  `Answer` exposed as a typed Python object and engine errors mapped to Python exceptions.
- [ ] `maturin` build + `pyproject.toml`; a **separate CI job** builds/tests the wheel. The pure-Rust gate
  excludes the py crate (`extension-module` feature-gated), so `cargo xtask ci` stays green and keyless.
- [ ] Python-side smoke + contract tests; publish wheels to **PyPI** (trusted publishing, no stored token).
- [ ] Promotes `agent-core`'s `Engine`/`Answer` to a committed, SemVer-tracked surface (note in ARCHITECTURE).

---

## Cross-cutting standards (apply to every phase)
- **Pure where it counts.** Data-prep and compute are pure functions, unit-tested without network — the
  home for evals.
- **`thiserror` in libs, `anyhow` at the app boundary; `#![forbid(unsafe_code)]`; MSRV pinned + CI-guarded.**
- **Newtypes / typed canonical schema across trait boundaries** (a decimal price type replaces `f64` — see
  LEARN-TECHNICAL.md); `#[non_exhaustive]` on public types.
- **Every phase closes with a git tag + a working demo + green CI**, and isn't started until the prior is
  green.

## Risks & open decisions (the dragons)
- **LLM reliability / hallucination.** The grounding + eval discipline (§0.5-5/-6, Phase 10) is the defense;
  a wrong number must be impossible-to-miss, not silent.
- **Provider API drift & licensing.** The anti-corruption layer + fixtures handle drift; respect each
  source's redistribution terms (never re-serve their data).
- **Cost & latency.** Real model/data calls cost money and time — caching (Phase 8) and budgets (Phase 10).
- **Crowded space.** The wedge is craft + the pluggable/keyless/self-hostable core + the Kalshi angle, not
  novelty.

## Architectural invariants (never traded away)
Headless engine, pure-view surfaces · the two trait seams (`Model` + `DataProvider`) · canonical schema as
the drift-defeating anti-corruption layer · capabilities declared, contract-tests-in-CI · grounded-not-advice
· keyless mock-first · secrets out of the repo · stable additive wire contract · one-way open-core boundary.

[`Plan`]: ./crates/core/src/lib.rs
