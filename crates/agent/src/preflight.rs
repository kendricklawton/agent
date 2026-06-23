//! Boot preflight — verify the node can actually run the agent *before* we touch the kernel.
//!
//! A clear, early failure beats a cryptic verifier/load error after the fact. The three platform
//! requirements the whole agent rests on are **hard** (abort): a recent-enough kernel, BTF, and
//! cgroup v2. The `bpf` LSM gates only the optional M6 enforcement backend, so its absence is a
//! **warning** that downgrades enforcement — never a block on capture.
//!
//! See [`docs/support-matrix.md`](../../../docs/support-matrix.md) for the full requirement table;
//! this module is its runtime enforcement point.

use std::{fs, path::Path};

use anyhow::{Context as _, bail};

/// Minimum kernel for the ring buffer (`BPF_MAP_TYPE_RINGBUF`, kernel ≥ 5.8). See ADR-0001.
const MIN_KERNEL: (u32, u32) = (5, 8);
/// CO-RE needs the kernel's own BTF (`CONFIG_DEBUG_INFO_BTF=y`). See ADR-0002.
const BTF_PATH: &str = "/sys/kernel/btf/vmlinux";
/// Presence of this file marks the unified cgroup v2 hierarchy (what M2 enrichment targets).
const CGROUP_V2_MARKER: &str = "/sys/fs/cgroup/cgroup.controllers";
/// The active LSM list (comma-separated). `bpf` must appear for BPF-LSM enforcement to attach.
const LSM_PATH: &str = "/sys/kernel/security/lsm";

/// Outcome of the preflight checks. Returned on success so callers (and future readiness probes)
/// can report what the node supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preflight {
    /// Detected kernel `(major, minor)`.
    pub kernel: (u32, u32),
    /// BTF is present (always true on success — a hard requirement).
    pub btf: bool,
    /// cgroup v2 unified hierarchy is mounted (always true on success — a hard requirement).
    pub cgroup_v2: bool,
    /// `bpf` is in the active LSM list — if false, M6 BPF-LSM enforcement can't attach.
    pub bpf_lsm: bool,
}

/// Run the boot preflight. Returns `Ok` only when every **hard** requirement is met; a missing
/// `bpf` LSM logs a warning but still succeeds.
pub fn check() -> anyhow::Result<Preflight> {
    let kernel = kernel_version().context("determine kernel version")?;
    if kernel < MIN_KERNEL {
        bail!(
            "kernel {}.{} is below the required {}.{} — the ring buffer (BPF_MAP_TYPE_RINGBUF) needs ≥ 5.8",
            kernel.0,
            kernel.1,
            MIN_KERNEL.0,
            MIN_KERNEL.1,
        );
    }

    let btf = Path::new(BTF_PATH).exists();
    if !btf {
        bail!(
            "kernel BTF not found at {BTF_PATH} — rebuild the kernel with CONFIG_DEBUG_INFO_BTF=y; \
             CO-RE relocation cannot work without it"
        );
    }

    let cgroup_v2 = Path::new(CGROUP_V2_MARKER).exists();
    if !cgroup_v2 {
        bail!(
            "cgroup v2 not detected ({CGROUP_V2_MARKER} missing) — the agent requires the unified \
             cgroup hierarchy"
        );
    }

    let bpf_lsm = lsm_has_bpf();
    if !bpf_lsm {
        tracing::warn!(
            "`bpf` is not in the active LSM list ({LSM_PATH}); BPF-LSM enforcement (M6) cannot \
             attach — kill/deny will degrade to bpf_send_signal + cgroup/connect. Add `bpf` to the \
             kernel `lsm=` boot parameter to enable it."
        );
    }

    tracing::info!(
        kernel_major = kernel.0,
        kernel_minor = kernel.1,
        btf,
        cgroup_v2,
        bpf_lsm,
        "boot preflight passed"
    );
    Ok(Preflight {
        kernel,
        btf,
        cgroup_v2,
        bpf_lsm,
    })
}

/// Read and parse the running kernel's `(major, minor)` from `/proc/sys/kernel/osrelease`.
fn kernel_version() -> anyhow::Result<(u32, u32)> {
    let raw = fs::read_to_string("/proc/sys/kernel/osrelease")
        .context("read /proc/sys/kernel/osrelease")?;
    parse_kernel_version(&raw)
        .with_context(|| format!("parse kernel version from {:?}", raw.trim()))
}

/// Parse a `uname -r`-style string into `(major, minor)`, e.g. `"7.0.11-arch1-1"` → `(7, 0)`.
/// Returns `None` if the first two dot-separated components aren't numbers.
fn parse_kernel_version(raw: &str) -> Option<(u32, u32)> {
    // Components are separated by '.' (and a trailing distro suffix by '-'); we only need the
    // first two numeric fields, so split on both and take them in order.
    let mut parts = raw.trim().split(['.', '-']);
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// True if the active LSM list contains `bpf`. Absent file (securityfs not mounted, or no LSMs
/// listed) is treated as "no bpf".
fn lsm_has_bpf() -> bool {
    fs::read_to_string(LSM_PATH)
        .map(|list| list.split(',').any(|m| m.trim() == "bpf"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_kernel_strings() {
        assert_eq!(parse_kernel_version("7.0.11-arch1-1"), Some((7, 0)));
        assert_eq!(parse_kernel_version("5.8.0"), Some((5, 8)));
        assert_eq!(parse_kernel_version("5.7.19-200.fc32.x86_64"), Some((5, 7)));
        assert_eq!(parse_kernel_version("5.10"), Some((5, 10)));
        assert_eq!(parse_kernel_version("6.1.0-rc4+\n"), Some((6, 1)));
    }

    #[test]
    fn rejects_malformed_kernel_strings() {
        assert_eq!(parse_kernel_version(""), None);
        assert_eq!(parse_kernel_version("not-a-version"), None);
        assert_eq!(parse_kernel_version("5"), None);
        assert_eq!(parse_kernel_version("5.x"), None);
    }

    #[test]
    fn min_kernel_ordering_is_correct() {
        // Tuple comparison is the gate used in `check()`; lock in the boundary behavior.
        assert!((5, 7) < MIN_KERNEL);
        assert!((5, 8) >= MIN_KERNEL);
        assert!((6, 0) >= MIN_KERNEL);
        assert!((4, 19) < MIN_KERNEL);
    }
}
