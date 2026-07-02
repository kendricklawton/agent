//! `agent-cli` — the terminal surface: the one-shot `ask` command. A pure view of the engine's
//! [`Answer`](agent_core::Answer) — it never calls a model or a data provider directly.

#![forbid(unsafe_code)]

use agent_core::Engine;

/// Ask a natural-language question and print the grounded answer. `--json` emits the structured
/// [`Answer`](agent_core::Answer) (a scripting contract with stable field names); the default prints the
/// human sentence. The error is returned before anything is printed, so partial output never leaks.
///
/// # Errors
/// If the engine cannot answer (planning, fetching, or the computation failed).
pub async fn ask(engine: &mut Engine, question: &str, json: bool) -> anyhow::Result<()> {
    let answer = engine
        .ask(question)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if json {
        println!("{}", serde_json::to_string(&answer)?);
    } else {
        println!("{}", answer.text);
    }
    Ok(())
}
