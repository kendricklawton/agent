# 0003 — Stable-root / nightly-eBPF toolchain split

- **Status:** Accepted
- **Date:** 2026-06-23
- **Deciders:** K-Henry
- **Milestone:** M0

## Context
aya's BPF target (`bpfel-unknown-none`) still requires the **nightly** toolchain (`-Z build-std=core`
+ `rust-src` + `bpf-linker`). The rest of the workspace — `agent`, `agent-common`, `xtask` — should
stay on **stable** to match the house style (pinned stable, edition 2024, the `deny`-unwrap/expect/
panic lint gate) and to avoid exposing the large userspace surface to nightly breakage.

The friction: `build.rs` runs under the *invoking* toolchain, so a stable workspace root cannot
naïvely drive the nightly `build-std` eBPF build. Worse, the outer (stable) cargo injects
`RUSTUP_TOOLCHAIN`, which overrides any `rust-toolchain.toml` for nested invocations.

## Decision
We will **split the toolchain by directory**:
- The workspace root `rust-toolchain.toml` pins **stable**.
- `crates/ebpf/rust-toolchain.toml` pins **nightly** (directory-scoped), with `rust-src`.
- `crates/agent/build.rs` cross-compiles the eBPF crate by invoking cargo **with the working
  directory set to `crates/ebpf`** and **clearing `RUSTUP_TOOLCHAIN`** (plus other injected vars), so
  rustup honors the eBPF crate's nightly pin. A dedicated `--target-dir` avoids the main build lock.
- `cargo xtask build|run` are the canonical entrypoints; the eBPF object is embedded into the agent
  via `include_bytes_aligned!`.

## Consequences
- ~95% of the code (everything but the eBPF crate) builds, lints, and tests on **stable** — full
  house-style gate applies.
- Nightly is **quarantined** to one crate; reproducibility improves further once the nightly date is
  pinned (currently floating — a follow-up).
- `build.rs` carries non-obvious env-scrubbing logic; it's commented and is the trickiest file in the
  repo.
- `crates/ebpf` is excluded from `default-members` so a bare `cargo build` ignores it (it can't
  compile for the host target).

## Alternatives considered
- **Pin nightly workspace-wide** — simplest, but puts the entire userspace surface on nightly for no
  benefit; against the house style. Rejected.
- **`aya-build` in `build.rs` under a stable root** — `aya-build` assumes the ambient toolchain is
  nightly (it passes `-Z build-std`); under a stable root it fails. Rejected in favor of the explicit
  directory-scoped invocation.
