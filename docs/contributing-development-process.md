# Development process

> **Stub** — captures the working rhythm of the repo; expands as the project grows.

## Milestone flow

Work is organized into milestones `M0`…`M10`, laddering to the `v0.10.0` platform release
([`ROADMAP.md`](../ROADMAP.md)). Each milestone:

- **opens with an ADR** that records the load-bearing decision before the code,
- ends with a **git tag + a working demo + green CI**, and
- is not started until the prior milestone is green.

The discipline test for any change: *does this deepen kernel-level signal or k8s/GPU enrichment
without breaking the verifier, the `common` ABI, the one-way dependency, or self-hostable-zero-cloud?*
If not, it sinks to a later milestone.

## Decision records

Write an ADR ([`docs/adr/`](./adr/)) when you make a significant, hard-to-reverse call. Copy
[`0000-template.md`](./adr/0000-template.md), take the next number, and link it from the
[index](./adr/README.md) and the relevant ROADMAP item. ADRs are **append-only** — supersede with a
new one rather than rewriting history.

## Commits & PRs

- One logical change per commit; imperative subject.
- **Never add AI co-author or attribution trailers**; never commit secrets or fetched/generated data.
- Every PR passes the full gate — see [CI](./contributing-ci.md).

## The source of truth

[`.rules`](../.rules) is authoritative for build commands, invariants, and code conventions; keep it
**concise and current**, and put depth in the ADRs and these guides rather than there.

*TODO: add issue/PR templates and a release/tagging checklist as they're introduced.*
