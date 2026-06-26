//! `cargo xtask <cmd>` — dev orchestration for the monitor.
//!
//! `ci` runs the full local gate (fmt, clippy, build, test, deny) — the same checks CI runs,
//! stopping at the first failure. No GPU needed: tests drive the mock collector.

use std::process::Command;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask", about = "dev orchestration")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the full local gate (fmt, clippy, build, test, deny) — mirrors CI.
    Ci,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::Ci => ci(),
    }
}

fn ci() -> anyhow::Result<()> {
    cargo(&["fmt", "--all", "--check"])?;
    cargo(&["clippy", "--all-targets", "--", "-D", "warnings"])?;
    cargo(&["build"])?;
    cargo(&["test"])?;
    cargo(&["deny", "check"])?;
    println!("\n✓ all checks passed");
    Ok(())
}

fn cargo(args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new("cargo").args(args).status()?;
    anyhow::ensure!(status.success(), "`cargo {}` failed", args.join(" "));
    Ok(())
}
