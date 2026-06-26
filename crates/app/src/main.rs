//! `gpumon` — the single binary. One headless engine, two first-class frontends, three subcommands
//! (the Docker/Ollama DX): `gui` · `top` · `ps` (see ARCHITECTURE.md). The remote `serve` collector
//! is added in M9.
//!
//! The app picks a [`Collector`] and hands it to a frontend; the frontends never touch a data source
//! themselves (see ARCHITECTURE.md). Today everything runs on the mock source; NVML selection lands in M1.

use clap::{Parser, Subcommand};
use gpumon_collector::MockCollector;
use gpumon_core::Collector;

#[derive(Parser)]
#[command(
    name = "gpumon",
    version,
    about = "Native GPU & inference monitor (GUI + CLI)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,

    /// Use the synthetic mock source (no GPU needed). Honored once NVML selection lands in M1.
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
    // M1 selects NVML when present and falls back to mock; today the mock is the only source.
    let _ = cli.mock;
    let mut collector: Box<dyn Collector> = Box::new(MockCollector::new(2));

    match cli.cmd.unwrap_or(Cmd::Gui) {
        Cmd::Gui => gpumon_ui::run(collector),
        Cmd::Top => gpumon_cli::top(collector),
        Cmd::Ps { json } => gpumon_cli::ps(collector.as_mut(), json),
    }
}
