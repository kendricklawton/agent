# CI

> **Stub** — the pipeline is scaffolded as part of M0 ([ROADMAP](../ROADMAP.md) M0). This page
> documents what each stage gates as it lands.

CI mirrors the local [testing tiers](./contributing-testing.md). Planned stages:

1. **Build** — `cargo build` (default-members) **and** the eBPF object build via `cargo xtask build`.
   (Never `cargo build --workspace` — the eBPF crate can't build for the host target.)
2. **Lint** — `cargo clippy --all-targets -- -D warnings` and `cargo fmt --all --check`.
3. **Supply chain** — `cargo deny check` (advisories, licenses, bans, sources).
4. **Unit tests** — `cargo test` (tier 1).
5. **eBPF load / verifier smoke-test** — in a `lvh`/qemu **microVM** with a pinned kernel (tier 3);
   the bare runner can't load eBPF, so this catches verifier/load regressions it otherwise couldn't.
6. **Kernel matrix** — build + load across a few kernel versions, to hold the
   [support matrix](../ROADMAP.md) honest.
7. **Open-core guard** — assert `agent` never depends on `agent-cloud`
   ([ADR-0007](./adr/0007-open-core-one-way-dependency.md)).

*TODO: link the workflow files and document required-checks/branch-protection once the pipeline is
committed.*
