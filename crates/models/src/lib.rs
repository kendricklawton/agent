//! `agent-models` — LLM adapters behind [`agent_core::Model`].
//!
//! [`MockModel`] is a deterministic, keyless stand-in for a real LLM (Claude/OpenAI adapters land next):
//! it parses a question into a [`Plan`] with plain rules, so the whole engine runs and tests offline. The
//! parsing helpers are pure, so they're unit-tested without any model.

#![forbid(unsafe_code)]

use agent_core::{Bar, DataQuery, Metric, Model, ModelError, Plan};
use async_trait::async_trait;

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

    async fn plan(&mut self, question: &str) -> Result<Plan, ModelError> {
        let symbol = extract_symbol(question).ok_or_else(|| {
            ModelError::Failed("no ticker (an UPPERCASE symbol) found in the question".to_owned())
        })?;
        let metric = extract_metric(question);
        let last_days = extract_days(question).unwrap_or(DEFAULT_DAYS);
        Ok(Plan::new(DataQuery::new(symbol, last_days), metric))
    }

    async fn answer(
        &mut self,
        _question: &str,
        metric: Metric,
        value: f64,
        bars: &[Bar],
    ) -> Result<String, ModelError> {
        Ok(format!(
            "The {} was {value:.2} over the last {} bar(s) (source: mock).",
            metric.label(),
            bars.len(),
        ))
    }
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
        .filter_map(|t| t.parse::<u32>().ok())
        .next()
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

    #[tokio::test]
    async fn plans_a_full_question() {
        let mut m = MockModel;
        let plan = m
            .plan("average close of FOO over the last 3 days")
            .await
            .expect("plan");
        assert_eq!(plan.query, DataQuery::new("FOO", 3));
        assert_eq!(plan.metric, Metric::AverageClose);
    }

    #[tokio::test]
    async fn errors_without_a_ticker() {
        let mut m = MockModel;
        assert!(m.plan("what is the weather today").await.is_err());
    }
}
