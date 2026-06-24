//! Kernel-side eBPF programs for the `agent` node agent.
//!
//! M1: a tracepoint on `sched/sched_process_exec` captures each process exec and emits an
//! [`ExecEvent`] over a ring buffer to userspace. Identity that needs CO-RE (`ppid`, `mnt_ns_inum`)
//! is left zero until Part B wires `vmlinux.rs`. Keep this crate `no_std`; every program must pass
//! the verifier.
#![no_std]
#![no_main]

use core::mem::size_of;

use agent_common::{EventKind, ExecEvent};
use aya_ebpf::{
    EbpfContext as _,
    helpers::{bpf_get_current_cgroup_id, bpf_ktime_get_ns, bpf_probe_read_kernel_str_bytes},
    macros::{map, tracepoint},
    maps::RingBuf,
    programs::TracePointContext,
};

// Generated CO-RE bindings to kernel structs (regenerate with `cargo xtask codegen`).
// Used by Part B to read `task_struct` fields (ppid, mnt-ns inode).
mod vmlinux;

/// Kernel→userspace event channel. The size + its power-of-two/page-multiple guard live in
/// `agent_common::RING_BUF_BYTES` (one constant shared with userspace).
#[map]
static EVENTS: RingBuf = RingBuf::with_byte_size(agent_common::RING_BUF_BYTES, 0);

/// Byte offset of the `__data_loc char[] filename` field in the `sched_process_exec` tracepoint
/// format, after the 8 bytes of common fields. The low 16 bits of that `u32` are the string's
/// byte offset from the event base.
const FILENAME_DATA_LOC_OFFSET: usize = 8;

/// Fires post-exec (so `comm`/exe are stable) on every successful `execve`.
#[tracepoint]
pub fn sched_process_exec(ctx: TracePointContext) -> u32 {
    // Reserve first; if the ring buffer is full, drop rather than block the kernel hot path.
    // (A drop counter lands with the metrics work; for now a full buffer simply skips the event.)
    let Some(mut entry) = EVENTS.reserve::<ExecEvent>(0) else {
        return 1;
    };

    // SAFETY: `slot` is a freshly reserved, uninitialized `ExecEvent` in the ring buffer. We zero
    // the whole slot, then write every field before `submit` — the verifier rejects any
    // uninitialized (including padding) byte that reaches `bpf_ringbuf_submit`.
    unsafe {
        let slot = entry.as_mut_ptr();
        core::ptr::write_bytes(slot, 0, 1);

        (*slot).hdr.ktime_ns = bpf_ktime_get_ns();
        (*slot).hdr.kind = EventKind::Exec.as_u32();
        (*slot).hdr.len = size_of::<ExecEvent>() as u32;
        (*slot).hdr.version = 1;

        // `pid` carries the userspace process id (tgid). ppid + mnt_ns_inum need CO-RE — Part B.
        (*slot).pid = ctx.tgid();
        (*slot).uid = ctx.uid();
        (*slot).gid = ctx.gid();
        (*slot).cgroup_id = bpf_get_current_cgroup_id();

        if let Ok(comm) = ctx.command() {
            (*slot).comm = comm;
        }

        // Full exec path from the tracepoint's `__data_loc` string, read into the bounded buffer.
        if let Ok(data_loc) = ctx.read_at::<u32>(FILENAME_DATA_LOC_OFFSET) {
            let off = (data_loc & 0xffff) as usize;
            let src = (ctx.as_ptr() as *const u8).add(off);
            let _ = bpf_probe_read_kernel_str_bytes(src, &mut (*slot).filename);
        }
    }

    entry.submit(0);
    0
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // eBPF programs cannot unwind; the verifier also rejects reachable panics. This is here to
    // satisfy `no_std` and should never execute.
    loop {}
}
