//! `agent-ui` — the GUI frontend (`egui` on `wgpu`), a pure view of
//! [`agent_core`](agent_core) (see ARCHITECTURE.md).
//!
//! Scaffold: the real window (live charts, gauges, the process table, the multi-GPU grid) is wired
//! in Phase 1. This is the entry point the `app` binary dispatches `gui` to.

use agent_core::Collector;

/// Launch the GPU-accelerated window. Wired in Phase 1 (`egui`/`eframe`).
///
/// # Errors
/// Returns an error until the GUI lands in Phase 1.
pub fn run(_collector: Box<dyn Collector>) -> anyhow::Result<()> {
    anyhow::bail!("the GUI lands in Phase 1 — for now use `top` or `ps` (see ROADMAP.md)")
}
