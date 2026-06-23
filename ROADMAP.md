# Roadmap — `agent` (OSS, Rust + eBPF/aya)

## §0 The spine

**`agent` is the open-source eBPF node agent you run in your own cluster; `agent-cloud` is the
hosted fleet control plane — the *same* event contract, aggregated across clusters, run for teams
who don't want to operate that control plane themselves.** The model is **open-core** (Falco →
Sysdig, Tetragon → Isovalent, Parca → Polar Signals): you run the agent free and self-hosted
forever; when you need fleet-wide aggregation, storage, analytics, and compliance without standing
up that infra, you point your agents at the cloud. The moat is **GPU/AI-workload awareness** — the
un-crowded slice generic tools miss — not generic k8s observability.

The shape it builds toward:

```
            kernel space                      user space (crates/agent)                 off-node
 ┌───────────────────────────┐   ringbuf   ┌──────────────────────────────┐   gRPC   ┌────────────┐
 │ eBPF programs (crates/ebpf)│ ─────────▶ │ loader → decode → enrich →   │ ───────▶ │ agent-cloud│
 │  exec / connect / open /   │  (events)  │   rules → sinks               │ (M7)     │  (private) │
 │  LSM hooks                 │ ◀───────── │      ▲            │           │          └────────────┘
 │  policy maps               │   policy   │  crates/enrich  crates/rules  │
 └───────────────────────────┘            │  cgroup→container→pod cache   │
        ▲ CO-RE / BTF                      │   + /proc baseline seed       │
        │                                  └──────────────┬───────────────┘
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
   with zero cloud**. This repo never imports `agent-cloud`; CI enforces it.
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

Each milestone is a **git tag + a working demo + green CI**; we don't start one until the prior is
green. Numbering (`M0`…`M7`) is shared with [`LEARN.md`](./LEARN.md) and [`.rules`](./.rules).

---

## Milestone index

| M | Milestone | Tag | Demo (observable outcome) | Cert |
|---|-----------|-----|---------------------------|------|
| 0 | **Scaffold & CI** | `v0.0.0` | `cargo xtask run` loads a no-op program on a real kernel; CI green for BPF + userspace | — |
| 1 | **First probe (exec)** ⭐ | `v0.1.0` | run `/bin/ls` → typed `ExecEvent` (pid/ppid/uid/comm/cgroup) in userspace via ring buffer | — |
| 2 | **k8s enrichment** ⭐ | `v0.2.0` | `kubectl exec` into a pod → event names the **pod/namespace/container** | CKS, RBAC |
| 3 | **Network + file probes** | `v0.3.0` | `curl` from a pod → `ConnectEvent`; read a sensitive path → `FileEvent`, both enriched | CKS |
| 4 | **GPU/AI telemetry** ⭐ | `v0.4.0` | per-pod GPU util/mem + inference KPIs joined to k8s identity (the differentiator) | — |
| 5 | **Detection engine** | `v0.5.0` | declarative rules → alerts: "shell in an inference pod", "unexpected egress from a GPU node" | CKS |
| 6 | **Enforcement (optional)** | `v0.6.0` | kill-on-match (`bpf_send_signal`) + egress deny; BPF-LSM deny where supported; audit-first | CKS |
| 7 | **Exporter + packaging** | `v0.7.0` | gRPC stream to control plane; DaemonSet + Helm + RBAC; Prometheus `/metrics`; docs | CKA/CKS |

> **Stop-and-ship:** after **M2** you have the signature demo — *drop a shell in a GPU inference pod
> → the agent catches it and names the pod, in real time.* M4 (GPU) is the wedge; if time is short,
> **M0 → M2 → M4 → M5** is the highest-signal portfolio path and M3/M6/M7 can follow.

---

## M0 — Scaffold & CI
A reproducible build of both a BPF object and the userspace binary, loadable on a real kernel, gated
by CI. No probes yet — just the skeleton everything hangs on.

- [x] Cargo workspace + the `common` crate (the ABI spine; `Cargo.toml` with `members = ["crates/*"]`).
- [x] Scaffold the remaining crates: `ebpf` (`no_std`, BPF target), `agent` (userspace, tokio),
  `xtask` (build orchestration). `enrich`/`rules`/`exporter` deferred to their milestone.
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
- [ ] **Boot preflight** (shipped now, relied on everywhere after): probe BTF present, cgroup v2,
  kernel ≥ 5.8, and read `/sys/kernel/security/lsm`; if `bpf` is absent, `WARN` that LSM enforcement
  (M6) can't attach and will degrade.
- [ ] CI: `cargo build` (default-members) + the eBPF object build, `cargo clippy -- -D warnings`,
  `cargo fmt --check`, `cargo deny check`.
- [ ] CI: **eBPF load/verifier smoke-test in a microVM** (`lvh`/qemu, known kernel) — catches
  verifier/load regressions the bare GitHub runner can't.
- [x] ADR template + [`docs/adr/`](docs/adr/) log; foundational decisions recorded as ADR-0001…0008.
- [ ] Seed the **kernel/platform support matrix** doc.
- [ ] Tag `v0.0.0` (loads + attaches a trivial no-op program, detaches cleanly on `Drop`).

## M1 — First probe: process execution ⭐
The end-to-end pipeline for one event type — kernel hook → ring buffer → typed struct in userspace.
**Demo:** start the agent, run `/bin/ls`, see a decoded `ExecEvent` with the right pid/ppid/uid/comm.

- [x] **Define the event ABI in `crates/common`** — `EventHeader` (with in-kernel `ktime_ns`),
  `EventKind`, and `ExecEvent` (incl. `cgroup_id` **and** `mnt_ns_inum`). Padding-free by
  construction (u64s first); `const _: () = assert!(size_of::<T>() == …)` guards the layout.
- [ ] **Hook:** raw tracepoint `sched/sched_process_exec` — the canonical "a process started" signal,
  post-exec so `comm`/exe are stable (over an arch-specific `kprobe` on `__x64_sys_execve`).
- [ ] **In-kernel capture:** `pid/tgid`, `uid/gid`, `comm`, `cgroup_id` (`bpf_get_current_cgroup_id`),
  `ppid` (`task → real_parent → tgid`), and the **mount-ns inode** `task → nsproxy → mnt_ns → ns.inum`
  (all CO-RE); stamp `ktime_ns` via `bpf_ktime_get_ns()`.
- [ ] **Write discipline:** reserve a ring-buffer slot, **zero the whole slot, then write fields in
  place** — never build on the 512-byte stack and copy. (The verifier rejects any uninitialized,
  incl. padding, byte reaching `bpf_ringbuf_submit` as *invalid indirect read from stack*.)
- [ ] **Userspace:** consume `aya::maps::RingBuf` async (tokio `AsyncFd`); cast bytes → struct
  (`bytemuck`/`aya::Pod`); dispatch on `hdr.kind`; handle partial reads + shutdown (detach on `Drop`).
- [ ] **Loader guard:** validate the ring-buffer size is a **power of two and a page multiple**
  before `bpf_map_create` (else `-EINVAL` with no diagnostic). *(Moved from M0 — the ring buffer
  first exists here.)*
- [ ] Bounded `argv` capture (a few args, truncated) via `bpf_probe_read_user` under `#[unroll]` —
  every read guarded for the verifier.
