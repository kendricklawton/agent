# Security Policy

`agent` is a **privileged, kernel-level eBPF agent**: it loads eBPF programs, runs as a node-level
DaemonSet with elevated capabilities (`CAP_BPF`/`CAP_PERFMON`/`CAP_SYS_ADMIN`/`CAP_SYS_RESOURCE`), and
— in its optional enforcement mode — can kill processes and deny syscalls. Its blast radius is the
whole node. We take security reports seriously and ask that you report them **privately** so we can
fix them before they are public.

## Supported versions

The project is pre-release and pre-`1.0`; the ABI and feature set are still moving (see
[`ROADMAP.md`](./ROADMAP.md)). During this phase, **only the latest tagged release and the `main`
branch receive security fixes** — there are no backported patch streams yet. A formal support window
(supported minor versions + backport policy) lands with the `v0.10.0` platform release.

| Version | Supported |
|---------|-----------|
| `main` + latest tag | ✅ |
| older tags | ❌ — upgrade to the latest |

## Reporting a vulnerability

**Please do not open a public issue, pull request, or discussion for a suspected vulnerability.**

Report it privately via **GitHub's private vulnerability reporting** — the **"Report a
vulnerability"** button under the repository's **Security** tab (GitHub Security Advisories). That
opens a private channel with the maintainers.

Please include, as far as you can determine:

- the **component** (eBPF program, loader, enrichment, rules, enforcement, exporter, fleet) and the
  version / commit;
- the **kernel version, distro, and CPU architecture** — bugs here are often kernel-version-specific;
- the **impact** and a **reproduction** (a PoC, steps, or a failing test);
- whether it requires **enforcement mode**, the **cloud exporter**, or **multi-tenant fleet mode**.

**What to expect.** This is an early, small project — we aim to **acknowledge a report within a few
days** and to keep you updated as we triage and fix. We practice **coordinated disclosure**: we agree
an embargo window with you, fix and release, then publish an advisory **crediting you** (unless you
prefer to stay anonymous). Good-faith research conducted under this policy is welcome and we will not
pursue it.

## Scope — what is and isn't a vulnerability

This tool is privileged *by design*, so some "issues" are inherent to what it is. To set expectations:

**In scope** (please report):

- the agent acting **beyond its intended privilege** — escaping its capability set, or being tricked
  into loading/attaching programs or enforcing policy it should not;
- **cross-tenant leakage** or policy bypass once multi-tenancy lands (M8);
- the loader/decoder being **crashed or corrupted by malformed kernel events or ring-buffer data**;
- **enforcement mis-firing** — killing or denying the wrong workload, or the self-protection allowlist
  being bypassable;
- **supply-chain** issues in our build, release artifacts, or signing;
- **secrets, tokens, or sensitive event data leaking** to logs, metrics, or the wrong sink.

**Out of scope** (not vulnerabilities):

- the agent **requiring root / `CAP_BPF` and host access** — that is its documented, necessary posture;
- attacks that **already require root** on the node (the attacker has already won);
- denial of service from intentionally running the agent on an **unsupported kernel that fails
  preflight**;
- the cloud control plane (`agent-cloud`) — a separate, private repository with its own policy.

## Our security posture

Several of this project's invariants *are* security controls — see the **Architectural invariants** in
[`ROADMAP.md`](./ROADMAP.md) and the decision records in [`docs/adr/`](./docs/adr/):

- **The verifier is the gate** — a program that does not pass the kernel verifier never ships.
- **Enforcement is audit-first and fail-open**, with a self-protection allowlist so the agent can
  never kill itself (M6).
- **Least privilege** — named capabilities over blanket `privileged`; least-privilege RBAC
  (`get/list/watch` on pods/nodes only).
- **Supply chain** — reproducible builds, signed images + SBOM, pinned dependencies, `cargo deny` in
  CI (M9).
- **Offline-first, one-way dependency** — the agent never imports the cloud and stays fully functional
  with zero cloud; fleet control carries only **signed, declarative policy, never remote code** (M8).
