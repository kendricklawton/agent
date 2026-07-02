//! `agent-cli` — the terminal frontend: one-shot `ps` (+ `--json`) and the live `top` TUI.
//!
//! A pure view of [`agent_core`](agent_core) (see ARCHITECTURE.md): `ps` samples once and renders from a
//! [`Snapshot`] — the same `core` type the GUI reads. The live `top` dashboard (`ratatui`) lands in Phase 3.

use agent_core::engine::Snapshot;
use agent_core::wire::WireSnapshot;
use agent_core::{Collector, SignalState};

/// One-shot snapshot of current device state.
///
/// `--json` emits the **scripting contract** (`schema_version` + stable field names — see
/// [`agent_core::wire`]); the human table is free to change. Exit codes: **0** on success (well-formed
/// JSON / a table on stdout, even with zero devices); **non-zero** on failure (the error is returned
/// before anything is printed, so partial JSON never leaks to stdout).
///
/// # Errors
/// Returns an error if the collector cannot be sampled.
pub fn ps(collector: &mut dyn Collector, json: bool) -> anyhow::Result<()> {
    let samples = collector
        .sample()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let snapshot = Snapshot::from_samples(&samples, SignalState::Ok);

    if json {
        println!("{}", render_json(&snapshot)?);
    } else {
        print_table(&snapshot);
    }
    Ok(())
}

/// Render the snapshot as the `ps --json` wire contract. Factored out so the exact bytes are golden-tested.
fn render_json(snapshot: &Snapshot) -> serde_json::Result<String> {
    serde_json::to_string(&WireSnapshot::from_snapshot(snapshot))
}

/// Render the snapshot as a human table. Not part of the contract — memory is shown in MiB for readability.
fn print_table(snapshot: &Snapshot) {
    println!(
        "{:<4} {:<20} {:>5}  {:>11}  {:>11}",
        "GPU", "NAME", "UTIL", "USED(MiB)", "TOTAL(MiB)"
    );
    for d in &snapshot.devices {
        if let Some(p) = d.latest {
            println!(
                "{:<4} {:<20} {:>4}%  {:>11}  {:>11}",
                d.index,
                d.name,
                p.util.get(),
                p.mem_used.as_mib(),
                p.mem_total.as_mib()
            );
        }
    }
}

/// The live terminal dashboard. Wired in Phase 3 (`ratatui`).
///
/// # Errors
/// Returns an error until the TUI lands in Phase 3.
pub fn top(_collector: Box<dyn Collector>) -> anyhow::Result<()> {
    anyhow::bail!("the live `top` TUI lands in Phase 3 — `ps` works now (see ROADMAP.md)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{Bytes, DeviceSample, Pct};
    use std::time::SystemTime;

    /// Golden test — the `ps --json` wire contract. Renaming/reordering a field or dropping `schema_version`
    /// is a SemVer-major break, so this asserts the exact bytes. Deterministic: the wire carries no timestamp.
    #[test]
    fn ps_json_is_the_stable_contract() {
        let samples = vec![
            DeviceSample::new(
                0,
                "Mock GPU 0".to_owned(),
                Pct::clamped(26),
                Bytes(5_300_000_000),
                Bytes(24_000_000_000),
                SystemTime::UNIX_EPOCH,
            ),
            DeviceSample::new(
                1,
                "Mock GPU 1".to_owned(),
                Pct::clamped(80),
                Bytes(9_000_000_000),
                Bytes(24_000_000_000),
                SystemTime::UNIX_EPOCH,
            ),
        ];
        let snapshot = Snapshot::from_samples(&samples, SignalState::Ok);
        let json = render_json(&snapshot).expect("serialize");
        assert_eq!(
            json,
            r#"{"schema_version":1,"devices":[{"index":0,"name":"Mock GPU 0","util_pct":26,"mem_used_bytes":5300000000,"mem_total_bytes":24000000000},{"index":1,"name":"Mock GPU 1","util_pct":80,"mem_used_bytes":9000000000,"mem_total_bytes":24000000000}]}"#
        );
    }

    #[test]
    fn ps_json_with_no_devices_is_well_formed() {
        let snapshot = Snapshot::from_samples(&[], SignalState::Ok);
        assert_eq!(
            render_json(&snapshot).expect("serialize"),
            r#"{"schema_version":1,"devices":[]}"#
        );
    }
}
