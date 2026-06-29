//! `agent` — the single binary. One headless engine, two first-class frontends, three subcommands
//! (the Docker/Ollama DX): `gui` · `top` · `ps` (see ARCHITECTURE.md). The remote `serve` collector
//! is added in Phase 10.
//!
//! The app picks a [`Collector`] and hands it to a frontend; the frontends never touch a data source
//! themselves (see ARCHITECTURE.md). Today everything runs on the mock source; NVML selection lands in Phase 1.

use agent_collector::MockCollector;
use agent_core::Collector;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "agent",
    version,
    about = "Native GPU & inference monitor (GUI + CLI)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,

    /// Use the synthetic mock source (no GPU needed). Honored once NVML selection lands in Phase 1.
    #[arg(long, global = true)]
    mock: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Launch the GPU-accelerated window (the default on a desktop).
    Gui,
    /// Live terminal dashboard — great over SSH on a headless box.
    Top,
    /// One-shot snapshot of devices.
    Ps {
        /// Emit JSON (the scripting contract).
        #[arg(long)]
        json: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // Phase 1 selects NVML when present and falls back to mock; today the mock is the only source.
    let _ = cli.mock;
    let mut collector: Box<dyn Collector> = Box::new(MockCollector::new(2));

    match cli.cmd.unwrap_or_else(default_cmd) {
        Cmd::Gui => agent_ui::run(collector),
        Cmd::Top => agent_cli::top(collector),
        Cmd::Ps { json } => agent_cli::ps(collector.as_mut(), json),
    }
}

/// The subcommand to run when none is given: the GUI on a desktop, but `top` on a headless box (no
/// display) — with a hint, so SSH/server users land somewhere useful instead of a window that can't open.
fn default_cmd() -> Cmd {
    if has_display() {
        Cmd::Gui
    } else {
        eprintln!(
            "agent: no display detected — falling back to `top` (run `agent gui` to force the window)"
        );
        Cmd::Top
    }
}

/// Whether a graphical display is available. On Linux/BSD that's an X11 or Wayland session; macOS and
/// Windows always have one.
#[cfg(all(unix, not(target_os = "macos")))]
fn has_display() -> bool {
    std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn has_display() -> bool {
    true
}
