# CI

CI runs on every push to `main` and every pull request. It mirrors the local
[testing tiers](./contributing-testing.md). Two workflows:

## `.github/workflows/ci.yml` — the required gate

**Job `check`** (ubuntu-latest):

1. **Toolchains** — the pinned stable installs from `rust-toolchain.toml`; nightly + `rust-src` are
   added for the eBPF cross-compile, then `bpf-linker`.
2. **Format** — `cargo fmt --all --check`.
3. **Lint** — `cargo clippy --all-targets -- -D warnings` (default-members; the eBPF crate is excluded
   by design — never `cargo build --workspace`).
4. **Build** — `cargo xtask build` (cross-compiles + embeds the eBPF object via `crates/agent/build.rs`).
5. **Unit tests** — `cargo test` (tier 1).
6. **eBPF load smoke-test** (tier 3) — `sudo ./target/debug/agent --once` on the runner's own kernel:
   proves the program loads, the verifier accepts it, it attaches, and it exits `0`.

**Job `deny`** — `cargo deny check` (advisories, licenses, bans, sources) via `cargo-deny-action`.

## `.github/workflows/ebpf-smoke.yml` — pinned-kernel microVM (scaffold)

The tier-3 load test across **pinned** kernels (`lvh`/qemu, `cilium/little-vm-helper`), which adds the
reproducible, known-kernel coverage the uncontrolled runner kernel in `ci.yml` can't give. Runs on
`workflow_dispatch` and PRs touching `crates/ebpf/**`.

> **Not yet a required check.** This workflow is a scaffold and needs iteration on a real runner to
> settle the lvh kernel tags and the in-VM invocation. The always-on baseline is the on-runner smoke in
> `ci.yml`; this exists so M0 isn't gated on a brittle setup.

## Planned / not yet wired

- **Kernel build-matrix** beyond the smoke kernels — folds into `ebpf-smoke.yml` as it matures.
- **Open-core guard** — assert `agent` never depends on `agent-cloud`
  ([ADR-0007](./adr/0007-open-core-one-way-dependency.md)); lands with M7 per the [ROADMAP](../ROADMAP.md).

*TODO: document required-checks / branch-protection once the pipeline settles.*
