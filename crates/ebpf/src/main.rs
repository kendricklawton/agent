//! Kernel-side eBPF programs for the `agent` node agent.
//!
//! M0 scaffold: a single no-op tracepoint that proves the load → attach → detach pipeline.
//! M1 fills its body in (read pid/cgroup/mnt_ns, stamp ktime, submit an `ExecEvent` to a ring
//! buffer). Keep this crate `no_std`; every program must pass the verifier.
#![no_std]
#![no_main]

use aya_ebpf::{macros::tracepoint, programs::TracePointContext};

/// No-op program. Userspace attaches it to `sched/sched_process_exec`; returning 0 = "continue".
#[tracepoint]
pub fn noop(_ctx: TracePointContext) -> u32 {
    0
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // eBPF programs cannot unwind; the verifier also rejects reachable panics. This is here to
    // satisfy `no_std` and should never execute.
    loop {}
}
