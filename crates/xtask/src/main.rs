//! `cargo xtask <cmd>` — dev orchestration for the engine.
//!
//! `ci` runs the full local gate (fmt, clippy, build, test, feature powerset, deny) — the same checks,
//! in the same order, that `.github/workflows/ci.yml` runs, stopping at the first failure. No API keys
//! needed: tests drive the mock adapters.

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
    /// Run the full local gate (fmt, clippy, build, test, feature powerset, deny) — mirrors CI.
    Ci,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::Ci => ci(),
    }
}

fn ci() -> anyhow::Result<()> {
    // `--locked` everywhere so a stale Cargo.lock fails here, not in CI (which also builds --locked).
    // Assumes cargo-deny and cargo-hack are installed, exactly as the deny step always has.
    cargo(&["fmt", "--all", "--check"])?;
    cargo(&[
        "clippy",
        "--all-targets",
        "--locked",
        "--",
        "-D",
        "warnings",
    ])?;
    cargo(&["build", "--locked"])?;
    cargo(&["test", "--locked"])?;
    // No --locked here: --no-dev-deps rewrites the manifests, which would force a lock update that
    // --locked forbids. Lock freshness is already gated by the build/test/clippy steps above.
    cargo(&[
        "hack",
        "--feature-powerset",
        "--no-dev-deps",
        "check",
        "--workspace",
    ])?;
    cargo(&["deny", "check"])?;
    println!("\n✓ all checks passed");
    Ok(())
}

fn cargo(args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new("cargo").args(args).status()?;
    anyhow::ensure!(status.success(), "`cargo {}` failed", args.join(" "));
    Ok(())
}
