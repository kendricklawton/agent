# Architecture

The design of the engine and the decisions behind it. The staged plan is in [`ROADMAP.md`](./ROADMAP.md);
the build commands + invariants in [`.rules`](./.rules); how to build and contribute in
[`CONTRIBUTING.md`](./CONTRIBUTING.md).

## The shape — one headless engine, two adapter seams, pure-view surfaces

This is **ports & adapters** (hexagonal architecture). A headless **engine** (`core`) answers questions by
driving two *ports* — a [`Model`] (an LLM) and a [`DataProvider`] (a data source) — and every **surface**
(the CLI and the Python SDK now, an API later) only *renders* the engine's answer. No surface calls an LLM
or a data provider directly. The **canonical schema** in the middle is a Domain-Driven-Design *anti-corruption layer*: every
provider maps its raw API onto it, so a provider's wire format — and its API drift — never leaks inward.

```
   adapters (ports)              core (the headless engine)          surfaces
 ┌──────────────────────┐  plan ┌───────────────────────────┐ read ┌──────────────────────┐
 │ Model:  claude/…/mock │◀────▶ │  plan → fetch → compute →  │ ────▶ │ CLI  ask (+ --json)   │
 │ Data:   polygon/…/mock│◀────▶ │  ground; canonical schema  │ ────▶ │ API  (later)          │
 └──────────────────────┘ fetch └───────────────────────────┘       └──────────────────────┘
   raw APIs mapped to the       the engine only ever sees the        every surface renders the
   canonical schema here        canonical schema (drift contained)   engine's grounded Answer
```

The flow is one-directional: **question → `Model` plans a structured query → `DataProvider` returns
canonical data → engine computes the metric → `Model` grounds the answer**. A new LLM or data source is a
new adapter and **nothing else** — the core depends on neither a specific model nor a specific provider.

## The crates

| Crate | Role |
|-------|------|
| `crates/core` | The engine: the `Model` + `DataProvider` traits, the canonical schema (`Bar`, `DataQuery`, `Metric`, `Answer`), `Capabilities`, and the pure `compute()`. **The spine.** |
| `crates/models` | LLM adapters behind `Model`: `mock` now; `claude` + `openai` + `gemini` at launch; local next. |
| `crates/providers` | Data adapters behind `DataProvider`: `mock` now; `polygon` + `yahoo` at launch; `kalshi`/custom next. |
| `crates/cli` | Terminal frontend (lib): the one-shot `ask` (+ `--json`). |
| `crates/app` | The single binary: subcommand dispatch, wires a model + a provider into the engine. |
| `xtask` | Build/dev orchestration (dev-only, never shipped). |

## Design decisions (the *why*)

The load-bearing, hard-to-reverse choices. Record new ones here when you make them.

1. **Two trait seams — `Model` and `DataProvider` (ports & adapters).** The engine drives two ports; every
   adapter is swappable. *Why:* pluggable models/sources, testable headless, vendor-neutral. *Rejected:* a
   surface calling an LLM or provider directly (untestable, coupled, un-swappable).
2. **A canonical, versioned schema is the anti-corruption layer.** Providers map their raw API → the
   engine's canonical types; the engine never sees raw. *Why:* provider APIs differ and change — contain
   the blast radius to one adapter, and catch drift with per-adapter **contract tests over recorded
   fixtures** in CI. *Rejected:* passing provider-native shapes through the core.
3. **Adapters declare capabilities.** Each `DataProvider` states what it can serve; the engine only plans
   queries a provider can answer. *Why:* fail fast and clearly, not deep in a fetch.
4. **Grounded answers, not advice.** The engine answers from the data it fetched and reports the provenance
   (model, provider, value, bars used); it never invents numbers. *Why:* a wrong number is worse than "no
   data" — this is a **research/analysis** tool, not financial advice.
5. **Mock-first, keyless by default.** A permanent mock model + mock provider make everything build, test,
   and demo with no API keys, and are the basis for deterministic **known-answer evals**. *Why:* fast,
   offline, reproducible; the same discipline that keeps CI green without secrets.
6. **Headless engine, pure-view surfaces.** The CLI (and any future API) render the engine's `Answer` and
   nothing else. *Why:* one place to test, one place the trust/parsing surface lives.
7. **LLM via tool-calling / structured output.** The `Model` turns NL into a *structured* `Plan` (a tool
   call), not free text the engine has to parse. *Why:* reliability and testability of the NL→query step.
8. **Open-core, one-way dependency.** The engine, the adapter SDK, and the reference adapters are OSS;
   hosted/multi-source/team features build *on* the core and never leak back into it. *Why:* the core stays
   clean and self-hostable; we sell the layer above it, not a crippled core.
9. **Release pipeline from day one; keyless signing + SBOM.** A tag-triggered workflow builds from the
   committed `Cargo.lock` (`--locked`), checksums, generates an SBOM, and **keyless-signs** (sigstore
   `cosign` over GitHub OIDC — no long-lived key), then verifies before publishing. *Why:* exercise the
   whole packaging/signing path before there's anything to ship.
