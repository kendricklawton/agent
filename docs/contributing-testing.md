# Testing

Make sure you can [build](./contributing-building.md) first — you can't test what you can't build.

Unlike a typical Rust crate, `agent`'s tests fan out by **privilege and hardware**, not just by
unit-vs-integration. There are five tiers, and **the most important thing to know is which tier your
change actually needs** — most changes need only tiers 1–2, which require no root, no kernel, and no
GPU.

## The tiers

| Tier | What it proves | Needs | Command |
|------|----------------|-------|---------|
| **1 · Unit** | parsers, rule engine, decoders, the `#[repr(C)]` ABI `const`-asserts | nothing — pure host (even macOS) | `cargo test` |
| **2 · Build / lint gate** | eBPF cross-compiles + embeds; style; supply chain | Linux + nightly + `bpf-linker` | `cargo xtask build` + the lint commands below |
| **3 · eBPF load / verifier** | the program **loads and passes the verifier** on a real kernel | root + kernel ≥ 5.8 + BTF | `cargo xtask run -- --once` (CI: on-runner smoke + pinned-kernel microVM) |
| **4 · Integration (k8s)** | enrichment correctness, RBAC, event → alert end-to-end | `kind` + `docker` + a real kernel | `kind create cluster` + the integration tests |
| **5 · e2e GPU** | real GPU numbers + the signature demo | an NVIDIA GPU on Linux (or a spot node) | manual, per the M4 runbook |

### Which tier does my change need?

- **Touched a parser, a rule, a decoder, the `common` types?** → tiers 1–2. No root needed.
- **Touched the eBPF crate (a probe, a field capture, a map)?** → also tier 3 — the verifier is the
  gate; a program that doesn't load doesn't ship.
- **Touched enrichment (cgroup → container → pod), RBAC, or the cold-start path?** → also tier 4.
- **Touched the GPU collector?** → the mock collector keeps tiers 1–4 GPU-free; tier 5 validates real
  hardware.

## Tier 1 — Unit tests

```console
cargo test
```

Runs the `#[cfg(test)]` modules and any `tests/` directories across the userspace crates, plus the
compile-time `const _: () = assert!(size_of::<T>() == …)` ABI guards in `crates/common`. These need no
privilege and run on every PR — and on macOS.

## Tier 2 — Build / lint gate

The same checks CI runs first. All must be clean — run them in one shot before pushing:

```console
cargo xtask ci                                 # fmt + clippy + build + test + deny, stops at first failure
```

…or individually:

```console
cargo xtask build                              # eBPF cross-compile + embed
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check                               # advisories, licenses, bans, sources
```

`unwrap`/`expect`/`panic!` are `deny`-lints outside tests (re-allowed in tests via `clippy.toml`).

## Tier 3 — eBPF load / verifier

This is the eBPF-specific layer and the analog of a language runtime's spec-test suite: it proves the
compiled program **loads and the kernel verifier accepts it**. Locally, on a Linux box with the
[platform requirements](./contributing-building.md#platform-requirements):

```console
cargo xtask run -- --once     # uses sudo; loads, attaches, detaches cleanly on Drop, exits
```

Success = no verifier rejection, the program attaches, and it tears down on `Drop`. A verifier
rejection is a **test failure**, not a warning. In CI this runs two ways: an **on-runner smoke**
(`sudo agent --once` in `ci.yml`) on the runner's own kernel — cheap and always-on — plus a
**pinned-kernel `lvh`/qemu microVM** (`ebpf-smoke.yml`) that adds the reproducible, known-kernel
coverage the *uncontrolled* runner kernel can't give. See [CI](./contributing-ci.md).

> Common eBPF test failures and their cause live next to the code discipline that prevents them:
> zero the reserved ring-buffer slot before writing, keep `#[repr(C)]` types padding-free, and bound
> every loop and `bpf_probe_read` (the verifier rejects uninitialized/padding bytes and unbounded
> loops). See [`.rules`](../.rules) → *Code conventions*.

## Tier 4 — Integration (Kubernetes)

Brings up a real cluster to test the userspace join that makes events valuable — `cgroup_id` → pod /
namespace / container — plus RBAC and the end-to-end event → alert path:

```console
kind create cluster
# deploy a workload, kubectl exec into it, assert the event is labeled with the right pod/ns/container
```

Cover **both** containerd and CRI-O cgroup formats and **both** systemd and cgroupfs drivers, and
assert the cache stays correct across pod churn (bounded growth, lifecycle eviction). The cold-start
contract ([ADR-0006](./adr/0006-cold-start-and-resync-contract.md)) is exercised here: capture is
never gated on enrichment, cache-misses become explicit `Unknown` rather than drops.

## Tier 5 — End-to-end GPU

The wedge. The **mock/synthetic collector** keeps the pipeline and rules fully testable without
hardware (tiers 1–4); real numbers — GPU util/mem, SM occupancy, CUDA-launch attribution, vLLM KPIs —
come from a genuine NVIDIA + Linux node and are validated as a documented **manual** run. See
[ADR-0008](./adr/0008-gpu-telemetry-hybrid-collector.md). This is also where the signature demo lives:
drop a shell in a GPU inference pod → the agent catches it and names the pod, in real time.

## Adding tests

### Unit tests (tier 1)

For "unit-y" tests, add a `#[cfg(test)] mod tests` in the same file as the code:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cri_containerd_scope() {
        // table-driven: input cgroup path → expected container id
    }
}
```

Prefer **table-driven fixtures** for the parsers and the rule engine — one case per real-world cgroup
path / event shape.

### eBPF tests (tier 3)

Add a deterministic check that the program loads and behaves: spawn a known trigger (e.g. exec a known
binary), then assert exactly the expected event arrives over the ring buffer with the right fields,
and assert **zero loss** under a tight trigger loop. These belong in the pinned-kernel microVM
(`ebpf-smoke.yml`) in CI.

### Integration tests (tier 4)

Add to the `kind`-based suite: deploy a fixture workload, perform the action, assert the enriched
event. Add cases for any new cgroup/runtime format you touch.

## Performance

Per the [overhead budget](../ROADMAP.md), changes on the hot path should be benchmarked against a
repeatable workload — keep per-event work minimal, and lean on in-kernel filtering and per-cgroup
rate-limiting rather than shipping the firehose to userspace.
