//! `agent-core` — the query engine and its two adapter seams.
//!
//! The engine turns a natural-language question into a **grounded** answer: a [`Model`] (an LLM) plans a
//! structured query, a [`DataProvider`] returns data in the canonical schema below, the engine computes
//! the requested [`Metric`], and the `Model` composes the answer. Every surface (the CLI now, an API
//! later) is a pure view of this — see ARCHITECTURE.md (headless engine, ports & adapters).

#![forbid(unsafe_code)]

use std::time::SystemTime;

use serde::Serialize;

// ───────────────────────── canonical schema ─────────────────────────

/// One OHLCV price bar. The engine's canonical shape: every provider adapter maps its raw API onto this,
/// so the engine never sees a provider's wire format (this is the anti-corruption layer that contains API
/// drift). Prices are `f64` for now — a decimal type is the correct long-term choice (see
/// LEARN-TECHNICAL.md, Tier 2). `#[non_exhaustive]` so the schema evolves additively.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Bar {
    /// When the bar closed.
    pub ts: SystemTime,
    /// Opening price.
    pub open: f64,
    /// Highest price in the interval.
    pub high: f64,
    /// Lowest price in the interval.
    pub low: f64,
    /// Closing price.
    pub close: f64,
    /// Volume traded.
    pub volume: u64,
}

impl Bar {
    /// Build a bar (needed because `#[non_exhaustive]` forbids struct-literal construction from adapter
    /// crates).
    #[must_use]
    pub fn new(ts: SystemTime, open: f64, high: f64, low: f64, close: f64, volume: u64) -> Self {
        Self {
            ts,
            open,
            high,
            low,
            close,
            volume,
        }
    }
}

/// What to fetch from a data provider: bars for `symbol` over the last `last_days`. The smallest useful
/// query shape for now; richer queries (date ranges, option chains, …) come as the schema grows.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DataQuery {
    /// Instrument ticker (e.g. "NVDA").
    pub symbol: String,
    /// Lookback window, in days.
    pub last_days: u32,
}

impl DataQuery {
    /// Build a query.
    #[must_use]
    pub fn new(symbol: impl Into<String>, last_days: u32) -> Self {
        Self {
            symbol: symbol.into(),
            last_days,
        }
    }
}

/// A metric the engine computes over a set of [`Bar`]s. Serialized snake_case in the wire output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Metric {
    /// Mean of the closes.
    AverageClose,
    /// The most recent close.
    LatestClose,
    /// The highest high.
    MaxHigh,
    /// The lowest low.
    MinLow,
}

impl Metric {
    /// A human label ("average close", …) for grounded answers.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Metric::AverageClose => "average close",
            Metric::LatestClose => "latest close",
            Metric::MaxHigh => "highest high",
            Metric::MinLow => "lowest low",
        }
    }
}

/// The `Model`'s structured plan for a question: which data to fetch, and what to compute from it. This is
/// what a real LLM produces via tool-calling; the mock model produces it deterministically.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Plan {
    /// The data to fetch.
    pub query: DataQuery,
    /// The metric to compute over it.
    pub metric: Metric,
}

impl Plan {
    /// Build a plan.
    #[must_use]
    pub fn new(query: DataQuery, metric: Metric) -> Self {
        Self { query, metric }
    }
}

/// What a provider can serve — the engine checks this before planning a fetch it can't fulfil. Grows a
/// field per capability; construct via [`Capabilities::new`] (`#[non_exhaustive]`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Capabilities {
    /// Can serve OHLCV bars.
    pub bars: bool,
}

impl Capabilities {
    /// Build a capability descriptor.
    #[must_use]
    pub fn new(bars: bool) -> Self {
        Self { bars }
    }
}

/// A grounded answer: the natural-language `text` plus the provenance that makes it trustworthy — which
/// model/provider, the metric + value, and how many bars it used. Serialized for `ask --json`.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct Answer {
    /// The original question.
    pub question: String,
    /// The model that answered.
    pub model: String,
    /// The data provider used.
    pub provider: String,
    /// The metric computed.
    pub metric: Metric,
    /// The computed value.
    pub value: f64,
    /// How many bars the answer is grounded in.
    pub bars_used: usize,
    /// The grounded natural-language answer.
    pub text: String,
}

// ───────────────────────── adapter seams (ports) ─────────────────────────