10. **One tool-use loop, not a split `plan`/`answer`; the provider is a tool the engine runs.** *(Landed,
    Phase 2.)* The `Model` seam collapses to a single `respond(conversation, tools) -> Step` step: each turn
    the model either asks to run tools (`Step::UseTools`) or emits its final answer (`Step::Done`) — the same
    shape as the real Messages API. "Planning" is just the model choosing to call the `query` tool; the split
    `plan()`/`answer()` methods disappear. Crucially the **engine runs the tools, not the LLM**: the model
    asks for a metric over a ticker's bars, but the `DataProvider` fetch and the `compute()` arithmetic are
    executed by the engine and fed back as a trustworthy tool result. *Why:* (a) it preserves
    **grounded-not-advice** — the LLM never does the math or invents a number; (b) a turn becomes a
    *sequence of messages*, so a `Conversation` is the unit of work and **multi-turn + session-resume**
    (Phase 5) fall out for free — the conversation serializes to JSONL, no database; (c) it matches the
    provider tool-calling contract, so the Claude adapter (Phase 3) wraps it directly. *Cost:* the seam
    signatures change from `question: &str` to a `Conversation` — a breaking reshape done deliberately while
    **mock-only and pre-freeze**, because doing it after a live adapter is far more expensive. *Rejected:*
    keeping `plan`/`answer` (can't express tool loops or multi-turn) and letting the LLM compute (breaks
    grounding). Streaming (`Step::Done` as a token delta stream) and the async boxing strategy for
    `Box<dyn Model>` layer on top of this same loop — see the async + streaming invariant.
    - *Async boxing (landed first, Phase 2):* the seams are `async` via **`async-trait`**, chosen because
      the engine holds `Box<dyn Model>`/`Box<dyn DataProvider>` for runtime adapter selection (12-factor
      backing services) and native `async fn` in traits isn't `dyn`-compatible. *Rejected:* generic
      `Engine<M, P>` + native async fn (kills runtime selection); hand-rolled `Pin<Box<dyn Future>>` (more
      boilerplate, no benefit — the per-call box is negligible next to an HTTP round-trip). The async
      substrate landed **before** this loop reshape so the two hard changes stay independently bisectable.
    - *Loop, as shipped (Phase 2):* **JSON-valued tool calls** (`ToolCall { id, name, input: Value }` /
      `ToolResult`), not a typed enum — the exact shape Claude/OpenAI/Gemini `tool_use` blocks parse into, so
      the frozen seam survives richer tools. **One `query` tool** now (fetch + `compute`, atomic and
      grounded); granular fetch/compute tools come with the Phase-9 query model. A **step budget** and an
      **ungrounded-answer guard** (a `Done` with no executed `query` is refused) enforce termination and
      honesty.
    - *Streaming (landed, Phase 2):* `Step::Done` carries a **`Stream` of text deltas** (freeze-safe — a real
      LLM streams SSE tokens and can't return a whole `String` without buffering). `Engine::ask` takes a
      **`&mut dyn TokenSink`** and pushes each delta to the surface as it consumes the stream, still returning
      the full `Answer`. *Why a push sink over `ask -> impl Stream`:* object-safe over `Box<dyn Model>`, no
      `&mut self` borrow entanglement, and it bridges cleanly to a Python callback (SDK) or an SSE channel
      (server). The human CLI streams to stdout; `--json` uses a `NullSink` and stays atomic.
11. **Config is layered and adapter selection is config, not code.** *(Landed, Phase 2.)* One `Config`
    resolved **flags > env (`AGENT_*`) > file (TOML) > defaults**, with IO split from logic (a pure
    `resolve` fold, unit-tested for precedence; an impure `load` that reads env + an explicit-path file).
    Which model/provider runs is a **name resolved to an adapter** in the app (`build_model`/`build_provider`),
    so a new adapter registers in one place — 12-factor backing services. *Why:* every real adapter needs
    keys + selection from the environment, and doing it now (mock-only) means Phase-3 adapters just add a
    match arm. *Rejected:* `figment`/`config-rs` (heavier trees, license-surface risk against the
    MIT/Apache-2.0/Unicode-3.0 allow-list) — hand-rolled `toml` + serde is a few lines and testable.
    **Logs are an event stream:** `tracing` events (the engine emits them) render via `tracing-subscriber`
    to **stderr**; **stdout is reserved** for the answer / `--json`, so `agent ask … 2>/dev/null` is
    pipe-clean. Secrets come from env only — never the `Config` struct or the file.

## Platform & trust surface

Unprivileged userspace — no root. The engine's real-world touchpoints are **outbound HTTP** to your chosen
LLM and data providers (using **your API keys, from env/config only**) and **parsing their responses**.

- **Rust, stable**, one host target (Linux `x86_64` first; others follow the same pure-Rust build).
- **Models:** any LLM behind the `Model` trait — Claude, OpenAI, and Gemini at launch; local next.
- **Data:** any source behind `DataProvider` — Polygon (licensed) and Yahoo Finance (keyless, **unofficial**)
  at launch; Kalshi/prediction markets and custom next.
- **Secrets** never touch the repo, logs, or fixtures. The **mock** adapters need no keys and no network.

## Extension model & non-goals

- **Extend via the two traits + the wire contract, not an embeddable API.** The crates are library-shaped,
  but the *committed* extension surface is the **`Model` and `DataProvider` traits** plus the **`ask`
  wire contract** (and the API later) — not `agent-core`'s Rust API, which is not SemVer-guaranteed pre-1.0.
  A new model or source is an adapter; a consumer reads the wire contract in any language.
- **Non-goals.** Not a trading bot and not financial advice — it *analyzes* and *cites*, it doesn't
  recommend trades. Not a market-data vendor — it reads sources you're licensed for and never redistributes
  their data. Not a dashboard or a time-series database. It is open-core and self-hostable; the commercial
  layer is additive.

## Invariants

The non-negotiables live in [`.rules`](./.rules) (Invariants) and the
[Architectural invariants](./ROADMAP.md) section of the roadmap. In short: the headless-engine/pure-view
split, the two trait seams, the canonical schema as the drift-defeating anti-corruption layer, capability
descriptors, contract-tests-in-CI, grounded-not-advice, mock-first/keyless, secrets-out-of-repo, and the
one-way open-core boundary.

[`Model`]: ./crates/core/src/lib.rs
[`DataProvider`]: ./crates/core/src/lib.rs
