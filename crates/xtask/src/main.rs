//! `cargo xtask <cmd>` — dev orchestration for the engine.
//!
//! `ci` runs the full local gate (fmt, clippy, build, test, docs, feature powerset, deny) — the same
//! checks, in the same order, that `.github/workflows/ci.yml` runs, stopping at the first failure. No API
//! keys needed: tests drive the mock adapters.

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
    /// Run the full local gate (fmt, clippy, build, test, docs, feature powerset, deny) — mirrors CI.
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
    // Docs are a first-class surface: broken/redundant intra-doc links and undocumented public items fail
    // here (rustdoc `-D warnings`), not silently on the published docs.
    cargo_env(
        &["doc", "--no-deps", "--workspace", "--locked"],
        &[("RUSTDOCFLAGS", "-D warnings")],
    )?;
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
    cargo_env(args, &[])
}

fn cargo_env(args: &[&str], env: &[(&str, &str)]) -> anyhow::Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.args(args);
    for (key, value) in env {
        cmd.env(key, value);
    }
    let status = cmd.status()?;
    anyhow::ensure!(status.success(), "`cargo {}` failed", args.join(" "));
    Ok(())
}
