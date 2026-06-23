# Kernel & platform support matrix

What a node needs to run `agent`, and where each requirement comes from. The runtime
[boot preflight](../crates/agent/src/preflight.rs) enforces the hard rows below and warns on the
soft one; this doc is the human-readable source of truth behind it.

## Kernel version floors

Per feature — the agent's **baseline** is the ring-buffer floor; the others gate optional M6
enforcement.

| Capability | Min kernel | Milestone | Why |
|------------|-----------|-----------|-----|
| Ring buffer (`BPF_MAP_TYPE_RINGBUF`) — **baseline** | **5.8** | M0 | kernel→userspace event channel ([ADR-0001](adr/0001-ring-buffer-over-perf-buffer.md)) |
| Kill-on-match (`bpf_send_signal`) | 5.3 | M6 | broadest enforcement primitive |
| BPF-LSM deny (`BPF_PROG_TYPE_LSM`) | 5.7 | M6 | `-EPERM` from LSM hooks, where available |

The agent refuses to start below **5.8**.

## Required kernel configuration

| Requirement | Check | Hard/soft | Milestone |
|-------------|-------|-----------|-----------|
| **BTF** (`CONFIG_DEBUG_INFO_BTF=y`) | `/sys/kernel/btf/vmlinux` exists | **hard** | M0 (CO-RE — [ADR-0002](adr/0002-co-re-btf-over-compile-per-kernel.md)) |
| **cgroup v2** (unified hierarchy) | `/sys/fs/cgroup/cgroup.controllers` exists | **hard** | M0 (capture); M2 enrichment targets it |
| **`bpf` in the active LSM list** | `bpf` present in `/sys/kernel/security/lsm` | soft (warn) | M6 (BPF-LSM enforcement) |
| `CONFIG_BPF_LSM=y` + `bpf` in boot `lsm=` | as above; set via kernel `lsm=...,bpf` | soft (warn) | M6 |

"Hard" requirements abort startup with an actionable error. The `bpf`-LSM gap only downgrades M6
enforcement (to `bpf_send_signal` + `cgroup/connect`), so it's a **warning**, never a block on
capture.

## What the preflight enforces

At startup, before any eBPF is loaded, [`preflight::check()`](../crates/agent/src/preflight.rs):

- **aborts** if kernel < 5.8, BTF is absent, or cgroup v2 is not mounted — with a message naming the
  missing requirement and the fix;
- **warns** if `bpf` is not in the active LSM list (M6 enforcement will degrade);
- on success, logs the detected kernel and which capabilities are available.

This is the single runtime gate for the rows above; keep this doc and that module in lockstep.

## Distro notes

| Distro | Out-of-box status |
|--------|-------------------|
| **Arch Linux** | Ideal: bleeding-edge kernel (≫ 5.8), BTF on, cgroup v2 default, **and** `bpf` already in the default LSM stack — even M6 enforcement attaches with no boot-param change. |
| Fedora / recent Ubuntu / Debian 12+ | BTF + cgroup v2 on by default; `bpf` usually **not** in the LSM list → add `lsm=...,bpf` for M6. |
| Hardened (Flatcar / Ubuntu Pro / RHEL with SELinux/AppArmor primary) | Capture works; M6 BPF-LSM needs `bpf` added to the boot `lsm=` list (see the M6 "LSM stacking" dragon in [ROADMAP](../ROADMAP.md)). |

## Platform validation targets

Where we intend to validate, and current status. `planned` means not yet exercised — this table is
kept honest, not aspirational.

| Platform | Status |
|----------|--------|
| Bare-metal Arch (dev box) | build + preflight + unit tests pass; live eBPF load (`cargo xtask run -- --once`) pending |
| GKE | planned |
| EKS | planned |
| AKS | planned |

> Validate on GKE/EKS/AKS + at least one bare-metal kernel, and document anywhere BTF or BPF-LSM is
> absent. See the [cross-cutting standards](../ROADMAP.md) in the roadmap.
