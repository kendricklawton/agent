# 0004 — Single self-contained binary, no sidecars

- **Status:** Accepted
- **Date:** 2026-06-23
- **Deciders:** K-Henry
- **Milestone:** cross-cutting

## Context
The agent runs on every node as a privileged workload. How it's packaged drives its blast radius,
attack surface, and operational simplicity. aya (pure Rust, no libbpf) **embeds the compiled BPF
object into the userspace binary at build time** (`include_bytes_aligned!`) — no separate `.o`, no
runtime clang/BCC, no libbpf C dependency — and can link statically against musl.

The workspace has many crates (`common`, `ebpf`, `agent`, plus future
`enrich`/`rules`/`exporter`/`fleet`), which could be mistaken for a multi-process design.

## Decision
We will ship the OSS agent as **one self-contained binary** with the eBPF object embedded, deployed
as a single **DaemonSet** (one pod per node), **with no sidecars**. The workspace crates are
**library boundaries, not processes** — they link into the one `agent` binary. `xtask` is a dev-only
build tool, never shipped. GPU collection, the rules engine, optional enforcement, the cloud exporter,
and the fleet-control client (M8) are all **in-process modules**, not separate containers. The plugin
/ extension SDK (M10) extends the binary in-process (trait-based and/or WASM) — it adds extensibility
without adding a sidecar.

## Consequences
- Minimal blast radius and attack surface; one artifact to build, sign, deploy, and reason about;
  optional musl-static for a tiny image.
- This is the deliberate **inverse of `agent-cloud`**, which *is* multi-service (ingest/store/
  analytics/api) — node agent stays lean; the service split lives in the cloud.
- One caveat: if **DCGM** is used for GPU metrics ([ADR-0008](0008-gpu-telemetry-hybrid-collector.md)),
  it implies an out-of-process NVIDIA daemon on GPU nodes — an external dependency the agent scrapes,
  not code inside the binary. NVML keeps the collector in-process.
- **Static-musl caveat on GPU nodes:** the tiny musl-static image holds for the base agent, but the
  GPU collector loads the NVIDIA driver library at runtime (`nvml-wrapper` `dlopen`s
  `libnvidia-ml.so.1`; the ioctl path needs the driver too). A *fully* static binary can't `dlopen`,
  so GPU builds link dynamically (glibc, or a dlopen-capable musl) against the node's driver —
  static-musl stays the default only for non-GPU nodes ([ADR-0008](0008-gpu-telemetry-hybrid-collector.md)).

## Alternatives considered
- **Multi-process / sidecars** (separate collector, exporter, etc.) — rejected: more surface, more to
  deploy, no benefit at node scale.
- **libbpf/C or BCC** (separate BPF object / runtime toolchain) — rejected: not pure-Rust, adds
  runtime deps, and forfeits the single-static-binary property aya gives us.
