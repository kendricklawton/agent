//! `agent` — the single binary. Wires a [`Model`](agent_core::Model) + a
//! [`DataProvider`](agent_core::DataProvider) into the [`Engine`] and runs the `ask` subcommand.
//!
//! Only the mock adapters exist today, so every run uses them; real Claude/Polygon selection (via
//! `--mock` vs config/env) lands with those adapters.

use agent_core::Engine;
use agent_models::MockModel;
use agent_providers::MockProvider;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "agent",
    version,
    about = "Ask your data questions in plain English"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,

    /// Use the mock model + mock data source (no API keys). Currently the only mode.
    #[arg(long, global = true)]
    mock: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Ask a natural-language question about your data.
    Ask {
        /// The question, e.g. "average close of FOO over the last 3 days".
        question: String,
        /// Emit the structured answer as JSON (the scripting contract).
        #[arg(long)]
        json: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if !cli.mock {
        eprintln!(
            "agent: no real model/data adapters yet — using the mock adapters \
             (Claude + Polygon land next; pass --mock to silence this)"
        );
    }
    // Only the mock adapters exist so far; real Model/DataProvider selection arrives with them.
    let mut engine = Engine::new(Box::new(MockModel), Box::new(MockProvider));
    match cli.cmd {
        Cmd::Ask { question, json } => agent_cli::ask(&mut engine, &question, json),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known-answer eval: the mock provider's closes are 100, 101, 102 over 3 days → average 101.0, and
    /// the engine must compute and ground it correctly. This is the seed of the eval suite.
    #[test]
    fn known_answer_average_close() {
        let mut engine = Engine::new(Box::new(MockModel), Box::new(MockProvider));
        let answer = engine
            .ask("average close of FOO over the last 3 days")
            .expect("engine answers");
        assert_eq!(answer.metric, agent_core::Metric::AverageClose);
        assert!((answer.value - 101.0).abs() < 1e-9);
        assert_eq!(answer.bars_used, 3);
        assert!(answer.text.contains("101"));
    }
}
