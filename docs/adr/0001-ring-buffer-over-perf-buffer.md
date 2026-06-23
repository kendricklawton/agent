# 0001 — Ring buffer over perf buffer

- **Status:** Accepted
- **Date:** 2026-06-23
- **Deciders:** K-Henry
- **Milestone:** M0 (used from M1)

## Context
eBPF programs must hand events to userspace. The two mechanisms are the **perf buffer**
(`BPF_MAP_TYPE_PERF_EVENT_ARRAY`) and the **ring buffer** (`BPF_MAP_TYPE_RINGBUF`):

- The perf buffer is **per-CPU**: a separate buffer per core. Under load it can **silently drop**
  events when a CPU's buffer fills, ordering across CPUs is lost, and userspace wakes per-CPU. It
  works on older kernels.
- The ring buffer (kernel ≥ 5.8) is a **single MPSC** buffer shared across CPUs, with a
  reserve/commit API, preserved ordering, adaptive wakeups (fewer syscalls), and no per-CPU
  duplication. Reserved space is either committed or discarded — no torn events.

For a security/observability agent, **silently losing events is a correctness failure**, and the
per-event work must be cheap.

## Decision
We will use the **`BPF_MAP_TYPE_RINGBUF`** ring buffer as the kernel→userspace event channel. This
sets a **minimum supported kernel of 5.8**.

## Consequences
- Lossless up to buffer capacity, correct ordering, fewer wakeups → lower overhead. Past capacity it
  still drops — but **detectably**: `bpf_ringbuf_reserve` returns NULL, so we count drops via a metric
  instead of losing events silently the way the per-CPU perf buffer does.
- Clean `reserve()` → write-in-place → `commit()` flow, which pairs with the zero-the-slot,
  padding-free event discipline ([ADR-0005](0005-event-abi-two-encodings.md)).
- **Min-kernel-5.8 floor** is now a hard platform requirement (documented in the support matrix);
  pre-5.8 kernels are unsupported.
- A future loader guard must validate the ring-buffer size is a power of two and page multiple
  (else `bpf_map_create` returns `-EINVAL`).

## Alternatives considered
- **Perf buffer** — wider kernel support, but per-CPU duplication, possible silent event loss, and
  worse wakeup behavior. Rejected: 5.8+ is broadly available on the platforms we target, and
  losslessness matters more than reaching ancient kernels.
