//! `agent-core` — the query engine and its two adapter seams.
//!
//! The engine turns a natural-language question into a **grounded** answer by driving a **tool-use loop**: a
//! [`Model`] (an LLM) either calls the engine's `query` tool or returns a final answer; the engine — not the
//! model — runs the tool (a [`DataProvider`] fetch mapped to the canonical schema below, then the pure
//! [`compute`]), and feeds the trustworthy result back for the model to ground on. Every surface (the CLI
//! and Python SDK now, an API later) is a pure view of this — see ARCHITECTURE.md (headless engine, ports &
//! adapters).

#![forbid(unsafe_code)]

use std::time::SystemTime;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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

/// A metric the engine computes over a set of [`Bar`]s. Serialized snake_case in the wire output and in the
/// `query` tool contract (so it round-trips through a model's tool call).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

// ───────────────────────── conversation & tool-use loop ─────────────────────────

/// A role in the conversation. Tool *results* are carried in a [`Role::User`] message (the Anthropic wire
/// convention), tool *calls* in a [`Role::Assistant`] message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Role {
    /// The human (and the tool results the engine feeds back).
    User,
    /// The model.
    Assistant,
}

/// One piece of a [`Message`]. The model emits `Text` and `ToolCall`; the **engine** emits `ToolResult`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Block {
    /// Natural-language text.
    Text(String),
    /// The model asks the engine to run a tool.
    ToolCall(ToolCall),
    /// The engine's trustworthy result for a prior tool call.
    ToolResult(ToolResult),
}

/// One turn in a [`Conversation`].
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Message {
    /// Who produced this message.
    pub role: Role,
    /// Its content blocks.
    pub content: Vec<Block>,
}

impl Message {
    /// A user message (the question, or tool results fed back to the model).
    #[must_use]
    pub fn user(content: Vec<Block>) -> Self {
        Self {
            role: Role::User,
            content,
        }
    }

    /// An assistant message (the model's tool calls or final text).
    #[must_use]
    pub fn assistant(content: Vec<Block>) -> Self {
        Self {
            role: Role::Assistant,
            content,
        }
    }
}

/// The message history the model reasons over. A single-turn `ask` builds one internally; multi-turn chat
/// (Phase 5) keeps one alive across turns and serializes it for session resume.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Conversation {
    messages: Vec<Message>,
}

impl Conversation {
    /// An empty conversation.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The messages so far, oldest first.
    #[must_use]
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Append a message.
    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
    }
}

/// A tool the engine offers the model, described for tool-calling. A real LLM adapter serializes this to the
/// provider's tool schema; the mock keys off `name`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ToolSpec {
    /// The tool's name (e.g. `"query"`).
    pub name: String,
    /// What it does — shown to the model.
    pub description: String,
    /// JSON Schema for the tool's input.
    pub input_schema: Value,
}

/// The model's request to run a tool — the parsed form of a provider `tool_use` block. `input` is JSON
/// (what every real provider sends), so the seam doesn't change when tools grow.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ToolCall {
    /// Correlates this call with its [`ToolResult`].
    pub id: String,
    /// Which tool to run.
    pub name: String,
    /// The tool's arguments, as JSON.
    pub input: Value,
}

impl ToolCall {
    /// Build a tool call (an adapter constructs these from the model's output; `#[non_exhaustive]`).
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, input: Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            input,
        }
    }
}

/// The engine's trustworthy result for a [`ToolCall`], fed back to the model to ground its answer.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ToolResult {
    /// The [`ToolCall::id`] this answers.
    pub id: String,
    /// The result, as JSON.
    pub output: Value,
}

impl ToolResult {
    /// Build a tool result (`#[non_exhaustive]`).
    #[must_use]
    pub fn new(id: impl Into<String>, output: Value) -> Self {
        Self {
            id: id.into(),
            output,
        }
    }
}

/// One turn of the model's reasoning: either run tools, or the final grounded answer. Replaces the old
/// split `plan`/`answer` — "planning" is now just the model choosing to call the `query` tool.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Step {
    /// Run these tools, feed the results back, and ask the model again.
    UseTools(Vec<ToolCall>),
    /// The model's final, grounded answer text.
    Done(String),
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

