# LEARN-TECHNICAL.md — mastering the tech to build this

A personal roadmap for the **engineering** skills this engine demands. Ordered by the critical path: what
you need to ship a first version is up top; scale/ops is lower. Each tier says **what to learn**, **why it
matters here** (mapped to the actual codebase), and **where to learn it**.

> **The architecture you're building has a name:** **ports & adapters** (a.k.a. *hexagonal architecture*),
> with the canonical schema as a Domain-Driven-Design **anti-corruption layer**. The `Model` and
> `DataProvider` traits are the *ports*; the claude/polygon/mock crates are the *adapters*; the canonical
> schema stops any provider's raw API from corrupting the core. Learning the theory behind what you're
> already doing will 5x your judgment.

---

## Tier 0 — Rust, deeply ⭐ (everything rides on this)
- [ ] Ownership, borrowing, lifetimes; when to clone vs borrow
- [ ] **Traits & trait objects** (`dyn Trait`), generics, associated types, `impl Trait`
- [ ] Error handling: `Result`, `?`, **`thiserror`** (libs) + **`anyhow`** (app boundary)
- [ ] Modules, crates, **workspaces**, feature flags
- [ ] `serde` (Serialize/Deserialize) — you will live in this
- [ ] Iterators, closures, `Option`/combinators; newtypes; `#[non_exhaustive]`
- [ ] Testing: unit, integration, `#[cfg(test)]`, and property tests (`proptest`)

**Why here:** the two trait seams (`Model`, `DataProvider`), the typed canonical schema, and the no-panic
error model *are* the product. This is 60% of the skill.
**Learn:** **The Rust Book** (free) → **Jon Gjengset's *Rust for Rustaceans*** + his YouTube → the API you
build here is the practice.

## Tier 1 — Async & concurrency
- [ ] `async`/`await`, futures, the **tokio** runtime
- [ ] Concurrency: `join!`/`select!`, spawning, channels, `Arc`/`Mutex`
- [ ] Streams (for live/websocket data); timeouts, cancellation, backpressure
- [ ] Retries with exponential backoff + jitter; rate limiting

**Why here:** every adapter is network I/O; you'll fan out concurrent provider calls, stream live quotes,
and must survive slow/flaky APIs without blocking. This is where "reliable" is won or lost.
**Learn:** the **Tokio tutorial** (excellent); `tower` for middleware (retry/rate-limit/timeout) patterns.

## Tier 2 — Serialization & the canonical schema
- [ ] `serde` in depth: `#[serde(rename/default/flatten)]`, enums, custom (de)serializers
- [ ] Designing a **stable, versioned schema**; additive-only evolution; schema/wire versioning
- [ ] The **anti-corruption layer**: mapping raw provider JSON → canonical types
- [ ] Time & money done right: UTC timestamps, `chrono`/`time`, decimals not `f64` for prices (`rust_decimal`)

**Why here:** the canonical schema is your drift defense. Prices in `f64` and naive timestamps are classic
finance bugs — model them correctly once.
**Learn:** serde docs; read Polygon+Kalshi JSON and practice mapping both to *one* `Bar`/`Quote` type.

## Tier 3 — API integration & the adapter pattern ⭐
- [ ] HTTP with **`reqwest`**; REST; JSON; status/error handling
- [ ] Auth: API keys, headers, OAuth; **secrets from env/config, never committed**
- [ ] Pagination, rate limits, retries; **streaming** (SSE/websockets) for live data
- [ ] **Contract testing + recorded fixtures** (VCR-style): capture real responses once, replay offline so
      tests are deterministic and **API drift fails CI**
- [ ] Capability descriptors: adapters advertise what they can serve

**Why here:** this *is* `crates/providers` and `crates/models`. Recorded-fixture contract tests are exactly
how you deliver on "drift caught in CI, not prod."
**Learn:** reqwest docs; look at how `wiremock`/VCR libraries record & replay HTTP for tests.

## Tier 4 — LLM integration ⭐ (the other half of the product)
- [ ] How chat LLM APIs work: messages, system prompts, streaming, tokens, cost/latency
- [ ] **Tool / function calling** + **structured output (JSON mode)** — how you turn NL → a *structured
      query* the engine can execute against a provider
- [ ] **Grounding & citations**: answer only from fetched data; detect/avoid hallucination
- [ ] **Evals**: how do you *know* an answer is right? build a small eval harness
- [ ] Prompt engineering fundamentals; RAG basics (retrieve → ground → answer)

