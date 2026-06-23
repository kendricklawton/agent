# agent

Open-source **eBPF node agent** for Kubernetes — observes and secures workloads, purpose-built to
understand **GPU/AI inference** workloads.

> **Status:** early scaffold (M0). The `common` event contract exists; probes and enrichment are
> in progress. See [`ROADMAP.md`](./ROADMAP.md) for the staged build plan.

## Why
Generic runtime-security/observability tools (Falco, Tetragon, Pixie) don't understand GPU or
per-model workloads. `agent` ties kernel-level events to the **pod, namespace, and model** behind
them — so you can answer questions like *"what just spawned a shell inside an inference pod?"* or
*"which model is driving egress from this GPU node?"*

**Signature demo (the target):** drop a shell in a GPU inference pod → the agent catches it and
names the pod, in real time.

## What it does
- **eBPF probes** (via `aya`, pure Rust): process `exec`, outbound `connect`, file opens.
- **k8s enrichment:** map every event to its pod / namespace / workload (cgroup → container → pod).
- **GPU/AI awareness:** surface GPU utilization and inference signals per pod — the part generic
  tools miss.
- **Local rules + alerts:** runs fully self-hosted, **zero cloud required**.
- **Optional enforcement:** block/kill via LSM-BPF.

## Stack
Rust · [`aya`](https://github.com/aya-rs/aya) (eBPF, pure Rust) · `kube-rs` · Cargo workspace.
Shipped as a Kubernetes **DaemonSet**.

## Layout
```
crates/common    shared #[repr(C)] event types — the kernel ⇄ userspace ⇄ cloud contract (no_std)
crates/ebpf      eBPF programs (no_std, BPF target): kprobes / tracepoints / LSM
crates/agent     userspace binary: loads eBPF, reads the ring buffer, enriches, alerts
crates/enrich    k8s metadata enrichment via kube-rs
crates/rules     detection engine
crates/exporter  ship events to the control plane (gRPC/proto)
xtask            build orchestration (compile eBPF + run)
```
(Only `crates/common` exists today; the rest land per the roadmap.)

## Open-core
`agent` is fully usable self-hosted with no cloud. The optional **`agent-cloud`** (private) is a
fleet control plane — multi-cluster analytics, storage, alerting. Dependencies point **one way**:
`agent-cloud` → `agent`. This repo never imports the cloud.

## License
Apache-2.0.

---
See [`.rules`](./.rules) for contributor/agent guidance and [`ROADMAP.md`](./ROADMAP.md) for the
build plan.
