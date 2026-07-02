//! `agent-models` — LLM adapters behind [`agent_core::Model`].
//!
//! [`MockModel`] is a deterministic, keyless stand-in for a real LLM (Claude/OpenAI/Gemini adapters land
//! next): it drives the same **tool-use loop** a real model does — parse the question into a `query` tool
//! call, then, once the engine feeds the result back, signal [`Step::Done`] and let the **engine** compose
//! the grounded sentence — using plain rules, so the whole engine runs and tests offline. The parsing
//! helpers are pure, so they're unit-tested without any model.

#![forbid(unsafe_code)]

use agent_core::{Conversation, Metric, Model, ModelError, Step, ToolCall, ToolResult, ToolSpec};
use async_trait::async_trait;
use serde_json::json;

/// Default lookback when the question doesn't specify one.
const DEFAULT_DAYS: u32 = 7;

/// A deterministic mock LLM: extracts a ticker, a metric, and a window from the question with plain rules,
/// and formats a grounded answer. Stands in for a real tool-calling model.
pub struct MockModel;

#[async_trait]
impl Model for MockModel {
    fn name(&self) -> &str {
        "mock"
    }

    async fn respond(
        &mut self,
        conversation: &Conversation,
        _tools: &[ToolSpec],
    ) -> Result<Step, ModelError> {
        // If the engine has already run the query and fed the result back, we're done — the engine composes
        // the grounded sentence from that result (a real adapter would stop here too, or add narration).
        if last_tool_result(conversation).is_some() {
            return Ok(Step::Done);
        }
        // Otherwise this is the first turn: parse the question into a `query` tool call.
        let question = question_text(conversation).unwrap_or_default();
        let symbol = extract_symbol(question).ok_or_else(|| {
            ModelError::Failed("no ticker (an UPPERCASE symbol) found in the question".to_owned())
        })?;
        let metric = extract_metric(question);
        let last_days = extract_days(question).unwrap_or(DEFAULT_DAYS);
        Ok(Step::UseTools(vec![ToolCall::new(
            "mock-1",
            "query",
            json!({ "symbol": symbol, "last_days": last_days, "metric": metric }),
        )]))
    }
}

/// The most recent tool result the engine fed back, if any.
fn last_tool_result(conversation: &Conversation) -> Option<&ToolResult> {
    conversation.messages().iter().rev().find_map(|m| {
        m.content.iter().rev().find_map(|b| match b {
            agent_core::Block::ToolResult(tr) => Some(tr),
            _ => None,
        })
    })
}

/// The first text block — the user's question.
fn question_text(conversation: &Conversation) -> Option<&str> {
    conversation.messages().iter().find_map(|m| {
        m.content.iter().find_map(|b| match b {
            agent_core::Block::Text(t) => Some(t.as_str()),
            _ => None,
        })
    })
}

/// First UPPERCASE alphabetic token of length 1–5 — a rough ticker match.
fn extract_symbol(q: &str) -> Option<String> {
    q.split(|c: char| !c.is_ascii_alphanumeric())
        .find(|t| (1..=5).contains(&t.len()) && t.chars().all(|c| c.is_ascii_uppercase()))
        .map(str::to_owned)
}

/// Map keywords to a metric; defaults to average close.
fn extract_metric(q: &str) -> Metric {
    let s = q.to_ascii_lowercase();
    if s.contains("latest") || s.contains("last close") || s.contains("current") {
        Metric::LatestClose
    } else if s.contains("high") || s.contains("max") {
        Metric::MaxHigh
    } else if s.contains("low") || s.contains("min") {
        Metric::MinLow
    } else {
        Metric::AverageClose
    }
}

/// If the question mentions "N day(s)", the first such number.
fn extract_days(q: &str) -> Option<u32> {
    let s = q.to_ascii_lowercase();
    if !s.contains("day") {
        return None;
    }
    s.split(|c: char| !c.is_ascii_digit())
        .find_map(|t| t.parse::<u32>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_a_ticker() {
        assert_eq!(
            extract_symbol("avg close of NVDA last week").as_deref(),
            Some("NVDA")
        );
        assert_eq!(extract_symbol("no ticker in this sentence"), None);
    }

    #[test]
    fn maps_keywords_to_metrics() {
        assert_eq!(extract_metric("average close of X"), Metric::AverageClose);
        assert_eq!(extract_metric("the latest price of X"), Metric::LatestClose);
        assert_eq!(extract_metric("highest high of X"), Metric::MaxHigh);
        assert_eq!(extract_metric("the low of X"), Metric::MinLow);
    }

    #[test]
    fn extracts_a_day_window() {
        assert_eq!(extract_days("over the last 3 days"), Some(3));
        assert_eq!(extract_days("last week"), None); // no "day"
    }

    use agent_core::{Block, Message};

    /// A conversation seeded with just the user's question.
    fn asking(question: &str) -> Conversation {
        let mut c = Conversation::new();
        c.push(Message::user(vec![Block::Text(question.to_owned())]));
        c
    }

    #[tokio::test]
    async fn first_turn_emits_a_query_tool_call() {
        let mut m = MockModel;
        let step = m
            .respond(&asking("average close of FOO over the last 3 days"), &[])
            .await
            .expect("step");
        let Step::UseTools(calls) = step else {
            panic!("expected a tool call, got {step:?}");
        };
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "query");
        assert_eq!(calls[0].input["symbol"], "FOO");
        assert_eq!(calls[0].input["last_days"], 3);
        assert_eq!(calls[0].input["metric"], "average_close");
    }

    #[tokio::test]
    async fn errors_without_a_ticker() {
        let mut m = MockModel;
        assert!(
            m.respond(&asking("what is the weather today"), &[])
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn signals_done_once_a_tool_result_is_present() {
        let mut c = asking("average close of FOO over the last 3 days");
        // Simulate the engine having run the query and fed the result back.
        c.push(Message::user(vec![Block::ToolResult(ToolResult::new(
            "mock-1",
            json!({ "symbol": "FOO", "metric": "average_close", "value": 101.0, "bars_used": 3 }),
        ))]));
        let step = MockModel.respond(&c, &[]).await.expect("step");
        // The mock hands off to the engine, which authors the grounded sentence from the result.
        assert!(matches!(step, Step::Done));
    }
}
