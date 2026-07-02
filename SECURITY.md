# Security Policy

This is a modular **natural-language data query engine** — an **unprivileged** CLI tool. It makes
**outbound HTTP** calls to the LLM and data providers *you* configure (using **your API keys, from the
environment**), and **parses their responses** into a canonical schema. It needs no root and stores no
long-lived service. Its blast radius is small, but the **secret-handling**, **outbound-call**, and
**response-parsing** surfaces are real. Please report vulnerabilities **privately** so we can fix them
before they're public.

## Supported versions

The project is pre-release and pre-`1.0`; the surfaces are still moving (see [`ROADMAP.md`](./ROADMAP.md)).
During this phase, **only the latest tagged release and the `main` branch receive security fixes** — there
are no backported patch streams yet. A formal support window lands with `v1.0.0`.

| Version | Supported |
|---------|-----------|
| `main` + latest tag | ✅ |
| older tags | ❌ — upgrade to the latest |

## Reporting a vulnerability

**Please do not open a public issue, pull request, or discussion for a suspected vulnerability.**

Report it privately via **GitHub's private vulnerability reporting** — the **"Report a vulnerability"**
button under the repository's **Security** tab (GitHub Security Advisories). That opens a private channel
with the maintainers.

Please include, as far as you can determine:

- the **component** (core / a `Model` adapter / a `DataProvider` adapter / CLI) and the version / commit;
- the **impact** and a **reproduction** (a PoC, steps, or a failing test);
- whether it requires a **specific provider/LLM** or a particular (recorded or live) response.

**What to expect.** This is an early, small project — we aim to **acknowledge within a few days** and keep
you updated as we triage and fix. We practice **coordinated disclosure**: agree an embargo, fix and release,
then publish an advisory **crediting you** (unless you prefer to stay anonymous). Good-faith research under
this policy is welcome and we will not pursue it.

## Scope — what is and isn't a vulnerability

**In scope** (please report):

- **Credential/secret leakage:** an API key appearing in logs, error messages, a recorded fixture, a
  world-readable config, or anywhere it's persisted.
- **Parsing untrusted responses:** malformed data from an LLM or data provider causing a crash, panic,
  unbounded memory/CPU use, or worse in an adapter's raw→canonical mapping.
- **Outbound-request abuse:** a configured endpoint/URL being coerced into reaching an unintended host
  (SSRF-style), or a provider response steering the engine to fetch somewhere it shouldn't.
- **Prompt-injection with real consequences:** content from a data source manipulating the model into
  leaking a secret or exceeding the engine's read-only, answer-only behavior. (A merely *wrong answer* is
  out of scope — see below.)
- **Supply-chain** issues in our build, release artifacts, signing, or SBOM.
- The app **requesting or requiring elevated privileges** it shouldn't need.

**Out of scope** (not vulnerabilities):

- The app **making outbound calls to the LLM/data endpoints you configured** — that's its normal behavior.
- A **wrong or imprecise answer**: this is a research/analysis tool, not financial advice; incorrect
  analysis is a bug to file, not a security issue (unless it stems from one of the in-scope items).
- Attacks that **already require local access** to your machine (the attacker has already won).
- Data from a **source you explicitly configured and trust**.

## Our security posture

Several of this project's invariants *are* security controls — see the **Architectural invariants** in
[`ROADMAP.md`](./ROADMAP.md) and the design notes in [`ARCHITECTURE.md`](./ARCHITECTURE.md):

- **Unprivileged by design** — no root, no special capabilities. If the app ever appears to need elevation,
  that's a bug to report.
- **Secrets stay out of the repo** — API keys come from env/config only; never committed, logged, or placed
  in fixtures, and redacted from error output.
- **Bounded, panic-free parsing** — `unwrap`/`expect`/`panic!` are denied outside tests; a malformed LLM or
  provider response degrades to a clear error, it never crashes the engine.
- **Grounded, read-only, answer-only** — the engine fetches data and answers from it; it takes no actions,
  which bounds what a prompt-injection can achieve.
- **Anti-corruption boundary** — providers' raw responses are contained in one adapter and mapped to the
  canonical schema, keeping the parsing/trust surface small and in one place.
- **Supply chain** — reproducible, keyless-signed releases + SBOM, pinned dependencies, `cargo deny` in CI,
  and a scheduled advisory audit.
