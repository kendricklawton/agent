# Architecture Decision Records

Each ADR captures one significant, hard-to-reverse decision: the **context** that forced it, the
**decision**, its **consequences**, and the **alternatives** rejected. They're append-only — when a
decision changes, write a new ADR that supersedes the old one (don't edit history).

**When to write one:** at the moment a decision is made — typically at the start of a milestone's
design (the [`ROADMAP.md`](../../ROADMAP.md) milestones open with their ADRs). Copy
[`0000-template.md`](0000-template.md), take the next number, and link it from this index and from
the relevant ROADMAP item.

## Index

| ADR | Title | Status | Milestone |
|-----|-------|--------|-----------|
| [0001](0001-ring-buffer-over-perf-buffer.md) | Ring buffer over perf buffer | Accepted | M0 |
| [0002](0002-co-re-btf-over-compile-per-kernel.md) | CO-RE/BTF over compile-per-kernel | Accepted | M0 |
| [0003](0003-stable-root-nightly-ebpf-toolchain-split.md) | Stable-root / nightly-eBPF toolchain split | Accepted | M0 |
| [0004](0004-single-self-contained-binary.md) | Single self-contained binary, no sidecars | Accepted | cross-cutting |
| [0005](0005-event-abi-two-encodings.md) | The event ABI: two encodings, padding-free, in-kernel identity | Accepted | M1 |
| [0006](0006-cold-start-and-resync-contract.md) | Cold-start & re-sync: capture never gated on enrichment | Accepted | cross-cutting |
| [0007](0007-open-core-one-way-dependency.md) | Open-core: one-way dependency (cloud → OSS) | Accepted | cross-cutting |
| [0008](0008-gpu-telemetry-hybrid-collector.md) | GPU telemetry: DCGM/NVML values + ioctl attribution | Accepted | M4 |

Deferred (written when their milestone begins): the enrichment join (M2), the policy language +
stateful-evaluation model (M5), the enforcement portability ladder (M6), the exporter proto contract
(M7), the fleet control channel + signed policy-bundle distribution (M8), the performance budget /
load-shedding / supply-chain posture (M9), and the v1 ABI/proto stability guarantee + plugin interface
(M10).
