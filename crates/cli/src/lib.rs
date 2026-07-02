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

/// Render the snapshot as a human table (headline metrics; full detail is the JSON's job). Not part of
/// the contract — free to change. Memory is MiB; a metric the source doesn't expose shows as `-`.
fn print_table(snapshot: &Snapshot) {
    println!(
        "{:<4} {:<16} {:>5} {:>9} {:>9} {:>5} {:>7} {:>6} {:>4}",
        "GPU", "NAME", "UTIL", "USED", "TOTAL", "TEMP", "POWER", "SMCLK", "FAN"
    );
    for d in &snapshot.devices {
        let Some(p) = d.latest else { continue };
        let m = p.metrics;
        println!(
            "{:<4} {:<16} {:>4}% {:>9} {:>9} {:>5} {:>7} {:>6} {:>4}",
            d.index,
            d.name,
            m.util.get(),
            m.mem_used.as_mib(),
            m.mem_total.as_mib(),
            opt(m.temperature.map(|t| format!("{}C", t.get()))),
            opt(m.power.map(|w| format!("{:.0}W", w.as_watts()))),
            opt(m.sm_clock.map(|c| c.get())),
            opt(m.fan.map(|f| format!("{}%", f.get()))),
        );
    }
}

/// Format an optional value for the table: the value, or `-` when the source doesn't expose it.
fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "-".to_owned(), |v| v.to_string())
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
    use agent_core::{Bytes, Celsius, DeviceSample, KbPerSec, Megahertz, Metrics, Milliwatts, Pct};
    use std::time::SystemTime;

    /// Golden test — the `ps --json` wire contract. Renaming/reordering a field or dropping `schema_version`
    /// is a SemVer-major break, so this asserts the exact bytes. Deterministic: the wire carries no
    /// timestamp. A fully-populated device locks every field name and its order.
    #[test]
    fn ps_json_is_the_stable_contract() {
        let metrics = Metrics::new(
            Pct::clamped(26),
            Bytes(5_300_000_000),
            Bytes(24_000_000_000),
        )
        .with_temperature(Some(Celsius(61)))
        .with_power(Some(Milliwatts(150_000)))
        .with_clocks(Some(Megahertz(1_800)), Some(Megahertz(9_500)))
        .with_sm_occupancy(Some(Pct::clamped(55)))
        .with_pcie(Some(KbPerSec(2_000)), Some(KbPerSec(1_500)))
        .with_fan(Some(Pct::clamped(42)));
        let samples = vec![DeviceSample::new(
            0,
            "Mock GPU 0".to_owned(),
            metrics,
            SystemTime::UNIX_EPOCH,
        )];
        let snapshot = Snapshot::from_samples(&samples, SignalState::Ok);
        assert_eq!(
            render_json(&snapshot).expect("serialize"),
            r#"{"schema_version":1,"devices":[{"index":0,"name":"Mock GPU 0","util_pct":26,"mem_used_bytes":5300000000,"mem_total_bytes":24000000000,"temperature_c":61,"power_mw":150000,"sm_clock_mhz":1800,"mem_clock_mhz":9500,"sm_occupancy_pct":55,"pcie_tx_kb_s":2000,"pcie_rx_kb_s":1500,"fan_pct":42}]}"#
        );
    }

    /// An absent optional metric serializes as `null` (stable key set), never omitted.
    #[test]
    fn ps_json_serializes_absent_metrics_as_null() {
        let metrics = Metrics::new(Pct::clamped(10), Bytes(0), Bytes(0)); // every optional None
        let samples = vec![DeviceSample::new(
            0,
            "g".to_owned(),
            metrics,
            SystemTime::UNIX_EPOCH,
        )];
        let snapshot = Snapshot::from_samples(&samples, SignalState::Ok);
        assert_eq!(
            render_json(&snapshot).expect("serialize"),
            r#"{"schema_version":1,"devices":[{"index":0,"name":"g","util_pct":10,"mem_used_bytes":0,"mem_total_bytes":0,"temperature_c":null,"power_mw":null,"sm_clock_mhz":null,"mem_clock_mhz":null,"sm_occupancy_pct":null,"pcie_tx_kb_s":null,"pcie_rx_kb_s":null,"fan_pct":null}]}"#
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
