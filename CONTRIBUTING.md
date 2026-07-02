# Contributing

Thanks for your interest — this is an open-source, modular **natural-language data query engine** in Rust:
ask a question in plain English and get an answer grounded in real data, by plugging in **any LLM** and
**any data provider** behind two small traits.

> Read [**`.rules`**](./.rules) first — the single source of truth for build commands and the invariants
> that must never be traded away (`CLAUDE.md`, `AGENTS.md`, and `GEMINI.md` all point there). The design is
> in [**`ARCHITECTURE.md`**](./ARCHITECTURE.md); the staged plan in [**`ROADMAP.md`**](./ROADMAP.md).
>
> **New to the domain or the stack?** [`LEARN-PRODUCT.md`](./LEARN-PRODUCT.md) is a guided path through the
> financial instruments the engine serves; [`LEARN-TECHNICAL.md`](./LEARN-TECHNICAL.md) maps the Rust / LLM /
> systems skills to this codebase.

## Prerequisites

- **Rust, stable** ([install `rustup`](https://www.rust-lang.org/tools/install)). No nightly, no `sudo`, no
  codegen step.
- For **real answers**: an API key for your chosen LLM (e.g. Claude) and data provider (e.g. Polygon), set
  via **environment variables** — never committed. For **no keys at all**: the built-in **mock** model +
  mock provider are the **keyless default** (`--mock` forces them explicitly), so every command builds,
  runs, tests, and demos offline.

## Quick start

```console
git clone <repo> && cd agent
cargo build

# Ask a question. The mock model + mock data source are the keyless default (no API keys, no network):
cargo run -p agent -- ask "average close of FOO over the last 3 days"
cargo run -p agent -- ask "latest close of FOO" --json          # structured output for scripts (stdout only)
AGENT_LOG=debug cargo run -p agent -- ask "latest close of FOO"  # event-stream logs → stderr; answer → stdout
```

A bare `cargo build` builds the whole workspace. Build one crate with `cargo build -p <crate>` (e.g.
`-p agent-core`). Config is layered **flags > env (`AGENT_*`) > file > defaults**: pick adapters with
`--model`/`--provider` (or `AGENT_MODEL`/a `--config` TOML) — only `mock` resolves today. Real model
adapters land in **Phase 3**, data providers in **Phase 4**, reading their keys from the environment.

## Before you push — the local gate

Run the same checks CI runs, in one shot:

```console
cargo install cargo-deny cargo-hack   # one-time: the gate shells out to both
cargo xtask ci                        # fmt + clippy + build + test + feature powerset + deny, stops at the first failure
```

…or individually:

```console
cargo test --locked                          # offline: mock adapters + recorded fixtures
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --all --check
cargo hack --feature-powerset --no-dev-deps check --workspace   # no --locked: --no-dev-deps rewrites manifests
cargo deny check
```

CI mirrors this on `ubuntu-latest` with stable Rust and **no API keys** — the mock adapters and recorded
fixtures keep the whole pipeline offline and deterministic.

## The testing ladder

Almost everything runs **offline, with no API keys**, via the mock adapters and recorded fixtures; only the
top rung needs live keys.

1. **Unit (pure):** the engine's `compute()`, the tool-use loop (`respond` → `query` tool call → grounded
   answer), adapter parsers/mappers (NL→a `query` tool call, raw API→canonical `Bar`), config precedence,
   and format helpers — pure/table-driven, unit-tested with no network. Property tests (`proptest`) cover
   engine invariants. `cargo test`.
2. **Contract tests (recorded fixtures):** each real adapter replays a captured LLM/provider response, so
   its raw→canonical mapping is deterministic and **API drift fails CI**. Capturing a fixture is the only
   step that touches a live endpoint (done deliberately, curated + license-checked, never auto-committed).
3. **Known-answer evals:** end-to-end questions whose answers are verifiable — assert the engine computes
   *and grounds* them correctly. This is the honesty backstop; it grows every phase.
4. **Live e2e (opt-in, needs keys):** run against a real LLM + data provider to sanity-check the whole loop.

## The no-panic discipline

`unwrap`, `expect`, and `panic!` are **`deny`-lints outside tests** (re-allowed in tests via `clippy.toml`).
A failed model call, an unreachable provider, or an unmappable response is a **value** (`Err(...)`) that
degrades to a clear message — never a crash.

## Secrets

API keys come from the **environment/config only**. Never commit, log, or embed them; never put a real key
or real fetched data in a recorded fixture. If you add an adapter, read its key via env and redact it from
any error/log output.

## Grounded, not advice

The engine answers from data it actually fetched and reports what it used. It is a **research/analysis**
tool, not financial advice, and it never fabricates numbers — keep it that way.

## Phases & decisions

Work is organized into phases (Phase 0 → Phase 14) that ladder to the `v0.1.0` release at Phase 14 (see
[`ROADMAP.md`](./ROADMAP.md)). Each phase closes with a **git tag + a working demo + green CI**, and isn't
started until the prior is green. Record any significant, hard-to-reverse decision in
[`ARCHITECTURE.md`](./ARCHITECTURE.md) (Design decisions) so the *why* outlives the diff.

## Commit & PR conventions

- One logical change per commit; imperative subject ("Add the Claude model adapter", not "added").
- **Never add AI co-author or attribution trailers**; never commit secrets or fetched/generated data.
- A new LLM or data source is a new **adapter behind a trait** — never a special case in the core.
- Every PR must pass the full gate (`cargo xtask ci`).

## License

By contributing you agree your contributions are licensed under **Apache-2.0**, the project's license.