- [ ] **Tests:** deterministic VM integration test (spawn a known binary → assert one `ExecEvent`);
  zero events lost in a tight exec loop; no verifier rejections.

> **Why `mnt_ns_inum` + `ktime` from M1, not later:** the mount-ns inode is a slower-recycling
> *secondary* identity key that lets M2 reconcile short-lived pods whose cgroup dir is unlinked
> before userspace resolves it; the in-kernel timestamp stops late enrichment from distorting event
> time. `synced`/`PodMeta` are **userspace annotations, not part of this kernel ABI** (see M2).

## M2 — Kubernetes enrichment ⭐ (the keystone)
Turn a kernel `cgroup_id` into **pod / namespace / container / workload** — the join that makes
everything else valuable, and the genuinely hard userspace engineering. **Demo (stop-and-ship):**
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

## M3 — Network + file probes
Broaden signal to egress and file access, reusing the M1 envelope and M2 enrichment. **Demo:**
`curl` from a pod → enriched `ConnectEvent`; `cat /etc/shadow` → enriched `FileEvent`.

- [ ] **Network egress:** `kprobe`/`fexit` on `tcp_v4_connect`/`tcp_v6_connect`, reading
  `saddr/daddr/sport/dport/family` from `struct sock` via CO-RE. (Choose `cgroup/connect4|6`
  `BPF_PROG_TYPE_CGROUP_SOCK_ADDR` where it fits — **reused for enforcement in M6**, since it denies.)
