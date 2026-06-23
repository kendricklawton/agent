//! `agent` userspace library — the testable core behind the thin `main.rs`.
//!
//! M0: load the embedded eBPF object, attach the no-op tracepoint, run until Ctrl-C. The pipeline
//! (ring-buffer drain → enrich → rules → export) lands in later milestones.

use anyhow::Context as _;
use aya::{Ebpf, programs::TracePoint};

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

/// Load the eBPF object and attach the M0 no-op program. Returns the live `Ebpf` handle; dropping
/// it detaches every program.
pub fn load() -> anyhow::Result<Ebpf> {
    let mut ebpf = Ebpf::load(EBPF_OBJECT).context("load embedded eBPF object")?;

    let program: &mut TracePoint = ebpf
        .program_mut("noop")
        .context("eBPF program `noop` not found in object")?
        .try_into()
        .context("program `noop` is not a tracepoint")?;
    program
        .load()
        .context("verify + load `noop` (BPF verifier)")?;
    program
        .attach("sched", "sched_process_exec")
        .context("attach `noop` to sched/sched_process_exec")?;

    Ok(ebpf)
}

/// Run the agent: load, attach, and wait for shutdown (unless `cfg.once`).
pub async fn run(cfg: Config) -> anyhow::Result<()> {
    let _ebpf = load()?;
    tracing::info!("eBPF program attached to sched/sched_process_exec (M0 no-op); agent ready");

    if cfg.once {
        return Ok(());
    }

    tokio::signal::ctrl_c().await.context("wait for Ctrl-C")?;
    tracing::info!("shutdown signal received; detaching programs");
    Ok(()) // `_ebpf` drops here → programs detach cleanly.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_is_constructible() {
        // Placeholder unit test: proves the lib is unit-testable without root/eBPF. Real load/attach
        // coverage is an `#[ignore]`d integration test that needs CAP_BPF (added in M1).
        let cfg = Config { once: true };
        assert!(cfg.once);
    }
}
