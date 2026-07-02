//! `agent-cli` — the terminal surface: the one-shot `ask` command plus the layered [`config`] and stderr
//! [`logging`] the binary wires up. A pure view of the engine's [`Answer`](agent_core::Answer) — it never
//! calls a model or a data provider directly.

#![forbid(unsafe_code)]

pub mod config;
pub mod logging;

use std::io::Write;

use agent_core::{Engine, NullSink, TokenSink};

/// A [`TokenSink`] that streams answer tokens to stdout, flushing each so they render live. Writes go
/// through `write_all`/`flush` (not `print!`, which panics on a broken pipe) and errors are swallowed, so
/// `agent ask … | head` closing the pipe mid-stream ends cleanly instead of panicking. The lock is taken
/// per delta (never held across the engine's `.await`).
struct StdoutSink;

impl TokenSink for StdoutSink {
    fn push(&mut self, delta: &str) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(delta.as_bytes());
        let _ = out.flush();
    }
}

/// Ask a natural-language question and print the grounded answer. The default **streams** the human
/// sentence to stdout token-by-token; `--json` emits the structured [`Answer`](agent_core::Answer) (a
/// scripting contract with stable field names) as a single line.
///
/// `--json` is all-or-nothing — nothing is printed until the answer is complete. The streamed human path
/// trades that atomicity for liveness: if the model errors mid-stream, partial text may already be on
/// screen (only reachable once real, network-backed adapters exist).
///
/// # Errors
/// If the engine cannot answer (planning, fetching, or the computation failed).
pub async fn ask(engine: &mut Engine, question: &str, json: bool) -> anyhow::Result<()> {
    tracing::debug!(question, "cli: answering question");
    let answer = if json {
        // Discard deltas; print the whole Answer as JSON once it's done.
        engine.ask(question, &mut NullSink).await
    } else {
        // Stream the answer text to stdout as it's produced.
        engine.ask(question, &mut StdoutSink).await
    }
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    tracing::debug!(
        model = answer.model,
        provider = answer.provider,
        value = answer.value,
        bars_used = answer.bars_used,
        "cli: got grounded answer"
    );
    // Swallowed writes (not `println!`) so a closed pipe ends cleanly rather than panicking.
    let mut out = std::io::stdout().lock();
    if json {
        let _ = writeln!(out, "{}", serde_json::to_string(&answer)?);
    } else {
        // The text already streamed via the sink; just terminate the line.
        let _ = writeln!(out);
    }
    Ok(())
}
