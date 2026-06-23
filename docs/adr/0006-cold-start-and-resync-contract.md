# 0006 — Cold-start & re-sync: capture never gated on enrichment

- **Status:** Accepted
- **Date:** 2026-06-23
- **Deciders:** K-Henry
- **Milestone:** cross-cutting (realized in M2)

## Context
When the DaemonSet lands on a node, workloads are **already running** — processes execing, traffic
flowing — before the userspace pod cache exists. The kube-rs reflector's first `List` takes time, and
already-running pods never fire an `exec` the agent can observe. Naïve designs either (a) wait for the
cache before attaching probes (a **blind window** where boot-time events are invisible) or (b) drop
events that can't be enriched yet (**silent loss**). Both are unacceptable for a security agent.

## Decision
We will treat **capture and enrichment as two separate clocks; capture is never gated on
enrichment**:
- **Attach probes first** — accept an initial burst of unenriched events rather than a blind window.
- **Seed the baseline from `/proc`** at boot (per-PID `cgroup`, `ns/mnt`, starttime, `comm`), then
  maintain it from exec/exit deltas.
- **Node-scope the kube List** (`fieldSelector=spec.nodeName=$NODE_NAME`) for a sub-second initial
  sync.
- **Park cache-miss events in a bounded, short-deadline resync queue** keyed on
  `(cgroup_id, mnt_ns_inum)`; re-resolve on cache updates; on deadline, emit as explicit `Unknown` —
  **never drop, never block the ring-buffer drain**. Queue-full → emit unenriched + a metric.
- **Make it auditable:** every event carries a `synced` bit (`cold_start` vs `steady`), and the
  readiness probe stays `NotReady` until the `/proc` backfill + initial List complete.

## Consequences
- No blind window (security-critical) and no silent loss; cold-start events are clearly labeled, so
  downstream rules/cloud can calibrate confidence and avoid false "unknown-pod" alerts on every
  rollout.
- The mount-namespace inode reconciles **short-lived pods** whose cgroup directory is unlinked before
  resolution (the 45 ms `cronjob` ghost).
- More complex enrichment pipeline (a resync queue, a `/proc` seeder, the `synced` flag) — accepted
  cost for correctness. Bounded memory throughout.

## Alternatives considered
- **Attach after the cache is warm** — rejected: a multi-second blind window; missing a boot-time
  shell is a security failure.
- **Drop unenriched events** — rejected: silent loss; a true event is still a true event.
- **Block the ring-buffer drain until enrichment resolves** — rejected: backpressure → kernel-side
  loss, the very thing the ring buffer choice avoids.
