# 0005 — The event ABI: two encodings, padding-free, in-kernel identity

- **Status:** Accepted
- **Date:** 2026-06-23
- **Deciders:** K-Henry
- **Milestone:** M1 (the spine; evolves additively through M7, frozen as **v1** at M10)

## Context
`crates/common` is the contract spanning **kernel ⇄ userspace ⇄ cloud**. The three hops have
conflicting needs:
- **Kernel → userspace** is `no_std`, on the hot path, and passes through the BPF verifier — which
  **rejects uninitialized bytes** (incl. struct padding) reaching `bpf_ringbuf_submit`. It must be
  zero-copy and tiny.
- **Userspace → cloud** crosses a network and must **evolve** (new fields, new event kinds) without
  breaking older peers.

A single encoding can't serve both (protobuf is wrong in the kernel; raw C structs are wrong on the
wire long-term).

## Decision
We will use **two encodings behind one logical contract**:
- **Kernel ⇄ userspace:** `#[repr(C)]` POD structs read zero-copy off the ring buffer. They are
  **padding-free by construction** (order `u64`s first) and guarded by `const _: () =
  assert!(size_of::<T>() == …)`. An **`EventHeader`** prefixes every event with an in-kernel
  `ktime_ns` (`bpf_ktime_get_ns`), a `kind` discriminant, and a `version`. Identity — `cgroup_id`
  **and** `mnt_ns_inum` — is captured **in-kernel, at event time** (the only moment it's guaranteed
  to exist). The reserved ring-buffer slot is zeroed before fields are written.
- **Userspace ⇄ cloud:** **protobuf/gRPC** (M7), schema in `crates/common`/proto, evolved
  additively.
- **Enrichment** (`PodMeta`, the `synced` flag) is a **userspace-only annotation** layered onto the
  exported event — *not* part of the kernel `#[repr(C)]` ABI.

ABI evolution rule: **additive only** — never reorder/resize existing fields; bump
`EventHeader.version` / add new proto field numbers and event kinds.

## Consequences
- Zero-cost, verifier-safe kernel path; an evolvable, cross-language cloud path.
- Two representations to keep in sync — the exporter maps `#[repr(C)]` → proto (a deliberate, small
  boundary).
- The `const`-asserts catch any **size or hidden-padding** change at build time — adding/removing a
  field, changing a field's width, or the compiler inserting padding all break the build instead of
  corrupting production. (A same-width field *swap* keeps the total size identical, so the assert does
  **not** catch it — the additive-only rule plus review is what guards layout there.)
- `mnt_ns_inum` captured in M1 (before it's "needed") is the slower-recycling secondary key that lets
  M2 reconcile short-lived pods ([ADR-0006](0006-cold-start-and-resync-contract.md)).
- The contract grows additively through the milestones and is **frozen as `v1` at M10** (platform GA),
  where a conformance suite turns the additive-only rule into an enforced stability guarantee. The two
  encodings stay in lockstep up to that point.

## Alternatives considered
- **Protobuf everywhere (incl. kernel)** — rejected: not `no_std`/verifier-friendly, and adds hot-path
  cost in the kernel.
- **Raw C structs on the wire too** — rejected: no schema evolution; every change would break the
  cloud contract.
- **Generate the kernel types from the proto** — rejected: proto-generated types aren't the
  fixed, padding-controlled `#[repr(C)]` the verifier and ring buffer need.
