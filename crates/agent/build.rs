//! Compile the `agent-ebpf` crate to a BPF object and stage it for `include_bytes_aligned!`.
//!
//! The workspace root pins **stable**, but the eBPF crate needs **nightly** (`-Z build-std`).
//! We invoke cargo with the working directory set to `crates/ebpf` so rustup honors that crate's
//! directory-scoped `rust-toolchain.toml` (nightly) — but only after clearing `RUSTUP_TOOLCHAIN`,
//! which the outer (stable) cargo injects and which would otherwise win over the toml. A dedicated
//! `--target-dir` keeps this nested build from contending on the main target lock.

use std::{env, fs, path::PathBuf, process::Command};

fn main() -> anyhow::Result<()> {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let ebpf_dir = manifest.join("../ebpf").canonicalize()?;
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let bpf_target_dir = out_dir.join("bpf");

    println!("cargo:rerun-if-changed={}", ebpf_dir.join("src").display());
    println!(
        "cargo:rerun-if-changed={}",
        ebpf_dir.join("Cargo.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        ebpf_dir.join("rust-toolchain.toml").display()
    );

    let status = Command::new("cargo")
        .current_dir(&ebpf_dir)
        // Let crates/ebpf/rust-toolchain.toml (nightly) win over the inherited stable pin.
        .env_remove("RUSTUP_TOOLCHAIN")
        .env_remove("RUSTC")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .args([
            "build",
            "--release",
            "--target",
            "bpfel-unknown-none",
            "-Z",
            "build-std=core",
            // The userspace `lto = "thin"` would break the BPF link; disable it for this build.
            "--config",
            "profile.release.lto=false",
            "--target-dir",
        ])
        .arg(&bpf_target_dir)
        .status()?;
    anyhow::ensure!(status.success(), "failed to build agent-ebpf (BPF object)");

    let obj = bpf_target_dir.join("bpfel-unknown-none/release/agent-ebpf");
    fs::copy(&obj, out_dir.join("agent-ebpf"))
        .map_err(|e| anyhow::anyhow!("copy {} -> OUT_DIR/agent-ebpf: {e}", obj.display()))?;
    Ok(())
}