- [ ] **File access:** `fentry`/tracepoint on `do_sys_openat2`/`sys_enter_openat`; bounded path, flags,
  result. **Filter in-kernel** — opens are high-volume — via an `LPM_TRIE`/`HASH` of sensitive path
  prefixes (and/or sampling) so userspace never sees the firehose.
- [ ] Add `ConnectEvent`/`FileEvent` to `common` sharing `EventHeader`; one ring buffer, demux by
  `kind`. No new pipeline.
- [ ] **Cost control:** per-cgroup token-bucket rate limiting in a BPF map; configurable ringbuf size;
  measure overhead under load against the budget.
- [ ] **Tests:** VM/kind integration for IPv4 + IPv6 connect and sensitive-path open, each enriched;
  zero ringbuf loss under a file-open storm with in-kernel filtering on.

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
- [ ] NVIDIA first; keep the interface vendor-neutral (AMD/ROCm later).

> **Dragon — static `libcudart`:** vLLM/TGI/TensorRT-LLM frequently static-link CUDA into a fat
> `.so`, so a `cudaLaunchKernel` uprobe has nothing to attach to and silently no-ops. The ioctl
> boundary is the durable attribution signal; **DCGM/NVML remain authoritative for metric values**
> (the ioctl ABI drifts across driver versions — version-gate it, treat it as activity, not numbers).

## M5 — Detection engine
Turn the event stream into **alerts** via declarative, hot-reloadable rules — Falco's intent without
its full DSL. **Demo:** trigger "shell spawned in an inference pod" and "unexpected egress from a GPU
node" → structured alerts on the local sink.

- [ ] ADR: the rule schema + the stateful-evaluation model.
- [ ] **Rule schema (YAML):** match event kind + fields (`comm`, `exe`, file path glob, dst CIDR/port,
  GPU thresholds) **and pod context** (label selectors, workload kind) with `and/or/not` and per-rule
  `severity`.
- [ ] **Stateful evaluation:** maintain per-pod state from enrichment + GPU signal — "shell *in an
  inference pod*" needs pod labels/serving-image (M2); "*unexpected* egress" needs a baseline
  allowlist; crypto-miner heuristics combine a GPU-util spike (M4) with mining-port egress.
- [ ] **Output:** `Alert { rule_id, severity, event, pod, ts, message }` with dedup/throttling; local
  sinks = structured JSON logs + a Prometheus alert counter (value with zero cloud). Hot-reload via
  `inotify`.
- [ ] **Starter ruleset:** shell-in-inference-pod, unexpected-egress, sensitive-file-read,
  crypto-miner-heuristic.
- [ ] **Tests:** golden-fixture `event → expected alert(s)` per rule (incl. stateful cases); malformed
  rule files rejected on load; hot-reload drops no events.

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
  **proto schema lives in `common`/`proto`** — the open-core contract `agent-cloud` imports.
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
- [ ] Tag `v0.7.0`; file remaining items as tracked issues.

