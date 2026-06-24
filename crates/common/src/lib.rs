//! Shared, `#[repr(C)]` event types — the spine of the project and the contract between the
//! eBPF (kernel) side, the userspace agent, and (over gRPC/proto) the `agent-cloud` control plane.
//!
//! Keep this crate `no_std` and dependency-light. Both kernel and userspace agree on these layouts.
//!
//! ## Invariants (see `.rules`)
//! - **Padding-free by construction.** Fields are ordered `u64`s-first so the compiler inserts no
//!   implicit padding. Any implicit padding byte is uninitialized memory the BPF verifier will
//!   reject when it reaches `bpf_ringbuf_submit` (*invalid indirect read from stack*). The
//!   `const` assertions below fail the build if a reorder reintroduces padding.
//! - **Zero the reserved ring-buffer slot before writing fields** (kernel side); never build on the
//!   512-byte BPF stack and copy.
//! - **Identity is captured in-kernel, at event time**: `cgroup_id`, `mnt_ns_inum`, and `ktime_ns`
//!   are read while the `task_struct` is live — the only moment they are guaranteed to exist.
//! - **Enrichment is userspace-only.** `PodMeta` and the `synced` flag are annotations layered onto
//!   the exported event by the agent; they are **not** part of this kernel ABI.

#![cfg_attr(not(test), no_std)]

use core::mem::size_of;

/// Event-type discriminant carried in [`EventHeader::kind`]. One ring buffer multiplexes all kinds.
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EventKind {
    Exec = 1,
    Connect = 2,
    FileOpen = 3,
    GpuStat = 4,
}

/// A `kind` discriminant read off the wire that doesn't match any known [`EventKind`]. Carries the
/// offending value so callers can count/log it rather than guess.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct UnknownEventKind(pub u32);

impl EventKind {
    /// The on-wire `u32` for this kind. Use this (not a cast) when writing [`EventHeader::kind`] in
    /// the kernel, so the wire encoding stays the single source of truth.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

impl TryFrom<u32> for EventKind {
    type Error = UnknownEventKind;

    /// Safely decode a wire `kind` into an [`EventKind`]. Never transmute a kernel-provided `u32`
    /// into the enum — an out-of-range value would be undefined behavior; this returns an error.
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Exec),
            2 => Ok(Self::Connect),
            3 => Ok(Self::FileOpen),
            4 => Ok(Self::GpuStat),
            other => Err(UnknownEventKind(other)),
        }
    }
}

/// Fixed header prefixing every event so userspace can demux by `kind` off a single ring buffer.
///
/// Layout is padding-free: `ktime_ns` (the in-kernel `bpf_ktime_get_ns()` stamp, so late
/// enrichment never distorts event time) leads, then the 32-bit fields, then the 16-bit fields and
/// explicit reserved bytes. `_pad`/`_reserved` exist so every byte is a real, zeroable field.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "pod", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct EventHeader {
    /// `bpf_ktime_get_ns()`, stamped in-kernel at event creation (monotonic).
    pub ktime_ns: u64,
    /// `EventKind` as `u32` (kept as a plain int on the wire for ABI stability).
    pub kind: u32,
    /// Total event length in bytes, including this header.
    pub len: u32,
    /// ABI version of this event kind; bump only additively.
    pub version: u16,
    /// Reserved; must be zero.
    pub _pad: u16,
    /// Reserved; must be zero.
    pub _reserved: u32,
}

/// A process-execution event (`sched_process_exec`). `cgroup_id` + `mnt_ns_inum` form the composite
/// identity key userspace joins to a pod — `mnt_ns_inum` recycles slower than the cgroup inode, so
/// it reconciles short-lived pods whose cgroup directory is unlinked before enrichment runs.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "pod", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct ExecEvent {
    pub hdr: EventHeader,
    pub pid: u32,
    pub ppid: u32,
    pub uid: u32,
    pub gid: u32,
    /// cgroup-v2 inode id (`bpf_get_current_cgroup_id`) — the primary join key for k8s enrichment.
    pub cgroup_id: u64,
    /// Mount-namespace inode (`task->nsproxy->mnt_ns->ns.inum`) — the slower-recycling secondary key.
    pub mnt_ns_inum: u64,
    pub comm: [u8; 16],
    pub filename: [u8; 256],
}

// Compile-time layout guards: sum-of-fields == size_of proves there is no implicit padding.
const _: () = assert!(size_of::<EventHeader>() == 24);
const _: () = assert!(
    size_of::<ExecEvent>() == size_of::<EventHeader>() + (4 + 4 + 4 + 4) + (8 + 8) + 16 + 256
);

/// Ring-buffer capacity for the kernel→userspace event channel (`BPF_MAP_TYPE_RINGBUF`). Declared
/// here so both the eBPF map and any userspace sizing share one constant.
///
/// The size **must** be a power of two **and** a page multiple, or `bpf_map_create` returns
/// `-EINVAL` with no diagnostic. The assert below is the M1 "loader guard" — enforced at **build
/// time**, since the size is baked into the eBPF object (there's nothing to check at load time).
pub const RING_BUF_BYTES: u32 = 256 * 1024;
const _: () = assert!(RING_BUF_BYTES.is_power_of_two() && RING_BUF_BYTES.is_multiple_of(4096));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_kind_roundtrips_through_u32() {
        for kind in [
            EventKind::Exec,
            EventKind::Connect,
            EventKind::FileOpen,
            EventKind::GpuStat,
        ] {
            assert_eq!(EventKind::try_from(kind.as_u32()), Ok(kind));
        }
    }

    #[test]
    fn unknown_event_kind_is_an_error_not_ub() {
        assert_eq!(EventKind::try_from(0), Err(UnknownEventKind(0)));
        assert_eq!(EventKind::try_from(5), Err(UnknownEventKind(5)));
        assert_eq!(
            EventKind::try_from(u32::MAX),
            Err(UnknownEventKind(u32::MAX))
        );
    }

    #[cfg(feature = "pod")]
    #[test]
    fn exec_event_roundtrips_through_bytes() {
        // Proves the userspace cast path: write fields → serialize → cast back, byte-for-byte.
        use bytemuck::Zeroable as _;
        let mut ev = ExecEvent::zeroed();
        ev.hdr.kind = EventKind::Exec.as_u32();
        ev.hdr.len = size_of::<ExecEvent>() as u32;
        ev.pid = 4242;
        ev.cgroup_id = 0xdead_beef;
        ev.comm[..2].copy_from_slice(b"ls");

        let bytes = bytemuck::bytes_of(&ev);
        assert_eq!(bytes.len(), size_of::<ExecEvent>());
        let back: &ExecEvent = bytemuck::from_bytes(bytes);

        assert_eq!(back.pid, 4242);
        assert_eq!(back.cgroup_id, 0xdead_beef);
        assert_eq!(&back.comm[..2], b"ls");
        assert_eq!(EventKind::try_from(back.hdr.kind), Ok(EventKind::Exec));
    }
}
