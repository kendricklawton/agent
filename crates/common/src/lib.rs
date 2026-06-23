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

#![no_std]

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

/// Fixed header prefixing every event so userspace can demux by `kind` off a single ring buffer.
///
/// Layout is padding-free: `ktime_ns` (the in-kernel `bpf_ktime_get_ns()` stamp, so late
/// enrichment never distorts event time) leads, then the 32-bit fields, then the 16-bit fields and
/// explicit reserved bytes. `_pad`/`_reserved` exist so every byte is a real, zeroable field.
#[repr(C)]
#[derive(Clone, Copy)]
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
#[derive(Clone, Copy)]
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
