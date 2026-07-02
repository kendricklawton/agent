//! `agent-ui` — the GUI frontend (`egui` on `wgpu`), a pure view of
//! [`agent_core`](agent_core) (see ARCHITECTURE.md).
//!
//! Phase 1: one panel — a device's utilization as a number + a live sparkline bound to the engine's
//! ring buffer. The window is a *reactive* surface: the engine wakes it (`request_repaint`) on each new
//! snapshot, so it redraws on new data and idles otherwise (no busy spin). Richer views land later.

#![forbid(unsafe_code)]

use agent_core::Collector;
use agent_core::Point;
use agent_core::engine::{self, DeviceSnapshot, EngineConfig, EngineHandle, SignalState};
use egui_plot::{Line, Plot, PlotPoints};

/// Launch the GPU-accelerated window. Spawns the engine (which owns the sampling loop) and renders the
/// snapshot it publishes; the GUI never touches a collector directly.
///
/// # Errors
/// If the engine thread can't be spawned or the windowing/`wgpu` backend fails to start.
pub fn run(collector: Box<dyn Collector>) -> anyhow::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "agent",
        options,
        Box::new(|cc| {
            // The engine wakes the UI on every publish; egui is otherwise idle (reactive).
            let ctx = cc.egui_ctx.clone();
            let engine = engine::spawn(
                collector,
                EngineConfig::default(),
                Box::new(move || ctx.request_repaint()),
            )?;
            Ok(Box::new(App { engine }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("failed to start the GUI: {e}"))
}

/// The eframe application — a thin view over the engine's latest [`Snapshot`](engine::Snapshot).
struct App {
    engine: EngineHandle,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let snapshot = self.engine.latest();
        egui::CentralPanel::default().show(ctx, |ui| match snapshot.devices.first() {
            // Render the absence of a signal explicitly — never a blank or a panic (§0.5).
            None => {
                ui.heading("agent");
                ui.label(state_label(&snapshot.state));
            }
            Some(device) => device_panel(ui, device, &snapshot.state),
        });
    }
}

/// Draw one device: name, the current utilization as a number + memory, and a live util sparkline.
fn device_panel(ui: &mut egui::Ui, device: &DeviceSnapshot, state: &SignalState) {
    ui.heading(&device.name);

    if let Some(p) = device.latest {
        ui.label(format!("Utilization: {} %", p.metrics.util.get()));
        ui.label(format!(
            "Memory: {} / {} MiB",
            p.metrics.mem_used.as_mib(),
            p.metrics.mem_total.as_mib()
        ));
    }
    if *state != SignalState::Ok {
        ui.label(state_label(state));
    }

    // A minimal, non-interactive sparkline with a fixed 0..=100 utilization range.
    let line = Line::new(PlotPoints::from(util_points(&device.history)));
    Plot::new("utilization")
        .height(120.0)
        .show_axes(false)
        .show_grid(false)
        .allow_zoom(false)
        .allow_drag(false)
        .allow_scroll(false)
        .include_y(0.0)
        .include_y(100.0)
        .show(ui, |plot_ui| plot_ui.line(line));
}

/// A short, human label for a signal state.
fn state_label(state: &SignalState) -> String {
    match state {
        SignalState::Ok => "live".to_owned(),
        SignalState::NoData => "waiting for the first sample…".to_owned(),
        SignalState::Stale { age } => format!("stale — no update for {}s", age.as_secs()),
        other => format!("no signal: {other:?}"),
    }
}

/// Map utilization history to plot points: x = sample index (oldest→newest), y = util percent. Pure, so
/// it's unit-testable without a window.
fn util_points(history: &[Point]) -> Vec<[f64; 2]> {
    history
        .iter()
        .enumerate()
        .map(|(i, p)| [i as f64, f64::from(p.metrics.util.get())])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{Bytes, Metrics, Pct};
    use std::time::SystemTime;

    fn point(util: u8) -> Point {
        let metrics = Metrics::new(Pct::clamped(util), Bytes(0), Bytes(0));
        Point::new(SystemTime::UNIX_EPOCH, metrics)
    }

    #[test]
    fn util_points_maps_index_and_percent() {
        let pts = util_points(&[point(10), point(20), point(30)]);
        assert_eq!(pts, vec![[0.0, 10.0], [1.0, 20.0], [2.0, 30.0]]);
    }

    #[test]
    fn util_points_of_empty_history_is_empty() {
        assert!(util_points(&[]).is_empty());
    }
}
