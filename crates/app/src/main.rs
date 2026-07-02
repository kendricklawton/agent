//! `agent` — the single binary. One headless engine, two first-class frontends, three subcommands
//! (the Docker/Ollama DX): `gui` · `top` · `ps` (see ARCHITECTURE.md). The remote `serve` collector
//! is added in Phase 10.
//!
//! The app picks a [`Collector`] and hands it to a frontend; the frontends never touch a data source
//! themselves (see ARCHITECTURE.md). The default source is NVML, falling back to the mock when no GPU is
//! present (`--mock` / `AGENT_SOURCE` override).

use agent_collector::{MockCollector, NvmlCollector};
use agent_core::Collector;
use clap::{Parser, Subcommand};

/// Synthetic GPUs the mock reports — for both the `--mock` source and the no-GPU fallback.
const MOCK_DEVICES: u32 = 2;

#[derive(Parser)]
#[command(
    name = "agent",
    version,
    about = "Native GPU & inference monitor (GUI + CLI)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,

    /// Use the synthetic mock source (no GPU needed); overrides the default NVML source.
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
    let source = resolve_source(cli.mock, std::env::var("AGENT_SOURCE").ok());
    let mut collector = build_collector(source)?;

    match cli.cmd.unwrap_or_else(default_cmd) {
        Cmd::Gui => agent_ui::run(collector),
        Cmd::Top => agent_cli::top(collector),
        Cmd::Ps { json } => agent_cli::ps(collector.as_mut(), json),
    }
}

/// Which data source to read. `Nvml { required }` distinguishes an explicit request (error if absent)
/// from the default (fall back to the mock if there's no GPU).
enum Source {
    Mock,
    Nvml { required: bool },
}

/// Resolve the source: flags > `AGENT_*` env > default. A minimal slice of §0.5's config precedence; the
/// full `Config` lands with the engine.
fn resolve_source(mock_flag: bool, env: Option<String>) -> Source {
    if mock_flag {
        return Source::Mock;
    }
    match env.as_deref() {
        Some("mock") => Source::Mock,
        Some("nvml") => Source::Nvml { required: true },
        _ => Source::Nvml { required: false },
    }
}

/// Build the selected collector. The default tries NVML and falls back to the (self-labelled) mock with a
/// notice on a GPU-less host; `AGENT_SOURCE=nvml` makes NVML mandatory.
fn build_collector(source: Source) -> anyhow::Result<Box<dyn Collector>> {
    match source {
        Source::Mock => Ok(Box::new(MockCollector::new(MOCK_DEVICES))),
        Source::Nvml { required } => match NvmlCollector::new() {
            Ok(c) => Ok(Box::new(c)),
            Err(e) if required => Err(anyhow::anyhow!("NVML required but unavailable: {e}")),
            Err(e) => {
                eprintln!(
                    "agent: no NVIDIA GPU detected ({e}) — using the synthetic mock source \
                     (run with --mock to silence this, or AGENT_SOURCE=nvml to require NVML)"
                );
                Ok(Box::new(MockCollector::new(MOCK_DEVICES)))
            }
        },
    }
}

/// The subcommand to run when none is given: probes for a display, warns on a headless box, and defers
/// the choice to [`choose_default`]. Kept thin (just the I/O) so the decision itself stays unit-testable.
fn default_cmd() -> Cmd {
    let display = has_display();
    if !display {
        eprintln!(
            "agent: no display detected — printing a one-shot `ps` (the live `top` TUI arrives in \
             Phase 3; run `agent gui` to force the window)"
        );
    }
    choose_default(display)
}

/// The pure default-subcommand decision (no I/O, so it's testable): the GUI when a display is present,
/// else a one-shot `ps` — it works today and exits cleanly, unlike the `top` placeholder. Switch the
/// headless arm back to `Cmd::Top` once Phase 3 lands the live TUI.
fn choose_default(display: bool) -> Cmd {
    if display {
        Cmd::Gui
    } else {
        Cmd::Ps { json: false }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_precedence_flag_beats_env_beats_default() {
        // The --mock flag wins over everything, including AGENT_SOURCE=nvml.
        assert!(matches!(
            resolve_source(true, Some("nvml".into())),
            Source::Mock
        ));
        // Env selects explicitly.
        assert!(matches!(
            resolve_source(false, Some("mock".into())),
            Source::Mock
        ));
        assert!(matches!(
            resolve_source(false, Some("nvml".into())),
            Source::Nvml { required: true }
        ));
        // Default (no flag, no/unknown env) is NVML, not required (so it can fall back to the mock).
        assert!(matches!(
            resolve_source(false, None),
            Source::Nvml { required: false }
        ));
        assert!(matches!(
            resolve_source(false, Some("other".into())),
            Source::Nvml { required: false }
        ));
    }

    #[test]
    fn default_is_gui_with_a_display_and_ps_when_headless() {
        assert!(matches!(choose_default(true), Cmd::Gui));
        // Headless must fall back to the working one-shot `ps`, never the unimplemented `top` (which
        // errors until Phase 3). Guards against a regression of that routing.
        assert!(matches!(choose_default(false), Cmd::Ps { json: false }));
    }
}
