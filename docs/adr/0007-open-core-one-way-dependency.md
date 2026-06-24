# 0007 — Open-core: one-way dependency (cloud → OSS)

- **Status:** Accepted
- **Date:** 2026-06-23
- **Deciders:** K-Henry
- **Milestone:** cross-cutting (foundational)

## Context
The product is open-core: a free, self-hostable OSS **`agent`** (this repo, Apache-2.0) plus a
private, paid **`agent-cloud`** fleet control plane. The model only works if the OSS agent is
**fully usable standalone** — that's the adoption driver and the moat (canonical engine + ecosystem),
following Falco→Sysdig / Tetragon→Isovalent / Parca→Polar Signals. The risk is coupling that quietly
makes the agent depend on the cloud, killing self-host adoption.

## Decision
We will enforce a **strictly one-way dependency: `agent-cloud` → `agent`, never the reverse.**
- This OSS repo **never imports `agent-cloud`**; a CI check fails the build if it does.
- The agent is **fully functional with zero cloud** — local sinks (Prometheus `/metrics`, JSON logs,
  local rules/alerts) deliver full value offline; the exporter is strictly optional.
- The **contract lives here**: `agent-common`'s `#[repr(C)]` event types + the gRPC/proto schema
  ([ADR-0005](0005-event-abi-two-encodings.md)). `agent-cloud` imports *that*.

## Consequences
- OSS adoption isn't gated on the cloud; the cloud is a convenience layer over the user's own infra,
  not a turnstile.
- The contract crate/proto is the single integration seam — it must stay stable and dependency-light.
- No shortcut of "just import the cloud code" — shared logic that both need lives in the OSS contract,
  not the cloud.
- Licensing is clean: OSS Apache-2.0 here; the cloud is proprietary and separate.
- **Fleet management (M8) preserves this, not breaks it.** The M8 control channel lets the cloud push
  to the agent (signed policy bundles + a fixed, audited verb set — snapshot/profile/drain/re-sync),
  but the *code* dependency stays strictly one-way (the agent never imports `agent-cloud`; it speaks
  the contract). It is **never a remote-code path** — only declarative, signature-verified policy — so
  a compromised plane has a bounded blast radius, and **offline-first holds**: an unreachable plane
  leaves the agent fully operational on its last-known-good bundle. Zero-cloud remains a first-class
  mode. The channel's own design is recorded in the deferred M8 ADR.

## Alternatives considered
- **Monorepo / tight coupling** between agent and cloud — rejected: the agent must stand alone, and
  bidirectional deps would leak proprietary concerns into the OSS.
- **Cloud-required agent** (phone-home to function) — rejected: kills self-host adoption, which is the
  entire open-core thesis.