---

## Cross-cutting standards (apply to every milestone)

**Kernel & platform support matrix** — maintain in `docs/`, test against:
- Min kernel **5.8** (ring buffer); BPF-LSM (M6) needs **5.7+** with `CONFIG_BPF_LSM` and `bpf` in the
  active LSM list (`lsm=...,bpf`) — preflighted via `/sys/kernel/security/lsm`. Requires CO-RE/BTF
  (`CONFIG_DEBUG_INFO_BTF`) and **cgroup v2**.
- Validate on GKE/EKS/AKS + at least one bare-metal kernel; document where BTF or BPF-LSM is absent.

**Testing ladder:**
- Unit (userspace): enrichment parsers, rule engine, decoders — table-driven fixtures.
- eBPF load/verifier tests in a microVM (`lvh`/qemu) in CI — verifier regressions the runner can't catch.
- Integration in `kind`: enrichment, end-to-end event → alert, RBAC.
- e2e on a real GPU node (manual/spot): M4 numbers + the full signature demo.
- CI kernel matrix: build + load across a few kernel versions.

**Performance & overhead budget:** set a concrete budget (e.g. low single-digit % CPU on a busy
node) and track it. Levers: in-kernel filtering (M3), per-cgroup rate limiting, ringbuf sizing,
minimal per-event work. Benchmark with a repeatable workload; publish numbers.

**Dependency / sequencing graph:**
```
M0 ─▶ M1 ─▶ M2 ─┬─▶ M3 ─▶ M5 ─▶ M6
                ├─▶ M4 ─────┘ (GPU rules need M4)
                └─▶ M7  (packaging can start after M2; exporter proto firms up with M5/M6)
```
M2 is the keystone — it unblocks every downstream signal. M3 and M4 are independent after M2 and
parallelizable. M5 needs M3+M4 signal; M6 builds on M3/M5; M7 can begin once M2 stabilizes.

---

## Architectural invariants (never traded away)

> Each invariant traces to a decision record in [`docs/adr/`](docs/adr/) (ADR-0001…0008).

- **The kernel is the source of truth; `common` is the contract.** Every `#[repr(C)]` event + the
  exporter proto is ABI: additive changes only, `EventHeader.version` + proto field numbers carry
  compatibility, never reorder/resize existing fields. Types are **padding-free and `const`-asserted**;
  `ktime_ns`/`cgroup_id`/`mnt_ns_inum` are captured in-kernel. `synced`/`PodMeta` are userspace
  annotations, not kernel ABI. The crate stays `no_std` and dependency-light.
- **The verifier is the gate.** Correctness before performance. Zero the reserved ring-buffer slot
  before writing; keep events padding-free; bound every loop and `bpf_probe_read`. A program that
  doesn't load doesn't ship.
- **Capture is never gated on enrichment (cold-start contract).** Attach probes first (no blind
  window), seed the baseline from `/proc`, node-scope the kube List, park cache-misses in a bounded
  resync queue that emits `Unknown` rather than dropping, flag every event `synced`. Detail in
  [`.rules`](./.rules).
- **One self-contained binary, no sidecars.** eBPF embedded via aya; shipped as a single DaemonSet
  binary; `xtask` is dev-only. Minimal blast radius and attack surface.
- **One-way dependency: cloud → OSS.** This repo never imports `agent-cloud`; the agent is fully
  usable self-hosted with zero cloud. CI enforces it.
- **Every feature points at AI/GPU workloads** — the wedge generic tools miss is the only durable edge.
- **Privileged but self-protecting:** least-privilege RBAC, named capabilities over `privileged`,
  pinned/signed supply chain; the agent must never disable or kill itself.
- **SemVer + a git tag per milestone**; the agent exports its own health/event/drop counters via
  `/metrics` and readiness probes (from M7; informally earlier).
