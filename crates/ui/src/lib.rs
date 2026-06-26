//! `gpumon-ui` — the GUI frontend (`egui` on `wgpu`), a pure view of
//! [`gpumon_core`](gpumon_core) (see ARCHITECTURE.md).
//!
//! Scaffold: the real window (live charts, gauges, the process table, the multi-GPU grid) is wired
//! in M1. This is the entry point the `app` binary dispatches `gui` to.

use gpumon_core::Collector;

/// Launch the GPU-accelerated window. Wired in M1 (`egui`/`eframe`).
///
/// # Errors
/// Returns an error until the GUI lands in M1.
pub fn run(_collector: Box<dyn Collector>) -> anyhow::Result<()> {
    anyhow::bail!("the GUI lands in M1 — for now use `top` or `ps` (see ROADMAP.md)")
}
