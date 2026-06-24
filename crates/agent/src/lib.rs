//! `agent` userspace library — the testable core behind the thin `main.rs`.
//!
//! M1: load the embedded eBPF object, attach the `sched_process_exec` probe, and drain `ExecEvent`s
//! off the ring buffer. Enrichment, rules, and export land in later milestones.

use std::mem::size_of;

use agent_common::{EventHeader, EventKind, ExecEvent};
use anyhow::Context as _;
use aya::{Ebpf, maps::RingBuf, programs::TracePoint};
use tokio::io::unix::AsyncFd;

pub mod preflight;

/// The eBPF object, compiled to the BPF target by `build.rs` and embedded at build time.
static EBPF_OBJECT: &[u8] = aya::include_bytes_aligned!(concat!(env!("OUT_DIR"), "/agent-ebpf"));

/// Runtime configuration (the first slice of the agent's config model).
#[derive(Debug, Clone)]
pub struct Config {
    /// Load + attach, then return immediately instead of waiting for a shutdown signal.
    pub once: bool,
}

/// Initialize structured logging. Honors `RUST_LOG`, falling back to `filter`.
pub fn init_tracing(filter: &str) {
    use tracing_subscriber::{EnvFilter, fmt};
    let env = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(filter))
        .unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(env).init();
}

/// Load the eBPF object and attach the `sched_process_exec` probe. Returns the live `Ebpf` handle;
/// dropping it detaches every program.
pub fn load() -> anyhow::Result<Ebpf> {
    let mut ebpf = Ebpf::load(EBPF_OBJECT).context("load embedded eBPF object")?;

    let program: &mut TracePoint = ebpf
        .program_mut("sched_process_exec")
        .context("eBPF program `sched_process_exec` not found in object")?
        .try_into()
        .context("program `sched_process_exec` is not a tracepoint")?;
    program
        .load()
        .context("verify + load `sched_process_exec` (BPF verifier)")?;
    program
        .attach("sched", "sched_process_exec")
        .context("attach `sched_process_exec` to sched/sched_process_exec")?;

    Ok(ebpf)
}

/// Run the agent: preflight, load + attach the exec probe, then drain the ring buffer until
/// shutdown. With `cfg.once` it loads, attaches, and returns immediately (the load smoke test).
pub async fn run(cfg: Config) -> anyhow::Result<()> {
    preflight::check().context("boot preflight")?;
    // `ebpf` must outlive the drain loop: it owns the attached program (dropping it detaches).
    let mut ebpf = load()?;
    tracing::info!("eBPF program attached to sched/sched_process_exec; agent ready");

    if cfg.once {
        return Ok(());
    }

    // Take the ring-buffer map out of `ebpf` and register its fd with the async runtime.
    let events = RingBuf::try_from(
        ebpf.take_map("EVENTS")
            .context("ring-buffer map `EVENTS` not found in object")?,
    )
    .context("open the EVENTS ring buffer")?;
    let mut async_fd =
        AsyncFd::new(events).context("register ring buffer with the async runtime")?;

    tracing::info!(
        "draining process-exec events (run with `--log debug` to print each; Ctrl-C to stop)"
    );
    loop {
        tokio::select! {
            res = tokio::signal::ctrl_c() => {
                res.context("wait for Ctrl-C")?;
                tracing::info!("shutdown signal received; detaching programs");
                break;
            }
            guard = async_fd.readable_mut() => {
                let mut guard = guard.context("await ring-buffer readiness")?;
                let ring = guard.get_inner_mut();
                while let Some(item) = ring.next() {
                    decode_event(&item);
                }
                guard.clear_ready();
            }
        }
    }
    Ok(()) // `ebpf` drops here → programs detach cleanly.
}

/// Decode one ring-buffer record and log it. Malformed or unknown records are skipped (never panic):
/// a true event downstream is more valuable than a strict parser.
fn decode_event(bytes: &[u8]) {
    let Some(hdr) = bytes
        .get(..size_of::<EventHeader>())
        .and_then(|b| bytemuck::try_from_bytes::<EventHeader>(b).ok())
    else {
        return;
    };

    match EventKind::try_from(hdr.kind) {
        Ok(EventKind::Exec) => match bytemuck::try_from_bytes::<ExecEvent>(bytes) {
            // Per-event log is `debug!` — quiet by default; raise to `--log debug` to see the stream.
            Ok(ev) => tracing::debug!(
                pid = ev.pid,
                ppid = ev.ppid,
                uid = ev.uid,
                gid = ev.gid,
                cgroup_id = ev.cgroup_id,
                comm = %nul_str(&ev.comm),
                filename = %nul_str(&ev.filename),
                "exec",
            ),
            Err(_) => tracing::warn!(len = bytes.len(), "ExecEvent: wrong size/alignment"),
        },
        Ok(other) => tracing::debug!(?other, "unhandled event kind"),
        Err(unknown) => tracing::warn!(kind = unknown.0, "unknown event kind"),
    }
}

/// Render a NUL-padded fixed byte buffer (`comm`, `filename`) as a lossy string up to the first NUL.
fn nul_str(buf: &[u8]) -> std::borrow::Cow<'_, str> {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_is_constructible() {
        // Placeholder unit test: proves the lib is unit-testable without root/eBPF. Real load/attach
        // coverage is the VM integration test (needs CAP_BPF) deferred to the `ebpf-smoke.yml` microVM.
        let cfg = Config { once: true };
        assert!(cfg.once);
    }
}
