# Contributing to `agent`

Thanks for your interest in `agent` — the open-source eBPF node agent for Kubernetes, purpose-built
to observe and secure **GPU/AI inference** workloads. This page is the **hub**; the focused guides
under [`docs/`](./docs/) cover each concern in depth.

> Before anything else, read [**`.rules`**](./.rules) — it's the single source of truth for how this
> repo is built and what invariants must never be traded away. `CLAUDE.md`, `AGENTS.md`, and
> `GEMINI.md` all point there too, so humans and AI assistants follow the same rules.

## The guides

| Guide | Read it when |
|-------|--------------|
| [Building](./docs/contributing-building.md) | Setting up your toolchain and making your first build |
| [Testing](./docs/contributing-testing.md) | Running the test tiers and adding new tests |
| [Architecture](./docs/contributing-architecture.md) | Understanding the crates, the ABI, and the design decisions |
| [CI](./docs/contributing-ci.md) | Knowing what the pipeline gates before merge |
| [Development process](./docs/contributing-development-process.md) | Opening a change, writing an ADR, milestone flow |

Decision records live in [`docs/adr/`](./docs/adr/); the staged build plan is in
[`ROADMAP.md`](./ROADMAP.md).

## Quick start

```console
# 1. Install the toolchain + eBPF prerequisites (see docs/contributing-building.md for your distro)
rustup show                       # picks up the pinned toolchains automatically
cargo install bpf-linker          # links the eBPF object

# 2. Build (cross-compiles the eBPF object and embeds it in the agent)
cargo xtask build

# 3. Run the fast, no-privilege checks
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

Most changes only need those host-side checks — **you do not need root, a GPU, or a cluster** to fix
a parser, a rule, or a decoder. See [Testing](./docs/contributing-testing.md) for which tier your
change actually requires.

## Finding something to work on

`agent` is built in milestones (`M0`…`M7`) — see [`ROADMAP.md`](./ROADMAP.md). Each milestone opens
with an ADR and ends with a git tag, a working demo, and green CI. Good entry points:

- **Userspace, no kernel needed:** enrichment parsers, the rule engine, decoders, test fixtures.
- **eBPF:** a new probe or field capture (needs a Linux kernel + root to verify — tier 3).
- **Docs:** the support matrix, the architecture/CI guides below as they fill in.

If you're unsure whether a change fits the current milestone, open an issue first — the
[discipline test](./ROADMAP.md) is *"does this deepen kernel-level signal or k8s/GPU enrichment
without breaking the verifier, the `common` ABI, the one-way dependency, or self-hostable-zero-cloud?"*

## Commit & PR conventions

- One logical change per commit; imperative subject line ("Add the exec tracepoint", not "added").
- **Never add AI co-author or attribution trailers**, and never commit secrets or fetched/generated
  data.
- Open a milestone's design with its ADR ([`docs/adr/`](./docs/adr/)); write a new ADR when you make
  a significant, hard-to-reverse call.
- Every PR must pass the full gate — see [CI](./docs/contributing-ci.md).

## License

By contributing you agree your contributions are licensed under **Apache-2.0**, the project's license.