/// An LLM adapter driving a **tool-use loop**: given the conversation and the tools it may call, the model
/// either asks the engine to run tools or returns its final grounded answer. A new model (Claude, OpenAI,
/// Gemini, local) is a new impl and nothing else. `Send` so the engine can move threads.
///
/// `respond` is `async` because a real adapter is a network round-trip; [`async_trait`] keeps the trait
/// object-safe so the engine can hold a `Box<dyn Model>` chosen at runtime.
#[async_trait]
pub trait Model: Send {
    /// A short label ("mock", "claude", …).
    fn name(&self) -> &str;

    /// Advance the conversation one turn: inspect the messages so far and the available `tools`, and return
    /// either [`Step::UseTools`] (the engine runs them and feeds the results back) or [`Step::Done`] with the
    /// final answer. "Planning" a query is just choosing to call the `query` tool.
    ///
    /// # Errors
    /// If the model can't produce a step (e.g. the question names no instrument to query).
    async fn respond(
        &mut self,
        conversation: &Conversation,
        tools: &[ToolSpec],
    ) -> Result<Step, ModelError>;
}

/// A data-source adapter: declares its capabilities and returns data in the canonical schema. A new source
/// (Polygon, Kalshi, custom) is a new impl and nothing else.
///
/// [`bars`](DataProvider::bars) is `async` (a network fetch); the sync methods just describe the adapter.
#[async_trait]
pub trait DataProvider: Send {
    /// A short label ("mock", "polygon", …).
    fn name(&self) -> &str;

    /// What this provider can serve — the engine checks this before fetching.
    fn capabilities(&self) -> Capabilities;