**Why here:** the `Model` trait wraps this. The core trick of the whole engine — NL question → structured
query → grounded answer — is LLM tool-calling. Evals are how you keep it honest.
**Learn:** the **Anthropic docs** (tool use, structured output) and OpenAI's function-calling guide; build
the mock `Model` first so you can develop the flow with zero API cost.

## Tier 5 — Software engineering & testing
- [ ] Test pyramid: unit → integration → **golden/contract** → property (`proptest`)
- [ ] **Wire contracts & SemVer**: `ask --json` is an API — additive changes only, golden-tested
- [ ] CI/CD: fmt, clippy `-D warnings`, test, **feature powerset** (`cargo-hack`), supply chain (`cargo-deny`)
- [ ] Observability: structured logging + **`tracing`**; metrics; never log secrets
- [ ] Docs, changelogs, release signing/SBOM (you already built this once — reuse it)

**Why here:** this is what makes it *production-grade* vs a toy — and it's the signal that gets you hired.
You already have a strong gate (`cargo xtask ci`); keep the bar.
**Learn:** **Luca Palmieri — *Zero To Production in Rust*** — near-perfect for this project (builds a real
Rust API with reqwest, testing, CI, tracing, config).

## Tier 6 — Distributed systems & scale (be honest about when)
- [ ] Fundamentals: statelessness, idempotency, caching, retries, timeouts, backpressure, load balancing
- [ ] Consistency & the CAP theorem; queues; eventual consistency; sharding basics
- [ ] Building an HTTP service in Rust (**`axum`**); connection pools; graceful shutdown
- [ ] Caching layers (in-memory → Redis) for expensive provider/LLM calls

**Why here — honestly:** a single-binary query engine is *not* inherently a distributed system. This tier
pays off in **(a)** the concurrency/reliability of your adapter calls and **(b)** the **hosted/commercial**
layer (serving many users, caching, rate-limit budgets). Learn it, but don't over-engineer the OSS core.
**Learn:** **Kleppmann — *Designing Data-Intensive Applications*** (the bible; read it slowly over months);
`axum` docs when the server phase lands.

## Tier 7 — Security & secrets
- [ ] Secret management: env/config, never in code/logs/fixtures/git
- [ ] TLS, verifying certs, safe HTTP; input validation on anything you parse
- [ ] Dependency auditing (`cargo-deny`, `rustsec/audit-check`) — you already wired this
- [ ] Threat-modeling the surfaces: outbound provider/LLM calls, response parsing, the future API

**Why here:** you're handling users' API keys and parsing untrusted responses — small blast radius, but
real. Get secrets hygiene right from commit #1.

## Tier 8 — Architecture & product thinking
- [ ] **Hexagonal / ports & adapters** and **Domain-Driven Design** (anti-corruption layer, bounded context)
- [ ] Headless engine + pure-view surfaces (you've done this — now know *why* it's right)
- [ ] Plugin/registry design; **open-core boundaries** (what's OSS vs commercial, and the one-way dependency)
- [ ] API design as a contract; DX/taste (the difference between a tool and a product)

**Why here:** this is the judgment that separates "can code" from "can architect a product." It's also the
Anthropic-tier signal: systems depth *plus* taste.
**Learn:** Eric Evans *DDD* (or Vaughn Vernon's *Implementing DDD*, more practical); Alistair Cockburn's
original hexagonal-architecture write-up.

---

## How this maps to the build (learn just-in-time)
| Phase | Lean on tiers |
|-------|---------------|
| Scaffold + traits + **mock** adapters (query works, no keys) | 0, 8 |
| Real Claude `Model` adapter (NL → tool-call → answer) | 4, 3, 1 |
| Real Polygon `DataProvider` + canonical schema | 2, 3 |
| Drift defense: capability descriptors + fixture/contract tests | 3, 5 |
| More adapters + `--json` wire contract | 5 |
| Hosted API + caching (open-core boundary) | 6, 7 |

## The core bookshelf
- **The Rust Book** → **Rust for Rustaceans** (Gjengset) — language mastery.
- **Zero To Production in Rust** (Palmieri) — production Rust services, testing, CI. *Start here for the how.*
- **Designing Data-Intensive Applications** (Kleppmann) — distributed systems, long game.
- **Tokio tutorial** + **Anthropic API docs** — async + LLM integration, hands-on.
- **Domain-Driven Design** (Evans/Vernon) — the schema/adapter architecture you're already building.

> Don't front-load all of this. **Tiers 0, 3, 4, 8** get you to a working, well-architected first version;
> pull in 5/6/7 as CI, scale, and the hosted layer arrive. Learn by building the mock end-to-end first.
