//! `gpumon-cli` — the terminal frontend: one-shot `ps` (+ `--json`) and the live `top` TUI.
//!
//! A pure view of [`gpumon_core`](gpumon_core) (see ARCHITECTURE.md). `ps` works today against any
//! collector; the live `top` dashboard (`ratatui`) is wired in M3.

use gpumon_core::Collector;

/// One-shot snapshot of current device state. `--json` emits the scripting contract.
///
/// # Errors
/// Returns an error if the collector cannot be sampled.
pub fn ps(collector: &mut dyn Collector, json: bool) -> anyhow::Result<()> {
    let samples = collector
        .sample()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    if json {
        // Minimal hand-rolled JSON for the scaffold; a `serde` schema + stable field names land in M1.
        let mut out = String::from("[");
        for (i, s) in samples.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                r#"{{"index":{},"name":{:?},"util_pct":{},"mem_used":{},"mem_total":{}}}"#,
                s.index, s.name, s.util_pct, s.mem_used, s.mem_total
            ));
        }
        out.push(']');
        println!("{out}");
    } else {
        println!(
            "{:<4} {:<16} {:>5}  {:>13}  {:>13}",
            "GPU", "NAME", "UTIL", "USED", "TOTAL"
        );
        for s in &samples {
            println!(
                "{:<4} {:<16} {:>4}%  {:>13}  {:>13}",
                s.index, s.name, s.util_pct, s.mem_used, s.mem_total
            );
        }
    }
    Ok(())
}

/// The live terminal dashboard. Wired in M3 (`ratatui`).
///
/// # Errors
/// Returns an error until the TUI lands in M3.
pub fn top(_collector: Box<dyn Collector>) -> anyhow::Result<()> {
    anyhow::bail!("the live `top` TUI lands in M3 — `ps` works now (see ROADMAP.md)")
}
