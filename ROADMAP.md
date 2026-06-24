# Roadmap — `agent` (OSS, Rust + eBPF/aya)

## §0 The spine

**`agent` is the open-source eBPF node agent you run in your own cluster; `agent-cloud` is the
hosted fleet control plane — the *same* event contract, aggregated across clusters, run for teams
who don't want to operate that control plane themselves.** The model is **open-core** (Falco →
Sysdig, Tetragon → Isovalent, Parca → Polar Signals): you run the agent free and self-hosted
forever; when you need fleet-wide aggregation, storage, analytics, and compliance without standing
up that infra, you point your agents at the cloud. The moat is **GPU/AI-workload awareness** — the
un-crowded slice generic tools miss — not generic k8s observability.

This is a **platform**, not a single-purpose collector: the end state is a fleet-manageable runtime
security and observability system for AI/GPU infrastructure — full kernel signal, k8s + GPU
enrichment, a real policy language, optional enforcement, and a control channel — that stays a
single self-contained binary and is fully usable with **zero cloud**.

The shape it builds toward:

```
            kernel space                      user space (crates/agent)                  off-node
 ┌───────────────────────────┐   ringbuf   ┌───────────────────────────────┐   gRPC   ┌────────────┐
 │ eBPF programs (crates/ebpf)│ ─────────▶ │ loader → decode → enrich →    │ ───────▶ │ agent-cloud│
 │  exec / fork / connect /   │  (events)  │   rules → enforce → sinks      │ events   │ (private   │
 │  open / creds / LSM hooks  │ ◀───────── │      ▲           │        ▲     │ ◀─────── │  fleet     │
 │  policy maps               │   policy   │  crates/enrich  rules   fleet   │ policy   │  plane)    │
 └───────────────────────────┘            │  cgroup→pod +  (M5/6)  (M8)      │ bundles  └────────────┘
        ▲ CO-RE / BTF                      │  process tree    │              │
        │                                  └──────────────────┼──────────────┘
   bpf_get_current_cgroup_id                   NVML/DCGM + ioctl + /metrics (M4)
```

Five keystones hold it up:

1. **The kernel is the source of truth; `crates/common` is the contract.** eBPF programs emit
   `#[repr(C)]` events through one ring buffer; that crate is the ABI between kernel ⇄ userspace ⇄
   cloud. It stays `no_std`, dependency-light, and **padding-free** (the verifier rejects
   uninitialized bytes). Identity (`cgroup_id`, `mnt_ns_inum`, `ktime_ns`) is captured **in-kernel,
   at event time** — the only moment it's guaranteed to exist. If this drifts, everything downstream
   breaks.
2. **One event contract, consumed two ways.** Local sinks (logs / `/metrics` / alerts) and the cloud
   exporter consume the *same* events. Self-hosted and cloud-connected give the same signal; the
   cloud only aggregates. The contract never forks.
3. **Dependencies point one way: cloud → OSS, never back.** The agent is **fully usable self-hosted
   with zero cloud**. It can be *fleet-managed* (M8) — pull signed policy, report health, take a
   fixed verb set — but it is **offline-first**: a stale last-known-good policy keeps it fully
   operational when the plane is unreachable. This repo never imports `agent-cloud`; CI enforces it.
4. **The wedge is GPU/AI-workload awareness.** Every feature points at "for AI/GPU workloads" — GPU
   utilization, per-model latency, KV-cache, attribution to the pod/model. That's the differentiation.
5. **One self-contained binary.** aya embeds the eBPF object into the userspace binary at build time
   (no libbpf, no runtime toolchain); the workspace's many crates are library boundaries, not
   processes (`xtask` is dev-only). Shipped as a single DaemonSet binary — **no sidecars** — for
   minimal blast radius and attack surface. (The inverse of `agent-cloud`, which *is* multi-service.)

**The discipline test for every step:** *does this deepen kernel-level signal or k8s/GPU enrichment
without breaking the verifier, the `common` ABI, the one-way dependency, or self-hostable-zero-cloud?*
If no, it sinks to a later milestone. **Correctness before performance — the verifier is the gate.
Capture is never gated on enrichment.**

