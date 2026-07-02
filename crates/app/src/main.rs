//! `agent` — the single binary. Resolves layered [config](agent_cli::config), initializes stderr
//! [logging](agent_cli::logging), then wires the configured [`Model`](agent_core::Model) +
//! [`DataProvider`](agent_core::DataProvider) into the [`Engine`] and runs the `ask` subcommand.
//!
//! Which adapters run is **config, not code**: `build_model`/`build_provider` map a name to an adapter, so a
//! new one (Phase 3) registers here. Only `mock` exists today; it's the keyless default.

use std::path::PathBuf;

use agent_core::{DataProvider, Engine, Model};
use agent_models::MockModel;
use agent_providers::MockProvider;
use anyhow::bail;
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

    /// Model adapter to use (default: `mock`). Also `AGENT_MODEL` / the config file.
    #[arg(long, global = true)]
    model: Option<String>,

    /// Data-provider adapter to use (default: `mock`). Also `AGENT_PROVIDER` / the config file.
    #[arg(long, global = true)]
    provider: Option<String>,

    /// Log filter, e.g. `debug` or `agent=debug` (default: `warn`). Also `AGENT_LOG`.
    #[arg(long, global = true)]
    log: Option<String>,

    /// Path to a TOML config file. Also `AGENT_CONFIG`.
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Shorthand for `--model mock --provider mock` (the keyless default pair).
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

/// Resolve a model adapter by name. A new model registers here (Phase 3).
fn build_model(name: &str) -> anyhow::Result<Box<dyn Model>> {
    match name {
        "mock" => Ok(Box::new(MockModel)),
        other => bail!("unknown model '{other}' (available: mock)"),
    }
}

/// Resolve a data-provider adapter by name. A new provider registers here (Phase 4).
fn build_provider(name: &str) -> anyhow::Result<Box<dyn DataProvider>> {
    match name {
        "mock" => Ok(Box::new(MockProvider)),
        other => bail!("unknown data provider '{other}' (available: mock)"),
    }
}

// A single-threaded runtime is plenty: the CLI issues one sequential ask. The multi-thread scheduler
// (and net/time features) arrive with the real HTTP adapters.
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // CLI flags are the top config layer; `--mock` is sugar that fills the mock pair unless a name was given.
    let mut flags = agent_cli::config::Partial {
        model: cli.model,
        provider: cli.provider,
        log: cli.log,
    };
    if cli.mock {
        flags.model.get_or_insert_with(|| "mock".to_owned());
        flags.provider.get_or_insert_with(|| "mock".to_owned());
    }

    let config = agent_cli::config::load(flags, cli.config.as_deref())?;
    agent_cli::logging::init(&config);

    let mut engine = Engine::new(
        build_model(&config.model)?,
        build_provider(&config.provider)?,
    );
    match cli.cmd {
        Cmd::Ask { question, json } => agent_cli::ask(&mut engine, &question, json).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_resolves_mock_and_rejects_unknown() {
        assert!(build_model("mock").is_ok());
        assert!(build_provider("mock").is_ok());
        assert!(build_model("nope").is_err());
        assert!(build_provider("nope").is_err());
    }

    /// Known-answer eval: the mock provider's closes are 100, 101, 102 over 3 days → average 101.0, and
    /// the engine must compute and ground it correctly. This is the seed of the eval suite.
    #[tokio::test]
    async fn known_answer_average_close() {
        let mut engine = Engine::new(
            build_model("mock").unwrap(),
            build_provider("mock").unwrap(),
        );
        let answer = engine
            .ask("average close of FOO over the last 3 days")
            .await
            .expect("engine answers");
        assert_eq!(answer.metric, agent_core::Metric::AverageClose);
        assert!((answer.value - 101.0).abs() < 1e-9);
        assert_eq!(answer.bars_used, 3);
        assert!(answer.text.contains("101"));
    }
}
