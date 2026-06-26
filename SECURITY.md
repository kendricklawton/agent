# Security Policy

This is a native **GPU & inference monitor** — an **unprivileged, read-mostly** desktop/CLI tool. It
reads local GPU metrics via **NVML** (no root, no special capabilities), reads `/proc` for process
names, optionally scrapes **local inference endpoints** (Ollama/vLLM), and — only if you enable it —
talks to a **remote collector** over the network. Its blast radius is small, but the network and
parsing surfaces are real. We ask that you report vulnerabilities **privately** so we can fix them
before they're public.

## Supported versions

The project is pre-release and pre-`1.0`; the surfaces are still moving (see [`ROADMAP.md`](./ROADMAP.md)).
During this phase, **only the latest tagged release and the `main` branch receive security fixes** —
there are no backported patch streams yet. A formal support window (supported minor versions +
backport policy) lands with `v1.0.0`.

| Version | Supported |
|---------|-----------|
| `main` + latest tag | ✅ |
| older tags | ❌ — upgrade to the latest |

## Reporting a vulnerability

**Please do not open a public issue, pull request, or discussion for a suspected vulnerability.**

Report it privately via **GitHub's private vulnerability reporting** — the **"Report a
vulnerability"** button under the repository's **Security** tab (GitHub Security Advisories). That
opens a private channel with the maintainers.

Please include, as far as you can determine:

- the **component** (collector / core / GUI / TUI / CLI / exporters / remote `serve`) and the version / commit;
- your **OS, GPU, and driver/NVML version** — GPU-source bugs are often driver-specific;
- the **impact** and a **reproduction** (a PoC, steps, or a failing test);
- whether it requires **remote mode** (`serve`/multi-host) or a specific **inference endpoint**.

**What to expect.** This is an early, small project — we aim to **acknowledge a report within a few
days** and to keep you updated as we triage and fix. We practice **coordinated disclosure**: we agree
an embargo window with you, fix and release, then publish an advisory **crediting you** (unless you
prefer to stay anonymous). Good-faith research conducted under this policy is welcome and we will not
pursue it.

## Scope — what is and isn't a vulnerability

**In scope** (please report):

- **The network surfaces** (`serve` remote + the exporter (Phase 9) `/metrics` / OTLP endpoints): a listener
  exposing metrics without auth or binding more broadly than intended, or a malicious remote
  host/collector being able to crash or exploit the desktop client.
- **Parsing untrusted input:** malformed data from an inference endpoint (Ollama `/api/ps`, Prometheus
  text) or a remote snapshot causing a crash, panic, unbounded memory use, or worse in the parser.
- **Credential/secret leakage:** if the app stores remote-host addresses, tokens, or endpoint
  credentials, anything that leaks them (world-readable config, logs, etc.).
- **Supply-chain** issues in our build, release artifacts, or signing.
- The app **requesting or requiring elevated privileges** it shouldn't need.

**Out of scope** (not vulnerabilities):

- The app **reading local GPU metrics (NVML) and `/proc`** — that's its normal, unprivileged behavior.
- Attacks that **already require local access** to the user's machine (the attacker has already won).
- Data shown from a **source the user explicitly pointed it at** (you chose to trust that host/endpoint).

## Our security posture

Several of this project's invariants *are* security controls — see the **Architectural invariants** in
[`ROADMAP.md`](./ROADMAP.md) and the design notes in [`ARCHITECTURE.md`](./ARCHITECTURE.md):

- **Unprivileged by design** — no root, no special capabilities; NVML/DCGM are read-only. If the app
  ever appears to need elevation, that's a bug to report.
- **Headless-engine boundary** — frontends only render `core`; data sources are isolated in
  `collector`, which keeps the parsing/trust surface small and in one place.
- **Bounded + panic-free parsing** — `unwrap`/`expect`/`panic!` are denied outside tests; malformed
  input from an endpoint or remote host degrades gracefully, it never crashes the app.
- **Local-first, remote opt-in** — nothing leaves your machine unless you enable remote monitoring;
  `serve` and multi-host are authenticated and off by default.
- **Supply chain** — reproducible, signed releases + SBOM, pinned dependencies, `cargo deny` in CI.