/// An LLM adapter: turns a question into a structured [`Plan`], then composes a grounded answer. A new
/// model (Claude, OpenAI, local) is a new impl and nothing else. `Send` so the engine can move threads.
pub trait Model: Send {
    /// A short label ("mock", "claude", …).
    fn name(&self) -> &str;

    /// Plan the structured query for a natural-language question.
    ///
    /// # Errors
    /// If the question can't be turned into a plan (e.g. no instrument found).
    fn plan(&mut self, question: &str) -> Result<Plan, ModelError>;

    /// Compose a grounded answer from the computed value and the data used.
    ///
    /// # Errors
    /// If the model fails to produce an answer.
    fn answer(
        &mut self,
        question: &str,
        metric: Metric,
        value: f64,
        bars: &[Bar],
    ) -> Result<String, ModelError>;
}

/// A data-source adapter: declares its capabilities and returns data in the canonical schema. A new source
/// (Polygon, Kalshi, custom) is a new impl and nothing else.
pub trait DataProvider: Send {
    /// A short label ("mock", "polygon", …).
    fn name(&self) -> &str;

    /// What this provider can serve — the engine checks this before fetching.
    fn capabilities(&self) -> Capabilities;

    /// Fetch bars for a query, mapped to the canonical [`Bar`] schema.
    ///
    /// # Errors
    /// If the source can't be reached or its response can't be mapped.
    fn bars(&mut self, query: &DataQuery) -> Result<Vec<Bar>, ProviderError>;
}

// ───────────────────────── errors ─────────────────────────

/// A recoverable failure in a [`Model`] adapter.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ModelError {
    /// The model could not produce a result.
    #[error("model error: {0}")]
    Failed(String),
}

/// A recoverable failure in a [`DataProvider`] adapter.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProviderError {
    /// The data source could not be queried.
    #[error("provider error: {0}")]
    Failed(String),
}

