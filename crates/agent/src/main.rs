//! `agent` entrypoint — thin: parse CLI, init logging, hand off to the library.

use clap::Parser;

/// eBPF node agent for Kubernetes (M0 scaffold).
#[derive(Debug, Parser)]
#[command(name = "agent", version, about)]
struct Cli {
    /// Load + attach the eBPF program, then exit (don't wait for Ctrl-C). Useful for smoke tests.
    #[arg(long)]
    once: bool,

    /// Log filter, e.g. `info`, `debug`, `agent=trace`. Overridden by `RUST_LOG` if set.
    #[arg(long, env = "AGENT_LOG", default_value = "info")]
    log: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    agent::init_tracing(&cli.log);
    agent::run(agent::Config { once: cli.once }).await
}
