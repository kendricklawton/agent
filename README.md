# agent *(name TBD)*

**Ask your data questions in plain English.** A modular, open-source engine in **Rust** that lets you plug
in **any LLM** and **any data provider** and query them together — you bring the APIs, the engine makes
them speak the same language and answers with real, cited data.

> **Status:** early, in active development. The engine + mock adapters work today (keyless); the real
> Claude + Polygon adapters are next. No published binary yet — build from source
> ([`CONTRIBUTING.md`](./CONTRIBUTING.md)). The repo is still named `agent` pending a rename.

## Why
Every "ask-your-data" tool is welded to one model and one data source. Swap the LLM or the data provider and
you rewrite everything — and when a provider changes its API, your app breaks in production. This engine
makes both **pluggable** and puts a **stable schema** between you and every provider, so:

- **Bring any model.** Claude, OpenAI, or a local/custom model — behind one `Model` trait.
- **Bring any data.** Polygon for markets, a custom source, prediction markets (e.g. Kalshi) later — behind
  one `DataProvider` trait.
- **Survive API drift.** Providers map their raw API to the engine's **canonical schema**; a provider's
  breaking change is contained to its adapter and caught by tests, not by your users.

## What it does
```
you ask ─▶ [ LLM plans a query ] ─▶ [ data provider returns canonical data ] ─▶ [ LLM answers, grounded ]
```
- **CLI** (`ask`, `ask --json`): a one-shot, scriptable natural-language query over your data.
- **Grounded answers:** the engine answers from the data it actually fetched and says what it used — it's a
  **research/analysis** tool, not financial advice.
- **Runs with no keys:** a built-in **mock** model + mock provider mean it builds, tests, and demos with no
  API keys and no network.

## Usage
```
agent ask "what was NVDA's average close last week?"     # uses your configured model + provider
agent ask "…" --json                                     # structured output for scripts
agent ask "…" --mock                                     # no API keys: mock model + mock data
```

## How it fits together
Every surface is a pure view of one **headless engine**: `model + provider → core → {cli | api}`. A new LLM
or data source is a **new adapter and nothing else** — the core never depends on a specific vendor, and
nothing downstream sees a provider's raw API.

## Layout
```
crates/core       the engine: canonical schema, the Model + DataProvider traits, the query flow
crates/models     LLM adapters (claude, mock, …)
crates/providers  data adapters (polygon, mock, …)
crates/cli        terminal frontend: `ask` (+ `--json`)
crates/app        the single binary: wires model + provider → core → surface
xtask             build orchestration (dev-only)
```

## Open core
The **engine, the adapter SDK, and the reference adapters are open source** (Apache-2.0). Hosted,
multi-source, and team features are additive commercial layers — they build *on* the OSS core, never
replace it.

## Security
API keys live in your environment/config — never committed, never logged. Report vulnerabilities privately;
see [`SECURITY.md`](./SECURITY.md). The notable surfaces are the outbound HTTP calls to your chosen model
and data providers, and parsing their responses.

## License
[Apache-2.0](./LICENSE).

---
**Build it & contribute:** [`CONTRIBUTING.md`](./CONTRIBUTING.md). The invariants and agent guidance live in
[`.rules`](./.rules); the design in [`ARCHITECTURE.md`](./ARCHITECTURE.md); the staged plan in
[`ROADMAP.md`](./ROADMAP.md).