    /// Fetch bars for a query, mapped to the canonical [`Bar`] schema.
    ///
    /// # Errors
    /// If the source can't be reached or its response can't be mapped.
    async fn bars(&mut self, query: &DataQuery) -> Result<Vec<Bar>, ProviderError>;
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
    /// The model failed to produce a step.
    #[error(transparent)]
    Model(#[from] ModelError),
    /// The data provider failed to serve data.
    #[error(transparent)]
    Provider(#[from] ProviderError),
    /// The chosen provider can't serve the kind of data the query needs.
    #[error("provider `{0}` cannot serve bars")]
    Unsupported(String),
    /// The query returned no data — we surface this rather than invent a number.
    #[error("no data returned for the query")]
    NoData,
    /// The model asked for a tool that doesn't exist, or with input the engine can't decode.
    #[error("tool error: {0}")]
    Tool(String),
    /// The model produced a final answer without grounding it in a tool result — refused, not trusted.
    #[error("the model answered without querying any data")]
    Ungrounded,
    /// The model exceeded the tool-use step budget without producing an answer.
    #[error("the model exceeded the {0}-step tool-use budget")]
    StepLimit(usize),
}

// ───────────────────────── the engine ─────────────────────────

/// The most tool-use turns [`Engine::ask`] will run before giving up — a backstop against a model that
/// never produces a final answer.
const MAX_STEPS: usize = 8;

/// The provenance the engine records while executing a `query` tool, used to build a grounded [`Answer`].
#[derive(Clone, Copy)]
struct Grounded {
    metric: Metric,
    value: f64,
    bars_used: usize,
}

/// The decoded input of a `query` tool call. Kept private — the wire form is JSON (a [`ToolCall`]); this is
/// just how the engine reads it.
#[derive(Deserialize)]
struct QueryInput {
    symbol: String,
    last_days: u32,
    metric: Metric,
}

/// The tools the engine offers the model. One for now: `query` fetches a metric over recent bars. Granular
/// fetch/compute tools arrive with the richer query model (Phase 9).
fn tool_specs() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "query".to_owned(),
        description:
            "Fetch a computed metric over the last N daily bars for a ticker. Returns the value the answer \
             must be grounded in."
                .to_owned(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string", "description": "Ticker, e.g. NVDA" },
                "last_days": { "type": "integer", "minimum": 1 },
                "metric": { "enum": ["average_close", "latest_close", "max_high", "min_low"] },
            },
            "required": ["symbol", "last_days", "metric"],
        }),
    }]
}

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

    /// Answer a question by driving the model's **tool-use loop**: seed a conversation, then repeatedly ask
    /// the model to [`respond`](Model::respond); run any tools it requests (the engine — not the model —
    /// fetches and computes, so the number is trustworthy) and feed the results back, until the model returns
    /// its final [`Step::Done`]. The answer is grounded in the last `query` the engine executed.
    ///
    /// # Errors
    /// If the model fails, a tool call is unknown/undecodable, the provider can't serve the query, the model
    /// answers without grounding ([`EngineError::Ungrounded`]), or it exceeds the step budget
    /// ([`EngineError::StepLimit`]).
    pub async fn ask(&mut self, question: &str) -> Result<Answer, EngineError> {
        let tools = tool_specs();
        let mut conversation = Conversation::new();
        conversation.push(Message::user(vec![Block::Text(question.to_owned())]));
        // Provenance of the last `query` the engine ran — an answer with none is ungrounded.
        let mut grounded: Option<Grounded> = None;

        for _ in 0..MAX_STEPS {
            match self.model.respond(&conversation, &tools).await? {
                Step::UseTools(calls) => {
                    if calls.is_empty() {
                        return Err(EngineError::Tool("the model requested no tools".to_owned()));
                    }
                    conversation.push(Message::assistant(
                        calls.iter().cloned().map(Block::ToolCall).collect(),
                    ));
                    let mut results = Vec::with_capacity(calls.len());
                    for call in &calls {
                        let (result, g) = self.run_tool(call).await?;
                        grounded = Some(g);
                        results.push(Block::ToolResult(result));
                    }
                    conversation.push(Message::user(results));
                }
                Step::Done(text) => {
                    let g = grounded.ok_or(EngineError::Ungrounded)?;
                    return Ok(Answer {
                        question: question.to_owned(),
                        model: self.model.name().to_owned(),
                        provider: self.provider.name().to_owned(),
                        metric: g.metric,
                        value: g.value,
                        bars_used: g.bars_used,
                        text,
                    });
                }
            }
        }
        Err(EngineError::StepLimit(MAX_STEPS))
    }

    /// Execute one tool call and return its result plus the provenance to ground the answer. The engine owns
    /// this — the model never fetches or computes, so a wrong number is impossible-by-construction.
    async fn run_tool(&mut self, call: &ToolCall) -> Result<(ToolResult, Grounded), EngineError> {
        match call.name.as_str() {
            "query" => {
                let input: QueryInput = serde_json::from_value(call.input.clone())
                    .map_err(|e| EngineError::Tool(format!("bad `query` input: {e}")))?;
                if !self.provider.capabilities().bars {
                    return Err(EngineError::Unsupported(self.provider.name().to_owned()));
                }
                let bars = self
                    .provider
                    .bars(&DataQuery::new(input.symbol, input.last_days))
                    .await?;
                let value = compute(input.metric, &bars)?;
                let grounded = Grounded {
                    metric: input.metric,
                    value,
                    bars_used: bars.len(),
                };
                let output = json!({
                    "metric": input.metric,
                    "value": value,
                    "bars_used": bars.len(),
                });
                Ok((
                    ToolResult {
                        id: call.id.clone(),
                        output,
                    },
                    grounded,
                ))
            }
            other => Err(EngineError::Tool(format!("unknown tool `{other}`"))),
        }
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

    /// Scan a conversation newest-first for the most recent tool-result `value`.
    fn last_result_value(convo: &Conversation) -> Option<f64> {
        convo.messages().iter().rev().find_map(|m| {
            m.content.iter().rev().find_map(|b| match b {
                Block::ToolResult(tr) => tr.output.get("value").and_then(Value::as_f64),
                _ => None,
            })
        })
    }

    /// A well-behaved stub: request the `query` tool, then ground the answer on the value the engine
    /// computed and fed back — the same two-phase shape a real model follows.
    struct StubModel;
    #[async_trait]
    impl Model for StubModel {
        fn name(&self) -> &str {
            "stub"
        }
        async fn respond(
            &mut self,
            convo: &Conversation,
            _tools: &[ToolSpec],
        ) -> Result<Step, ModelError> {
            if let Some(value) = last_result_value(convo) {
                return Ok(Step::Done(format!("grounded: {value}")));
            }
            Ok(Step::UseTools(vec![ToolCall {
                id: "1".to_owned(),
                name: "query".to_owned(),
                input: json!({ "symbol": "FOO", "last_days": 3, "metric": "average_close" }),
            }]))
        }
    }

    /// A misbehaving stub that never finishes — always asks for the tool again, ignoring the result.
    struct LoopingModel;
    #[async_trait]
    impl Model for LoopingModel {
        fn name(&self) -> &str {
            "looping"
        }
        async fn respond(
            &mut self,
            _convo: &Conversation,
            _tools: &[ToolSpec],
        ) -> Result<Step, ModelError> {
            Ok(Step::UseTools(vec![ToolCall {
                id: "1".to_owned(),
                name: "query".to_owned(),
                input: json!({ "symbol": "FOO", "last_days": 3, "metric": "average_close" }),
            }]))
        }
    }

    /// A misbehaving stub that answers without ever querying — must be rejected as ungrounded.
    struct UngroundedModel;
    #[async_trait]
    impl Model for UngroundedModel {
        fn name(&self) -> &str {
            "ungrounded"
        }
        async fn respond(
            &mut self,
            _convo: &Conversation,
            _tools: &[ToolSpec],
        ) -> Result<Step, ModelError> {
            Ok(Step::Done("42, trust me".to_owned()))
        }
    }

    struct StubProvider {
        bars: bool,
    }
    #[async_trait]
    impl DataProvider for StubProvider {
        fn name(&self) -> &str {
            "stub"
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::new(self.bars)
        }
        async fn bars(&mut self, _q: &DataQuery) -> Result<Vec<Bar>, ProviderError> {
            Ok(vec![bar(100.0), bar(101.0), bar(102.0)])
        }
    }

    #[tokio::test]
    async fn engine_runs_the_tool_loop_and_grounds() {
        let mut e = Engine::new(Box::new(StubModel), Box::new(StubProvider { bars: true }));
        let a = e.ask("whatever").await.expect("answer");
        assert_eq!(a.metric, Metric::AverageClose);
        assert!((a.value - 101.0).abs() < 1e-9); // avg of 100,101,102
        assert_eq!(a.bars_used, 3);
        assert_eq!((a.model.as_str(), a.provider.as_str()), ("stub", "stub"));
        assert!(a.text.contains("101")); // the model grounded on the fed-back value
    }

    #[tokio::test]
    async fn engine_rejects_a_provider_that_cannot_serve_bars() {
        let mut e = Engine::new(Box::new(StubModel), Box::new(StubProvider { bars: false }));
        assert!(matches!(e.ask("x").await, Err(EngineError::Unsupported(_))));
    }

    #[tokio::test]
    async fn engine_enforces_the_step_budget() {
        let mut e = Engine::new(
            Box::new(LoopingModel),
            Box::new(StubProvider { bars: true }),
        );
        assert!(matches!(e.ask("x").await, Err(EngineError::StepLimit(_))));
    }

    #[tokio::test]
    async fn engine_rejects_an_ungrounded_answer() {
        let mut e = Engine::new(
            Box::new(UngroundedModel),
            Box::new(StubProvider { bars: true }),
        );
        assert!(matches!(e.ask("x").await, Err(EngineError::Ungrounded)));
    }

    #[tokio::test]
    async fn engine_rejects_an_unknown_tool() {
        // Drive run_tool directly with a bogus tool name via a one-off model.
        struct BadToolModel;
        #[async_trait]
        impl Model for BadToolModel {
            fn name(&self) -> &str {
                "bad"
            }
            async fn respond(
                &mut self,
                _c: &Conversation,
                _t: &[ToolSpec],
            ) -> Result<Step, ModelError> {
                Ok(Step::UseTools(vec![ToolCall {
                    id: "1".to_owned(),
                    name: "nope".to_owned(),
                    input: json!({}),
                }]))
            }
        }
        let mut e = Engine::new(
            Box::new(BadToolModel),
            Box::new(StubProvider { bars: true }),
        );
        assert!(matches!(e.ask("x").await, Err(EngineError::Tool(_))));
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
        assert_eq!(
            EngineError::Ungrounded.to_string(),
            "the model answered without querying any data"
        );
        assert_eq!(
            EngineError::StepLimit(8).to_string(),
            "the model exceeded the 8-step tool-use budget"
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
