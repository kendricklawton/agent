//! `cargo xtask` — build/run orchestration for the agent.
//!
//! `build` compiles the agent (its build.rs cross-compiles + embeds the eBPF object under nightly).
//! `run` builds then launches the agent under `sudo`, since loading eBPF needs CAP_BPF/CAP_PERFMON.

use std::process::Command;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask", about = "build orchestration for the agent")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build the agent (compiles + embeds the eBPF object).
    Build {
        /// Build the release profile.
        #[arg(long)]
        release: bool,
    },
    /// Build, then run the agent under sudo (eBPF load needs elevated capabilities).
    Run {
        /// Build the release profile.
        #[arg(long)]
        release: bool,
        /// Arguments forwarded to the agent binary (e.g. `-- --once`).
        #[arg(last = true)]
        args: Vec<String>,
    },
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::Build { release } => build(release),
        Cmd::Run { release, args } => {
            build(release)?;
            let bin = format!("target/{}/agent", if release { "release" } else { "debug" });
            run_as_root(&bin, &args)
        }
    }
}

fn build(release: bool) -> anyhow::Result<()> {
    let mut args = vec!["build", "-p", "agent"];
    if release {
        args.push("--release");
    }
    cargo(&args)
}

fn cargo(args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new("cargo").args(args).status()?;
    anyhow::ensure!(status.success(), "`cargo {}` failed", args.join(" "));
    Ok(())
}

fn run_as_root(bin: &str, args: &[String]) -> anyhow::Result<()> {
    let status = Command::new("sudo").arg(bin).args(args).status()?;
    anyhow::ensure!(status.success(), "agent exited unsuccessfully ({status})");
    Ok(())
}
