# Architecture

> **Stub** — fills in as the milestones land. The authoritative design lives in the decision records
> ([`docs/adr/`](./adr/)) and the [`ROADMAP.md`](../ROADMAP.md) spine; this page will grow into the
> human-readable tour that ties them together.

## The shape

```
            kernel space                      user space (crates/agent)                  off-node
 ┌───────────────────────────┐   ringbuf   ┌───────────────────────────────┐   gRPC   ┌────────────┐
 │ eBPF programs (crates/ebpf)│ ─────────▶ │ loader → decode → enrich →    │ ───────▶ │ agent-cloud│
 │  exec / fork / connect /   │  (events)  │   rules → enforce → sinks      │ events   │ (private   │
 │  open / creds / LSM hooks  │ ◀───────── │      ▲           │        ▲     │ ◀─────── │  fleet     │
 │  policy maps               │   policy   │  crates/enrich  rules   fleet   │ policy   │  plane)    │
 └───────────────────────────┘            │  cgroup→pod    (M5/6)  (M8)      │ bundles  └────────────┘
        ▲ CO-RE / BTF                      └─────────────────────────────────┘
        │
   bpf_get_current_cgroup_id
```

## The crates

| Crate | Role |
|-------|------|
| `crates/common` | `#[repr(C)]` event types — the kernel ⇄ userspace ⇄ cloud contract (`no_std`). The spine. |
| `crates/ebpf` | eBPF programs (`no_std`, BPF target): tracepoints / kprobes / LSM |
| `crates/agent` | userspace binary: boot preflight, load eBPF, drain the ring buffer, enrich, alert, export |
| `crates/enrich` | k8s enrichment via `kube-rs` (cgroup → container → pod) — *M2, not yet* |
| `crates/rules` | detection engine + policy language (compile rules → eval) — *M5, not yet* |
| `crates/exporter` | gRPC/proto export to `agent-cloud` — *M7, not yet* |
| `crates/fleet` | fleet control: signed policy bundles, node identity, multi-tenancy — *M8, not yet* |
| `xtask` | build orchestration (dev-only, never shipped) |

## Where the design is recorded

Every load-bearing decision is an ADR — start with the [index](./adr/). The invariants that must never
be traded away are summarized in [`.rules`](../.rules) and the
[Architectural invariants](../ROADMAP.md) section of the roadmap.

*TODO: expand into a narrated walkthrough — the event lifecycle, the enrichment join, the cold-start
clocks, and the GPU collector — as M1–M4 are built.*