This roadmap ladders to **`v0.10.0` — the platform release**, budgeted at roughly **100k lines of
authored Rust** (see [*Scale & LOC budget*](#scale--loc-budget-the-0100-target)). Each milestone
`M0…M10` maps to a minor tag `v0.0.0…v0.10.0` and is a **git tag + a working demo + green CI**; we
don't start one until the prior is green. `0.10.0` is the goal line, **not `1.0`** — it marks the
platform feature-complete and contract-stable; `1.0` is the later post-GA SemVer-commitment phase.
Numbering is shared with [`LEARN.md`](./LEARN.md) and [`.rules`](./.rules).

---

## Milestone index

| M | Milestone | Tag | LOC\* | Demo (observable outcome) |
|---|-----------|-----|-------|---------------------------|
| 0 | **Scaffold & CI** | `v0.0.0` | ~1k | `cargo xtask run` loads a no-op program on a real kernel; CI green for BPF + userspace |
| 1 | **First probe (exec)** ⭐ | `v0.1.0` | ~2k | run `/bin/ls` → typed `ExecEvent` (pid/ppid/uid/comm/cgroup) in userspace via ring buffer |
| 2 | **k8s enrichment** ⭐ | `v0.2.0` | ~12k | `kubectl exec` into a pod → event names the **pod/namespace/container/workload** |
| 3 | **Full syscall surface** (net + file + identity) | `v0.3.0` | ~22k | `curl` → `ConnectEvent`; read a secret → `FileEvent`; DNS, `setuid`, fork/exit — all enriched |
| 4 | **GPU/AI telemetry** ⭐ | `v0.4.0` | ~32k | per-pod GPU util/mem + inference KPIs joined to k8s identity (the differentiator) |
| 5 | **Detection engine + policy language** | `v0.5.0` | ~44k | compile a rule → alert: "shell in an inference pod", "unexpected egress from a GPU node" |
| 6 | **Enforcement** (optional) | `v0.6.0` | ~53k | kill-on-match (`bpf_send_signal`) + egress deny; BPF-LSM deny where supported; audit-first |
| 7 | **Exporter + packaging** | `v0.7.0` | ~61k | gRPC/OTLP stream; DaemonSet + Helm + RBAC; Prometheus `/metrics`; offline-first |
| 8 | **Fleet control & multi-tenancy** | `v0.8.0` | ~73k | signed OCI policy bundles pushed fleet-wide; per-node identity; tenant isolation; staged rollout |
| 9 | **Hardening, scale & performance** | `v0.9.0` | ~85k | kernel + arch (x86_64/arm64) matrix green; perf budget held at 250-pod scale; fuzz + soak + chaos |
| 10 | **Platform GA** ⭐ | `v0.10.0` | ~100k | stable ABI/proto v1, plugin SDK, conformance suite, full docs — the platform, end to end |

> \* **LOC** = *cumulative* authored Rust toward the `v0.10.0` target; excludes generated `vmlinux`
> and vendored protos; ~30% is tests. It's the **shape** of the build, not a quota — we never pad to
> hit it (see [*Scale & LOC budget*](#scale--loc-budget-the-0100-target)). The earlier "Cert"
> mapping (CKS/CKA/RBAC as a learning byproduct) now lives in [`LEARN.md`](./LEARN.md).

> **Signature demo (from M2 on):** *drop a shell in a GPU inference pod → the agent catches it and
> names the pod, in real time.* M4 (GPU) is the wedge. The platform value compounds: M2 names it, M4
> attributes it to a model, M5 alerts on it, M6 stops it, M8 pushes that policy across the fleet.
> M0 → M2 → M4 → M5 is the earliest end-to-end story; M6–M10 turn it into a product.

---

## M0 — Scaffold & CI
A reproducible build of both a BPF object and the userspace binary, loadable on a real kernel, gated
by CI. No probes yet — just the skeleton everything hangs on.

- [x] Cargo workspace + the `common` crate (the ABI spine; `Cargo.toml` with `members = ["crates/*"]`).
- [x] Scaffold the remaining crates: `ebpf` (`no_std`, BPF target), `agent` (userspace, tokio),
  `xtask` (build orchestration). `enrich`/`rules`/`exporter`/`fleet` deferred to their milestone.
- [x] `rust-toolchain.toml` split: root pins **stable**; `crates/ebpf/rust-toolchain.toml` pins
  **nightly** (dir-scoped) for the BPF target (`bpfel-unknown-none`, `-Z build-std=core`, `bpf-linker`).
  → [ADR-0003](docs/adr/0003-stable-root-nightly-ebpf-toolchain-split.md)
- [x] **`xtask` + build wiring**: `crates/agent/build.rs` cross-compiles the eBPF crate under nightly
  (clearing the inherited `RUSTUP_TOOLCHAIN` so the dir-scoped pin wins) and embeds it via
  `include_bytes_aligned!`. `cargo xtask build` / `cargo xtask run` are the canonical entrypoints.
- [~] **CO-RE/BTF:** `cargo xtask codegen` mechanism + `mod vmlinux;` wiring in place (relies on
  `/sys/kernel/btf/vmlinux`; one portable object via load-time relocation; never hand-roll structs).
  Run `cargo xtask codegen` (needs `bpftool` + `aya-tool`) to populate `crates/ebpf/src/vmlinux.rs`
  with `task_struct`; M1 is the first consumer. → [ADR-0002](docs/adr/0002-co-re-btf-over-compile-per-kernel.md)
- [x] **Decision: ring buffer over perf buffer** (`BPF_MAP_TYPE_RINGBUF` — MPSC, lossless, clean
  reserve/commit). Sets the **min-kernel-5.8** floor. → [ADR-0001](docs/adr/0001-ring-buffer-over-perf-buffer.md)
- [x] **Boot preflight** (shipped now, relied on everywhere after): probe BTF present, cgroup v2,
  kernel ≥ 5.8, and read `/sys/kernel/security/lsm`; if `bpf` is absent, `WARN` that LSM enforcement
  (M6) can't attach and will degrade. → `crates/agent/src/preflight.rs`
- [x] CI: `cargo build` (default-members) + the eBPF object build, `cargo clippy -- -D warnings`,
  `cargo fmt --check`, `cargo deny check`. → `.github/workflows/ci.yml` (plus an on-runner load smoke-test).
- [~] CI: **eBPF load/verifier smoke-test in a microVM** (`lvh`/qemu, known kernel) — catches
  verifier/load regressions the bare GitHub runner can't. → `.github/workflows/ebpf-smoke.yml`
  (scaffolded over pinned kernels; needs CI iteration to go green).
- [x] ADR template + [`docs/adr/`](docs/adr/) log; foundational decisions recorded as ADR-0001…0008.
- [x] Seed the **kernel/platform support matrix** doc. → [`docs/support-matrix.md`](docs/support-matrix.md)
- [ ] Tag `v0.0.0` (loads + attaches a trivial no-op program, detaches cleanly on `Drop`).

## M1 — First probe: process execution ⭐
The end-to-end pipeline for one event type — kernel hook → ring buffer → typed struct in userspace.
**Demo:** start the agent, run `/bin/ls`, see a decoded `ExecEvent` with the right pid/ppid/uid/comm.

- [x] **Define the event ABI in `crates/common`** — `EventHeader` (with in-kernel `ktime_ns`),
  `EventKind`, and `ExecEvent` (incl. `cgroup_id` **and** `mnt_ns_inum`). Padding-free by
  construction (u64s first); `const _: () = assert!(size_of::<T>() == …)` guards the layout.
- [x] **Hook:** **regular** tracepoint `sched/sched_process_exec` — canonical "a process started"
  signal, post-exec so `comm`/exe are stable. (Chose `#[tracepoint]` over a raw tracepoint:
  `EbpfContext` gives pid/uid/comm for free and the args are less CO-RE-coupled for no gain on
  low-frequency exec.)
- [~] **In-kernel capture:** done — `pid` (tgid), `uid/gid`, `comm`, `cgroup_id`
  (`bpf_get_current_cgroup_id`), `ktime_ns` (`bpf_ktime_get_ns`), and the exec `filename` (tracepoint
  `__data_loc`). **Pending Part B (CO-RE):** `ppid` (`task → real_parent → tgid`) and the **mount-ns
  inode** (`task → nsproxy → mnt_ns → ns.inum`) — needs `cargo xtask codegen` (`bpftool`+`aya-tool`).
- [x] **Write discipline:** reserve a ring-buffer slot, **zero the whole slot, then write fields in
  place** — never build on the 512-byte stack and copy. (The verifier rejects any uninitialized,
  incl. padding, byte reaching `bpf_ringbuf_submit` as *invalid indirect read from stack*.)
- [x] **Userspace:** consume `aya::maps::RingBuf` async (tokio `AsyncFd`); cast bytes → struct
  (`bytemuck`); dispatch on `hdr.kind`; handle partial reads + shutdown (detach on `Drop`).
- [x] **Loader guard:** ring-buffer size is a **power of two and a page multiple** — enforced as a
  compile-time `const`-assert in `agent_common` (the size is baked into the object, so it's a build-time
  guard, not a runtime one). *(Moved from M0 — the ring buffer first exists here.)*
- [ ] Bounded `argv` capture (a few args, truncated) via `bpf_probe_read_user` under `#[unroll]` —
  every read guarded for the verifier.
- [~] **Tests:** unit done — the ABI byte round-trip + `EventKind` + preflight parsing. **Pending:**
  deterministic VM integration test (spawn a known binary → assert one `ExecEvent`) + zero-loss under a
  tight exec loop (needs root / the `ebpf-smoke.yml` microVM).

> **Why `mnt_ns_inum` + `ktime` from M1, not later:** the mount-ns inode is a slower-recycling
> *secondary* identity key that lets M2 reconcile short-lived pods whose cgroup dir is unlinked
> before userspace resolves it; the in-kernel timestamp stops late enrichment from distorting event
> time. `synced`/`PodMeta` are **userspace annotations, not part of this kernel ABI** (see M2).

## M2 — Kubernetes enrichment ⭐ (the keystone)
Turn a kernel `cgroup_id` into **pod / namespace / container / workload** — the join that makes
everything else valuable, and the genuinely hard userspace engineering. **Demo:**
`kubectl exec -it <pod> -- sh` → the exec event is labeled with its pod, namespace, container, and
owning workload.

- [ ] ADR: the enrichment join (cgroupfs-path primary, CRI socket fallback) and the cache model.
- [ ] **`cgroup_id → containerID`:** walk **only `kubepods.slice`** and `statx` **directories only**
  (the `cgroup_id` *is* the dir inode — never stat leaf files like `cpu.pressure`/`memory.stat`, or a
  250-pod node spikes the agent's own CPU). Parse the container id from the leaf path
  (`.../cri-containerd-<id>.scope`, `...docker-<id>.scope`, CRI-O variants).
- [ ] One-time backfill + `inotify` deltas on a **low-priority background task** that yields between
  descents. Cache `cgroup_id → containerID`.
- [ ] **`containerID → PodMeta`:** a **node-scoped** kube-rs reflector/watch
  (`fieldSelector=spec.nodeName=$NODE_NAME`, from downward-API `NODE_NAME`), indexed by
  `status.containerStatuses[].containerID`; evict on pod delete.
- [ ] **Process-tree reconstruction:** stitch exec/fork/exit (the M3 lifecycle events) into per-pod
  process ancestry so an event carries its parent chain, not just its own pid — the backbone every
  detection rule and forensic trace leans on. Bounded, lifecycle-evicted.
- [ ] **RBAC (the CKS/RBAC exercise):** a ServiceAccount with `get/list/watch` on `pods` (and
  `nodes`) only — no cluster-wide write.
- [ ] **Cold-start & re-sync** (the architectural invariant — see [`.rules`](./.rules)): attach probes
  first, seed the baseline from `/proc`, node-scope the List for a sub-second sync, and park
  cache-miss events in a bounded short-deadline resync queue keyed on `(cgroup_id, mnt_ns_inum)`;
  emit enriched or explicit `Unknown`, **never drop, never block the ring-buffer drain**.
- [ ] Mark every event with a `synced` bit (`cold_start` vs `steady`); readiness probe stays
  `NotReady` until the `/proc` backfill + initial List complete.
- [ ] **Tests:** `kind` cluster, deploy a pod, `kubectl exec`, assert correct pod/ns/container.
  Cover **both containerd and CRI-O** cgroup formats and **systemd vs cgroupfs** drivers. Cache
  correct across pod churn; bounded growth (LRU + lifecycle eviction).

> **Dragon — short-lived-pod ghosting:** a 45 ms `cronjob` execs and exits; its cgroup dir is
> unlinked before the `cgroup_id → path` lookup runs, resolving to nothing. The slower-recycling
> `mnt_ns_inum` (captured in M1) is the secondary key that reconciles the dead workload.

## M3 — Full syscall surface: network, file, identity
Broaden signal from one event type to the surface a security platform needs — egress, file access,
credential changes, and process lifecycle — reusing the M1 envelope and M2 enrichment. **Demo:**
`curl` from a pod → enriched `ConnectEvent`; `cat /etc/shadow` → enriched `FileEvent`; `sudo` →
`CredEvent`; a forking workload → a correct process tree.

- [ ] **Network egress:** `kprobe`/`fexit` on `tcp_v4_connect`/`tcp_v6_connect`, reading
  `saddr/daddr/sport/dport/family` from `struct sock` via CO-RE. (Choose `cgroup/connect4|6`
  `BPF_PROG_TYPE_CGROUP_SOCK_ADDR` where it fits — **reused for enforcement in M6**, since it denies.)
- [ ] **DNS visibility:** parse DNS query/response (tracepoint or socket filter) so egress reads as
  names, not just IPs — the join most "unexpected egress" rules actually need.
- [ ] **File access:** `fentry`/tracepoint on `do_sys_openat2`/`sys_enter_openat`; bounded path, flags,
  result. **Filter in-kernel** — opens are high-volume — via an `LPM_TRIE`/`HASH` of sensitive path
  prefixes (and/or sampling) so userspace never sees the firehose.
- [ ] **Identity & lifecycle:** `setuid`/`setgid`/capability changes (`CredEvent`) and `fork`/`exit`
  (feeding M2's process tree) — the events that turn raw execs into an attributable session.
- [ ] Add `ConnectEvent`/`FileEvent`/`CredEvent`/`ExitEvent` to `common` sharing `EventHeader`; one
  ring buffer, demux by `kind`. Additive ABI only. No new pipeline.
- [ ] **Cost control:** per-cgroup token-bucket rate limiting in a BPF map; configurable ringbuf size;
  measure overhead under load against the budget.
- [ ] **Tests:** VM/kind integration for IPv4 + IPv6 connect, DNS, sensitive-path open, and cred
  change, each enriched; zero ringbuf loss under a file-open storm with in-kernel filtering on.

## M4 — GPU / AI telemetry ⭐ (the differentiator)
Per-pod GPU and inference signal generic kernel tools cannot produce — the wedge. **Demo:** per
inference pod, GPU util %, memory, SM occupancy, CUDA launch attribution, and (where exposed)
tokens/sec, queue depth, KV-cache — all joined to pod identity via M2.

- [x] ADR: the hybrid GPU collector interface → [ADR-0008](docs/adr/0008-gpu-telemetry-hybrid-collector.md)
  (DCGM/NVML values + ioctl attribution + mock; three pluggable sources, degrade when one is absent).
- [ ] **Values (source of truth): NVML/DCGM** via the `nvml-wrapper` crate (model after
  `dcgm-exporter`). `nvmlDeviceGetComputeRunningProcesses` → per-PID **GPU memory** (per-PID
  *utilization* via `nvmlDeviceGetProcessUtilizationSamples`/DCGM) → join `PID → cgroup → pod`.
  Handles util, mem, SM occupancy, MIG.
- [ ] **Attribution (the eBPF wedge): trace the NVIDIA driver `ioctl` boundary** (`/dev/nvidia*` +
  `/dev/nvidia-uvm`) for *which pod is on the GPU* — immune to static linking. `libcudart` uprobes
  stay an opportunistic extra where symbols exist.
- [ ] **Inference KPIs:** scrape the serving runtime's Prometheus endpoint (**vLLM** exposes
  tokens/sec, queue depth, KV-cache) rather than fragile uprobes into the framework.
- [ ] Emit a periodic `GpuStatEvent` per pod (userspace-originated), unified downstream.
- [ ] **Mock/synthetic collector** so the pipeline + rules (M5) are testable **without GPU hardware**;
  real-GPU e2e is a documented manual test on a spot GPU node.
- [ ] NVIDIA first; keep the interface vendor-neutral (AMD/ROCm later — the seam that pays off at M9).

> **Dragon — static `libcudart`:** vLLM/TGI/TensorRT-LLM frequently static-link CUDA into a fat
> `.so`, so a `cudaLaunchKernel` uprobe has nothing to attach to and silently no-ops. The ioctl
> boundary is the durable attribution signal; **DCGM/NVML remain authoritative for metric values**
> (the ioctl ABI drifts across driver versions — version-gate it, treat it as activity, not numbers).

## M5 — Detection engine + policy language
Turn the event stream into **alerts** via a declarative, hot-reloadable **policy language** — Falco's
intent, expressed as a real compiled rule language rather than ad-hoc matching. **Demo:** author and
compile a rule, trigger "shell spawned in an inference pod" and "unexpected egress from a GPU node" →
structured alerts on the local sink.

- [ ] ADR: the policy language + the stateful-evaluation model + the compilation/versioning story.
- [ ] **Policy language (the platform jump, not just a matcher):** a YAML/expression surface with a
  real **lexer → typed AST → validator → compiler** to an evaluation form. Match event kind + fields
  (`comm`, `exe`, file path glob, dst CIDR/port, DNS name, GPU thresholds) **and pod context** (label
  selectors, workload kind) with `and/or/not`, severity, and reusable macros/lists. Type-checked at
  compile time so a bad rule fails to load, never at event time.
- [ ] **Stateful evaluation:** maintain per-pod state from enrichment + process tree + GPU signal —
  "shell *in an inference pod*" needs pod labels/serving-image (M2); "*unexpected* egress" needs a
  baseline allowlist; crypto-miner heuristics combine a GPU-util spike (M4) with mining-port egress.
- [ ] **Output:** `Alert { rule_id, severity, event, pod, ts, message }` with dedup/throttling; local
  sinks = structured JSON logs + a Prometheus alert counter (value with zero cloud). Hot-reload via
  `inotify`, atomic compile-then-swap (never serve a half-applied ruleset).
- [ ] **Starter ruleset:** shell-in-inference-pod, unexpected-egress, sensitive-file-read,
  crypto-miner-heuristic, privilege-escalation (from M3 cred events).
- [ ] **Tests:** golden-fixture `event → expected alert(s)` per rule (incl. stateful cases); the
  compiler rejects malformed/ill-typed rules on load; hot-reload drops no events; a fuzz target on the
  rule parser (deepened in M9).

## M6 — Enforcement (optional)
Act on detections — kill or block — with strong safety rails. Optional because kernel support varies;
the agent stays fully useful observe-only. **Demo:** enforce mode kills a flagged exec and fails a
denied egress; audit mode logs the same decisions without acting.

- [ ] ADR: the enforcement model, safety rails, and the portability ladder.
- [ ] **Kill-on-match (broadest):** `bpf_send_signal(SIGKILL)` from the exec probe (kernel ≥ 5.3),
  decision from a userspace-updated (or in-kernel fast-path) policy map.
- [ ] **Egress deny:** flip the M3 `cgroup/connect4|6` hook to deny — **no BPF-LSM required**, works
  on most managed clusters.
- [ ] **BPF-LSM deny (where available):** LSM hooks (`bprm_check_security`, `socket_connect`,
  `file_open`) → `-EPERM`. Requires kernel ≥ 5.7, `CONFIG_BPF_LSM`, and `bpf` in the active LSM list.
- [ ] **Preflight + degrade loudly** (uses M0's `/sys/kernel/security/lsm` read): if `bpf` is absent,
  `WARN` and fall back to `bpf_send_signal` + `cgroup/connect`. **`BPF_PROG_TYPE_LSM` programs fail to
  *attach* (`EINVAL`) when bpf isn't registered — a hard precondition, not a silent skip.**
- [ ] **Audit-first, fail-open by default:** every policy ships dry-run; enforcement opt-in per rule;
  an allowlist prevents self-lockout (the agent must never kill itself).
- [ ] **Tests:** audit logs without acting; enforce verified in a VM per supported backend;
  self-protection allowlist proven; staged rollout (audit → one namespace → cluster).

> **Dragon — LSM stacking:** on hardened distros (Flatcar / Ubuntu Pro / RHEL) with AppArmor/SELinux
> primary, LSM runs every registered module per hook and short-circuits only on a *deny* — an
> upstream *allow* still reaches our hook, but if `bpf` isn't in the boot `lsm=` list we can't attach
> at all. Preflight, then degrade.

## M7 — Exporter + packaging (OSS → fleet product)
Ship events/alerts to `agent-cloud` over a stable contract and make the agent trivially deployable —
while staying **offline-first**. **Demo:** `helm install` → DaemonSet on every node (incl. GPU);
events stream over mTLS; killing the endpoint doesn't disrupt local operation.

- [ ] ADR: the exporter contract (proto schema) and the packaging/security posture.
- [ ] **Exporter (`crates/exporter`):** `tonic` gRPC client streaming `Event`/`Alert`/`GpuStat`. The
  **proto schema lives in `common`/`proto`** — the open-core contract `agent-cloud` imports. Optional
  OTLP sink for teams already on an OpenTelemetry collector.
- [ ] Batching, backpressure, **mTLS + auth token**, retry/backoff, at-least-once with a bounded local
  buffer (optional disk spool). Strictly optional — disabled, the agent is fully functional.
- [ ] **Local-first sinks:** Prometheus `/metrics` (self-metrics + event/alert counters) + structured
  JSON logs — full value with zero cloud.
- [ ] **Packaging:** multi-stage image (needs `CAP_BPF`/`CAP_PERFMON`/`CAP_SYS_ADMIN`/`CAP_SYS_RESOURCE`,
  `hostPID`, host mounts for `/sys/fs/cgroup`, `/sys/kernel/btf`, the CRI socket). **DaemonSet + Helm +
  Kustomize**: least-privilege ClusterRole (`get/list/watch` pods/nodes), named capabilities over
  blanket `privileged`, tolerations for **all** nodes incl. GPU, priorityClass, limits, health/ready
  probes.
- [ ] **Docs:** install, configure, write-a-rule, architecture, the security model, the kernel
  requirements matrix.
- [ ] **Tests:** stream to a stub cloud over mTLS; endpoint death → buffer + reconnect, no local
  disruption; RBAC least-privilege verified by removing perms and seeing graceful degradation.
- [ ] CI check: `agent` never depends on `agent-cloud`.
- [ ] Tag `v0.7.0`.

## M8 — Fleet control & multi-tenancy
The platform jump on the agent side: make a single agent **fleet-manageable** — pull signed policy,
report health, accept a fixed, audited verb set from the control plane — without ever giving up
offline-first operation or the one-way dependency. **Demo:** push a signed policy bundle from a stub
control plane; it rolls out audit → canary → fleet; a second tenant's rules never touch the first;
kill the plane and the agent keeps enforcing the last-known-good bundle.

- [ ] ADR: the fleet control channel (bidirectional gRPC over the M7 mTLS link) and the
  policy-bundle distribution + versioning model.
- [ ] **Signed policy bundles:** rules + allowlists + GPU baselines packaged as **content-addressed,
  cosign-signed OCI artifacts**; the agent pulls/receives, **verifies signature + digest before load**,
  compiles (M5), and hot-swaps atomically — rolling back on compile or health regression.
- [ ] **Fleet identity & enrollment:** per-node identity (SPIFFE/X.509 SVID), enrollment +
  attestation, so the plane authenticates each agent and an agent only accepts policy for its tenant.
- [ ] **Bidirectional control channel:** the plane can push config/policy and request a **fixed verb
  set** (snapshot, profile, drain, re-sync) — capability-gated and fully audited. **Never a
  remote-code path:** only declarative policy + that closed verb set, so the blast radius of a
  compromised plane is bounded.
- [ ] **Multi-tenancy:** per-tenant rule namespaces, quotas, and event routing with hard isolation;
  one tenant can neither see nor perturb another's policy or signal.
- [ ] **Staged rollout & canary:** a bundle version rolls out audit → a canary node set → namespace →
  fleet, with **automatic rollback** on error-rate or agent-health regression.
- [ ] **Offline-first preserved (the invariant under stress):** unreachable plane ⇒ keep running on
  the stale-but-valid last-known-good bundle; reconcile on reconnect. Zero cloud is still a valid mode.
- [ ] **Tests:** bundle signature/digest verify + rollback; tenant isolation (no cross-tenant leak);
  control-channel authz on every verb; split-brain convergence; offline survival + reconcile.

> **Dragon — split-brain policy:** a flapping channel or two sources deliver conflicting bundle
> versions; the agent must converge **deterministically** (monotonic version + content digest, prefer
> last-known-good) and **never oscillate enforcement** — flicker between allow/deny is worse than a
> stale-but-stable decision.

## M9 — Hardening, scale & performance
Make it production-grade at fleet scale across the whole support matrix — the difference between "it
runs" and "it runs on every node we have, forever, without becoming the problem." **Demo:** the
kernel + arch matrix is green in CI; the perf budget holds on a 250-pod node under a syscall storm; a
24h soak shows no leaks; chaos (plane death, GPU driver reload, kernel pressure) degrades gracefully.

- [ ] ADR: the performance budget, the load-shedding model, and the supply-chain posture.
- [ ] **Perf budget enforced in CI:** a benchmark harness with a repeatable workload; a regression
  gate on CPU / memory / per-event latency; **published numbers** in the support matrix.
- [ ] **Load shedding & adaptive sampling:** under pressure, per-cgroup rate limits + adaptive
  sampling degrade gracefully and **never block the drain or the kernel** — bounded everything.
- [ ] **Kernel + arch matrix green:** the `ebpf-smoke` microVM matrix across pinned kernels
  (5.8 … latest) **and `aarch64` (arm64) in addition to `x86_64`**; gaps documented, not hidden.
- [ ] **Multi-distro validation:** GKE/EKS/AKS + bare metal + hardened distros
  (Flatcar/Bottlerocket/RHEL); LSM-stacking handled; second GPU vendor (AMD/ROCm) seam exercised.
- [ ] **Fuzzing & property tests:** fuzz the event decoders, the rule compiler, and the cgroup/path
  parsers; property tests on the ABI round-trip and enrichment cache invariants.
- [ ] **Chaos & soak:** a multi-hour soak proving bounded caches (no leaks); chaos suites (kill the
  plane, reload the GPU driver mid-stream, apply memory/CPU pressure) with graceful degradation.
- [ ] **Supply chain:** reproducible builds, signed images + SBOM (cosign/syft), pinned deps,
  `cargo deny` and advisory gates green; self-protection (watchdog, can't be starved into a blind spot).
- [ ] **Tests:** the matrix is the test — every row above is a CI lane or a documented manual gate.

> **Dragon — the observer that became the bottleneck:** at 250 pods × a high syscall rate, naive
> enrichment or an unbounded cache makes the agent the noisy neighbor it exists to watch. Bound
> everything; measure against the **worst case**, not the average; shed load before you block.

## M10 — Platform GA (v0.10.0) ⭐
Consolidate everything into a stable, extensible, documented platform release — the goal line.
**Demo (the whole thing, end to end):** fresh multi-node cluster → `helm install` → fleet-managed
agents → a GPU inference workload → push a signed policy bundle → drop a shell in the inference pod →
caught, attributed to the model/pod, enforced, and streamed to the (stub) control plane.

- [ ] ADR: the **v1 ABI/proto stability guarantee** and the plugin/extension interface.
- [ ] **Stable contract:** freeze `EventHeader` + the event ABI + the exporter proto as **v1**
  (additive-only thereafter); a **conformance suite** asserts a build honors the contract.
- [ ] **Plugin / extension SDK:** a documented boundary (trait-based and/or WASM) for third-party
  sinks, collectors, and rule functions — the platform extends **without forking core**.
- [ ] **Conformance & certification suite:** a runnable suite a deployment passes to claim
  "agent-compatible" — kernel features, enrichment correctness, rule semantics, enforcement safety.
- [ ] **Full docs site:** install, operate, write-a-rule, write-a-plugin, architecture, security
  model, support matrix, published performance numbers, and the upgrade/compatibility policy.
- [ ] **Release engineering:** SemVer + deprecation policy, a stated support window, migration guides;
  the path from `v0.10.0` toward a future `1.0` SemVer commitment is written down.
- [ ] **Tests:** the conformance suite green on the full kernel/arch/distro matrix; the end-to-end
  platform demo automated in `kind` + a manual GPU-node run.
- [ ] Tag `v0.10.0` — **the platform release.**

---

## Cross-cutting standards (apply to every milestone)

**Kernel & platform support matrix** — maintained in [`docs/support-matrix.md`](docs/support-matrix.md)
(enforced at startup by the boot preflight), test against:
- Min kernel **5.8** (ring buffer); BPF-LSM (M6) needs **5.7+** with `CONFIG_BPF_LSM` and `bpf` in the
  active LSM list (`lsm=...,bpf`) — preflighted via `/sys/kernel/security/lsm`. Requires CO-RE/BTF
  (`CONFIG_DEBUG_INFO_BTF`) and **cgroup v2**.
- Validate on GKE/EKS/AKS + at least one bare-metal kernel; **`x86_64` and `arm64`** (M9); document
  where BTF or BPF-LSM is absent.

**Testing ladder:**
- Unit (userspace): enrichment parsers, the rule compiler, decoders — table-driven fixtures.
- eBPF load/verifier tests in a microVM (`lvh`/qemu) in CI — verifier regressions the runner can't catch.
- Integration in `kind`: enrichment, end-to-end event → alert, RBAC, fleet bundle rollout.
- e2e on a real GPU node (manual/spot): M4 numbers + the full signature demo.
- CI kernel + arch matrix: build + load across kernel versions and `x86_64`/`arm64`.
- Fuzz + property + soak + chaos (M9): decoders, rule compiler, parsers; bounded-cache and ABI invariants.

**Performance & overhead budget:** set a concrete budget (e.g. low single-digit % CPU on a busy
node) and track it from M3, **gate it in CI from M9**. Levers: in-kernel filtering (M3), per-cgroup
rate limiting, adaptive sampling, ringbuf sizing, minimal per-event work. Benchmark with a repeatable
workload; publish numbers in the support matrix.

**Dependency / sequencing graph:**
```
M0 ─▶ M1 ─▶ M2 ─┬─▶ M3 ─▶ M5 ─▶ M6 ─┐
                └─▶ M4 ─────┘        │
                                     ▼
                          M7 ─▶ M8 ─▶ M9 ─▶ M10 (GA / v0.10.0)
```
M2 is the keystone — it unblocks every downstream signal. M3 and M4 are independent after M2 and
parallelizable; M5 needs M3+M4 signal; M6 builds on M3/M5. M7 (exporter contract) firms up with
M5/M6 and unblocks M8 (fleet, which speaks that contract). **M9 (hardening/scale) runs continuously
from M3 onward but is the hard gate before M10 GA.**

### Scale & LOC budget (the `0.10.0` target)
`v0.10.0` is the platform release, budgeted at **~100k lines of authored Rust** (generated `vmlinux`
+ vendored protos excluded; ~30% is tests). The rough allocation — the *shape* of the system, not a
quota:

| Area | ~kLOC | What it is |
|------|-------|------------|
| eBPF programs (`crates/ebpf`) | ~10 | full syscall surface + GPU ioctl + LSM hooks, CO-RE across kernels |
| `common` ABI + proto | ~3 | the kernel⇄userspace⇄cloud contract |
| enrich (`crates/enrich`) | ~12 | cgroup→container→pod, process-tree reconstruction, identity, cache |
| rules / detection (`crates/rules`) | ~12 | the policy language (lexer→AST→compiler), stateful eval, rulesets |
| enforcement | ~8 | LSM/signal/egress backends, safety rails, the portability ladder |
| GPU/AI telemetry | ~10 | NVML/DCGM, ioctl attribution, inference KPIs, MIG, vendor seam |
| exporter + local sinks | ~6 | gRPC/OTLP, batching/backpressure, disk spool, `/metrics` |
| fleet / control (`crates/fleet`) | ~8 | signed bundles, identity, multi-tenancy, staged rollout |
| packaging / operability / config / self-protection | ~6 | Helm/DaemonSet, config model, watchdog, health |
| tests (VM matrix, kind e2e, golden, fuzz, soak) | ~25 | proportional to running-in-the-kernel risk |
| **Total** | **~100** | the platform at `v0.10.0` |

This is a **multi-year, multi-engineer scope**. The honest unit of progress is the **milestone tag +
demo + green CI**, not the line count — the LOC budget exists to size ambition and catch scope drift,
and **we never pad code to hit it**. If a milestone lands its demo in far fewer lines, that's a win,
not a miss; if it balloons past its row, that's a signal to split it.

---

## Architectural invariants (never traded away)

> Each invariant traces to a decision record in [`docs/adr/`](docs/adr/) (ADR-0001…0008).

- **The kernel is the source of truth; `common` is the contract.** Every `#[repr(C)]` event + the
  exporter proto is ABI: additive changes only, `EventHeader.version` + proto field numbers carry
  compatibility, never reorder/resize existing fields. Types are **padding-free and `const`-asserted**;
  `ktime_ns`/`cgroup_id`/`mnt_ns_inum` are captured in-kernel. `synced`/`PodMeta` are userspace
  annotations, not kernel ABI. The crate stays `no_std` and dependency-light. **Frozen as v1 at M10.**
- **The verifier is the gate.** Correctness before performance. Zero the reserved ring-buffer slot
  before writing; keep events padding-free; bound every loop and `bpf_probe_read`. A program that
  doesn't load doesn't ship.
- **Capture is never gated on enrichment (cold-start contract).** Attach probes first (no blind
  window), seed the baseline from `/proc`, node-scope the kube List, park cache-misses in a bounded
  resync queue that emits `Unknown` rather than dropping, flag every event `synced`. Detail in
  [`.rules`](./.rules).
- **One self-contained binary, no sidecars.** eBPF embedded via aya; shipped as a single DaemonSet
  binary; `xtask` is dev-only. Minimal blast radius and attack surface.
- **One-way dependency: cloud → OSS, and offline-first.** This repo never imports `agent-cloud`; the
  agent is fully usable self-hosted with zero cloud. Fleet management (M8) is a *fixed, audited verb
  set + signed declarative policy* — **never a remote-code path** — and a stale last-known-good bundle
  keeps the agent fully operational when the plane is gone. CI enforces the no-import rule.
- **Every feature points at AI/GPU workloads** — the wedge generic tools miss is the only durable edge.
- **Privileged but self-protecting:** least-privilege RBAC, named capabilities over `privileged`,
  pinned/signed supply chain (M9); the agent must never disable, starve, or kill itself, and bounds
  every cache so it can't become the bottleneck it watches for.
- **SemVer + a git tag per milestone**; the agent exports its own health/event/drop counters via
  `/metrics` and readiness probes (from M7; informally earlier). `0.10.0` is the platform goal line;
  the route to a `1.0` SemVer commitment is written down at M10.