/// A failure to answer a question.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EngineError {
    /// The model failed to plan or answer.
    #[error(transparent)]
    Model(#[from] ModelError),
    /// The data provider failed to serve data.
    #[error(transparent)]
    Provider(#[from] ProviderError),
    /// The chosen provider can't serve the kind of data the plan needs.
    #[error("provider `{0}` cannot serve bars")]
    Unsupported(String),
    /// The query returned no data — we surface this rather than invent a number.
    #[error("no data returned for the query")]
    NoData,
}

// ───────────────────────── the engine ─────────────────────────

/// Ties a [`Model`] to a [`DataProvider`] and answers questions. Holds boxed adapters so the binary picks
/// them at runtime (`--mock` vs real), and the surfaces stay decoupled from concrete adapters.
pub struct Engine {
    model: Box<dyn Model>,
    provider: Box<dyn DataProvider>,
}

impl Engine {
    /// Build an engine from a model and a data provider.
    #[must_use]
    pub fn new(model: Box<dyn Model>, provider: Box<dyn DataProvider>) -> Self {
        Self { model, provider }
    }

    /// Answer a question: plan → capability check → fetch → compute → ground.
    ///
    /// # Errors
    /// If planning, fetching, or the computation fails, or the provider can't serve the query.
    pub fn ask(&mut self, question: &str) -> Result<Answer, EngineError> {
        let plan = self.model.plan(question)?;
        if !self.provider.capabilities().bars {
            return Err(EngineError::Unsupported(self.provider.name().to_owned()));
        }
        let bars = self.provider.bars(&plan.query)?;
        let value = compute(plan.metric, &bars)?;
        let text = self.model.answer(question, plan.metric, value, &bars)?;
        Ok(Answer {
            question: question.to_owned(),
            model: self.model.name().to_owned(),
            provider: self.provider.name().to_owned(),
            metric: plan.metric,
            value,
            bars_used: bars.len(),
            text,
        })
    }
}

/// Compute a [`Metric`] over bars. Pure and total — the engine's one piece of real arithmetic, so it's the
/// natural home for known-answer evals. Errors on empty input rather than inventing a number.
///
/// # Errors
/// [`EngineError::NoData`] if `bars` is empty.
pub fn compute(metric: Metric, bars: &[Bar]) -> Result<f64, EngineError> {
    let Some(last) = bars.last() else {
        return Err(EngineError::NoData);
    };
    let value = match metric {
        Metric::AverageClose => bars.iter().map(|b| b.close).sum::<f64>() / bars.len() as f64,
        Metric::LatestClose => last.close,
        Metric::MaxHigh => bars.iter().map(|b| b.high).fold(f64::MIN, f64::max),
        Metric::MinLow => bars.iter().map(|b| b.low).fold(f64::MAX, f64::min),
    };
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(close: f64) -> Bar {
        // high = close + 1, low = close - 1, so the metrics are predictable.
        Bar::new(
            SystemTime::UNIX_EPOCH,
            close,
            close + 1.0,
            close - 1.0,
            close,
            100,
        )
    }

    #[test]
    fn compute_metrics_over_bars() {
        let bars = [bar(100.0), bar(102.0), bar(104.0)];
        let approx = |m, want: f64| (compute(m, &bars).expect("value") - want).abs() < 1e-9;
        assert!(approx(Metric::AverageClose, 102.0));
        assert!(approx(Metric::LatestClose, 104.0));
        assert!(approx(Metric::MaxHigh, 105.0)); // high of last bar = 104 + 1
        assert!(approx(Metric::MinLow, 99.0)); // low of first bar = 100 - 1
    }

    #[test]
    fn compute_empty_is_no_data() {
        assert!(matches!(
            compute(Metric::AverageClose, &[]),
            Err(EngineError::NoData)
        ));
    }

    // Stub adapters drive the engine end-to-end with no network — the discipline for testing the seams.
    struct StubModel;
    impl Model for StubModel {
        fn name(&self) -> &str {
            "stub"
        }
        fn plan(&mut self, _q: &str) -> Result<Plan, ModelError> {
            Ok(Plan::new(DataQuery::new("FOO", 3), Metric::AverageClose))
        }
        fn answer(
            &mut self,
            _q: &str,
            metric: Metric,
            value: f64,
            bars: &[Bar],
        ) -> Result<String, ModelError> {
            Ok(format!(
                "{} = {value} ({} bars)",
                metric.label(),
                bars.len()
            ))
        }
    }

    struct StubProvider {
        bars: bool,
    }
    impl DataProvider for StubProvider {
        fn name(&self) -> &str {
            "stub"
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::new(self.bars)
        }
        fn bars(&mut self, _q: &DataQuery) -> Result<Vec<Bar>, ProviderError> {
            Ok(vec![bar(100.0), bar(101.0), bar(102.0)])
        }
    }

    #[test]
    fn engine_answers_end_to_end() {
        let mut e = Engine::new(Box::new(StubModel), Box::new(StubProvider { bars: true }));
        let a = e.ask("whatever").expect("answer");
        assert_eq!(a.metric, Metric::AverageClose);
        assert!((a.value - 101.0).abs() < 1e-9);
        assert_eq!(a.bars_used, 3);
        assert_eq!((a.model.as_str(), a.provider.as_str()), ("stub", "stub"));
        assert!(a.text.contains("101"));
    }

    #[test]
    fn engine_rejects_a_provider_that_cannot_serve_bars() {
        let mut e = Engine::new(Box::new(StubModel), Box::new(StubProvider { bars: false }));
        assert!(matches!(e.ask("x"), Err(EngineError::Unsupported(_))));
    }

    #[test]
    fn error_display_is_stable() {
        assert_eq!(ModelError::Failed("x".into()).to_string(), "model error: x");
        assert_eq!(
            ProviderError::Failed("y".into()).to_string(),
            "provider error: y"
        );
        assert_eq!(
            EngineError::NoData.to_string(),
            "no data returned for the query"
        );
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// The average close always lands within [min close, max close], for any non-empty series.
        #[test]
        fn average_close_within_min_and_max(
            closes in prop::collection::vec(-1e6f64..1e6f64, 1..64),
        ) {
            let bars: Vec<Bar> = closes
                .iter()
                .map(|&c| Bar::new(SystemTime::UNIX_EPOCH, c, c, c, c, 0))
                .collect();
            let avg = compute(Metric::AverageClose, &bars).expect("non-empty");
            let min = closes.iter().copied().fold(f64::INFINITY, f64::min);
            let max = closes.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            prop_assert!(avg >= min - 1e-6 && avg <= max + 1e-6);
        }
    }
}
