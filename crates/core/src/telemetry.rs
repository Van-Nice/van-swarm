//! Observability & APM for agent runs (§13).
//!
//! # Overview
//!
//! | Item                    | Purpose                                              |
//! |-------------------------|------------------------------------------------------|
//! | [`AgentSpanKind`]       | Discriminant: thought / tool-call / observation / answer (§13.1) |
//! | [`SpanEvent`]           | One step with timing, token counts, content excerpt  |
//! | [`RunTrace`]            | Full trace: TTFT, cost, context-fill (§13.3–13.5)   |
//! | [`RunTraceBuilder`]     | Accumulator used inside [`crate::react::run_agent_traced`] |
//! | [`ModelPricing`]        | Per-model USD/1M-token prices (§13.3)                |
//! | [`default_pricing`]     | Built-in prices for common models                    |
//! | [`ContextMeter`]        | Context-window fill tracker with overflow alert (§13.5) |
//! | [`TraceStore`]          | Pluggable backend trait (§13.9)                      |
//! | [`FileTraceStore`]      | NDJSON append-only file backend                      |
//! | [`InMemoryTraceStore`]  | In-process trace store for tests                     |
//! | [`SamplingFilter`]      | Probabilistic sampling for live evaluations (§13.10) |
//!
//! # OpenTelemetry bridge (§13.8)
//!
//! Every framework component emits `tracing` spans and events via
//! `#[instrument]`.  Since `tracing-opentelemetry` bridges `tracing` → OTEL,
//! **no additional code is required in this library** to export to Jaeger,
//! Tempo, Honeycomb, or any OTLP backend.  At application startup:
//!
//! ```toml
//! # Cargo.toml (application only, not this library)
//! tracing-opentelemetry = "0.27"
//! opentelemetry_sdk      = { version = "0.26", features = ["rt-tokio"] }
//! opentelemetry-otlp     = { version = "0.26", features = ["grpc-tonic"] }
//! ```
//!
//! ```rust,ignore
//! use opentelemetry_otlp::WithExportConfig;
//! use tracing_opentelemetry::OpenTelemetryLayer;
//! use tracing_subscriber::prelude::*;
//!
//! let tracer = opentelemetry_otlp::new_pipeline()
//!     .tracing()
//!     .with_exporter(
//!         opentelemetry_otlp::new_exporter()
//!             .tonic()
//!             .with_endpoint("http://localhost:4317"),
//!     )
//!     .install_batch(opentelemetry_sdk::runtime::Tokio)?;
//!
//! tracing_subscriber::registry()
//!     .with(tracing_subscriber::fmt::layer())
//!     .with(OpenTelemetryLayer::new(tracer))
//!     .init();
//! // All #[instrument] spans from run_agent, step, tool calls, etc. now flow
//! // to your OTEL backend automatically.
//! ```

use std::{path::PathBuf, sync::Arc, time::Duration};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::{fs::OpenOptions, io::AsyncWriteExt, sync::Mutex};

use crate::{message::TokenUsage, Result};

// ── AgentSpanKind ─────────────────────────────────────────────────────────────

/// The kind of step recorded in an agent run trace (§13.1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentSpanKind {
    /// The model produced reasoning text (chain-of-thought, before tool calls).
    Thought,
    /// The agent dispatched a tool call.
    ToolCall {
        /// Name of the tool that was invoked.
        name: String,
    },
    /// A tool result was received and appended to the context.
    Observation {
        /// Name of the tool that produced the result.
        tool_name: String,
    },
    /// The model produced a final answer; the run is complete.
    FinalAnswer,
}

// ── SpanEvent ─────────────────────────────────────────────────────────────────

/// One recorded step in an agent run trace (§13.1–13.2).
///
/// Records what happened, when it happened, how long it took, and
/// how many tokens were consumed.  Content is truncated to 500 chars
/// to keep stored traces compact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    /// The kind of step.
    pub kind: AgentSpanKind,
    /// Truncated content excerpt (reasoning text, tool args, or result).
    pub content: String,
    /// Milliseconds since run start when this step began.
    pub started_at_ms: u64,
    /// Elapsed duration of this step (model latency or tool execution time).
    pub duration_ms: u64,
    /// Input tokens consumed (model calls only; 0 for tool/observation spans).
    pub input_tokens: u32,
    /// Output tokens produced (model calls only; 0 for tool/observation spans).
    pub output_tokens: u32,
}

// ── RunTrace ──────────────────────────────────────────────────────────────────

/// Full observability trace for one agent run (§13.1–13.5).
///
/// Returned by [`crate::react::run_agent_traced`] alongside the final answer.
/// Persist it with a [`TraceStore`] for historical queries or forward to
/// OpenTelemetry (§13.8).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunTrace {
    /// Globally unique identifier for this run (UUID v4).
    pub run_id: String,
    /// Display name of the agent.
    pub agent_name: String,
    /// Model ID used for all LLM calls (e.g. `"claude-opus-4-6"`).
    pub model_id: String,
    /// Ordered events: thoughts, tool calls, observations, final answer.
    pub spans: Vec<SpanEvent>,
    /// Time from run start to first model response — analogous to TTFT (§13.4).
    ///
    /// `None` if no model call completed (e.g. run errored immediately).
    pub ttft_ms: Option<u64>,
    /// Total wall-clock duration of the run in milliseconds.
    pub total_duration_ms: u64,
    /// Accumulated input tokens across all model calls.
    pub total_input_tokens: u32,
    /// Accumulated output tokens across all model calls.
    pub total_output_tokens: u32,
    /// Estimated USD cost (None when model is absent from the pricing table).
    pub estimated_cost_usd: Option<f64>,
    /// Peak context-window fill: `tokens_used / max_context_tokens × 100` (§13.5).
    ///
    /// `0.0` when `max_context_tokens` was not configured.
    pub context_utilization_pct: f64,
    /// Number of tool calls dispatched (executed path length for SPL).
    pub tool_call_count: usize,
    /// Number of ReAct iterations (model turns completed).
    pub iterations: usize,
}

impl RunTrace {
    /// Re-compute estimated cost using a custom pricing table.
    ///
    /// Useful when the trace was stored before pricing was configured, or
    /// when you want to compare costs across different providers.
    pub fn compute_cost(&self, pricing: &[ModelPricing]) -> Option<f64> {
        let p = pricing.iter().find(|p| p.model_id == self.model_id)?;
        let input_cost = (self.total_input_tokens as f64 / 1_000_000.0) * p.input_cost_per_1m;
        let output_cost = (self.total_output_tokens as f64 / 1_000_000.0) * p.output_cost_per_1m;
        Some(input_cost + output_cost)
    }

    /// Return all spans of a specific kind.
    pub fn spans_of_kind(&self, kind: &AgentSpanKind) -> Vec<&SpanEvent> {
        self.spans
            .iter()
            .filter(|s| &s.kind == kind)
            .collect()
    }

    /// Summarise the trace for display or logging.
    pub fn summary(&self) -> String {
        format!(
            "run={} agent={} model={} iters={} tools={} in={} out={} cost={:.4}$ dur={}ms ctx={:.1}%",
            &self.run_id[..8],
            self.agent_name,
            self.model_id,
            self.iterations,
            self.tool_call_count,
            self.total_input_tokens,
            self.total_output_tokens,
            self.estimated_cost_usd.unwrap_or(0.0),
            self.total_duration_ms,
            self.context_utilization_pct,
        )
    }
}

// ── ModelPricing ──────────────────────────────────────────────────────────────

/// Per-model token pricing for cost attribution (§13.3).
#[derive(Debug, Clone)]
pub struct ModelPricing {
    /// Exact model ID as used in `CompletionRequest::model`.
    pub model_id: &'static str,
    /// USD per 1 million input tokens.
    pub input_cost_per_1m: f64,
    /// USD per 1 million output tokens.
    pub output_cost_per_1m: f64,
}

/// Built-in pricing table for common models (as of early 2026).
///
/// Prices are in USD per 1M tokens (input / output).  Update this table
/// as provider pricing changes, or supply your own via
/// [`RunTraceBuilder::with_pricing`].
pub fn default_pricing() -> Vec<ModelPricing> {
    vec![
        // ── Anthropic ─────────────────────────────────────────────────────────
        ModelPricing { model_id: "claude-opus-4-6",            input_cost_per_1m: 15.0,  output_cost_per_1m: 75.0  },
        ModelPricing { model_id: "claude-sonnet-4-6",          input_cost_per_1m: 3.0,   output_cost_per_1m: 15.0  },
        ModelPricing { model_id: "claude-haiku-4-5-20251001",  input_cost_per_1m: 0.8,   output_cost_per_1m: 4.0   },
        // ── OpenAI ────────────────────────────────────────────────────────────
        ModelPricing { model_id: "gpt-4o",                     input_cost_per_1m: 2.5,   output_cost_per_1m: 10.0  },
        ModelPricing { model_id: "gpt-4o-mini",                input_cost_per_1m: 0.15,  output_cost_per_1m: 0.6   },
        ModelPricing { model_id: "o1",                         input_cost_per_1m: 15.0,  output_cost_per_1m: 60.0  },
        ModelPricing { model_id: "o3-mini",                    input_cost_per_1m: 1.1,   output_cost_per_1m: 4.4   },
        // ── Google ────────────────────────────────────────────────────────────
        ModelPricing { model_id: "gemini-2.0-flash",           input_cost_per_1m: 0.075, output_cost_per_1m: 0.3   },
        ModelPricing { model_id: "gemini-2.0-flash-lite",      input_cost_per_1m: 0.018, output_cost_per_1m: 0.072 },
        ModelPricing { model_id: "gemini-2.0-pro-exp",         input_cost_per_1m: 1.25,  output_cost_per_1m: 5.0   },
    ]
}

// ── ContextMeter ──────────────────────────────────────────────────────────────

/// Tracks context-window utilisation and alerts on near-overflow (§13.5).
///
/// # Example
/// ```
/// use openswarm_core::telemetry::ContextMeter;
///
/// let mut m = ContextMeter::new(200_000); // 200K token limit
/// m.record(5_000);
/// m.record(3_000);
/// assert!(!m.is_near_limit(0.9));  // still under 90 %
/// ```
#[derive(Debug, Clone, Default)]
pub struct ContextMeter {
    /// Maximum context tokens (0 = unknown → utilisation always 0.0).
    pub max_tokens: u32,
    /// Cumulative tokens added across all turns.
    pub used_tokens: u32,
}

impl ContextMeter {
    /// Create a meter with a known context-window limit.
    pub fn new(max_tokens: u32) -> Self {
        Self { max_tokens, used_tokens: 0 }
    }

    /// Add `tokens` to the running total.
    pub fn record(&mut self, tokens: u32) {
        self.used_tokens = self.used_tokens.saturating_add(tokens);
    }

    /// Utilisation as a fraction in `[0.0, 1.0]`.  Clamped at 1.0 on overflow.
    pub fn utilization(&self) -> f64 {
        if self.max_tokens == 0 {
            return 0.0;
        }
        (self.used_tokens as f64 / self.max_tokens as f64).min(1.0)
    }

    /// Utilisation as a percentage: `utilization() × 100`.
    pub fn utilization_pct(&self) -> f64 {
        self.utilization() * 100.0
    }

    /// Returns `true` when fill ≥ `threshold` (e.g. `0.8` = 80 %).
    ///
    /// Use to emit an `ContextWindowExceeded` warning before the model
    /// actually truncates.
    pub fn is_near_limit(&self, threshold: f64) -> bool {
        self.utilization() >= threshold
    }
}

// ── RunTraceBuilder ───────────────────────────────────────────────────────────

/// Accumulates [`SpanEvent`]s during a run and finalises them into a [`RunTrace`].
///
/// Used internally by [`crate::react::run_agent_traced`]; exposed here for
/// callers that implement their own run loop.
pub struct RunTraceBuilder {
    run_id: String,
    agent_name: String,
    model_id: String,
    spans: Vec<SpanEvent>,
    run_start: std::time::Instant,
    ttft_ms: Option<u64>,
    meter: ContextMeter,
    pricing: Vec<ModelPricing>,
}

impl RunTraceBuilder {
    /// Create a new builder.
    ///
    /// `max_context_tokens = 0` disables context-utilisation tracking.
    pub fn new(
        agent_name: impl Into<String>,
        model_id: impl Into<String>,
        max_context_tokens: u32,
    ) -> Self {
        Self {
            run_id: uuid::Uuid::new_v4().to_string(),
            agent_name: agent_name.into(),
            model_id: model_id.into(),
            spans: Vec::new(),
            run_start: std::time::Instant::now(),
            ttft_ms: None,
            meter: ContextMeter::new(max_context_tokens),
            pricing: default_pricing(),
        }
    }

    /// Override the pricing table (e.g. for private pricing or newer models).
    pub fn with_pricing(mut self, pricing: Vec<ModelPricing>) -> Self {
        self.pricing = pricing;
        self
    }

    /// Milliseconds elapsed since the builder was created.
    pub fn elapsed_ms(&self) -> u64 {
        self.run_start.elapsed().as_millis() as u64
    }

    /// Record a model reasoning step (chain-of-thought before a tool call).
    pub fn record_thought(
        &mut self,
        content: &str,
        duration: Duration,
        usage: TokenUsage,
    ) {
        if self.ttft_ms.is_none() {
            self.ttft_ms = Some(self.elapsed_ms());
        }
        self.meter.record(usage.input_tokens + usage.output_tokens);
        let started = self.elapsed_ms().saturating_sub(duration.as_millis() as u64);
        self.spans.push(SpanEvent {
            kind: AgentSpanKind::Thought,
            content: truncate(content, 500),
            started_at_ms: started,
            duration_ms: duration.as_millis() as u64,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
        });
    }

    /// Record a tool-call dispatch.
    pub fn record_tool_call(
        &mut self,
        tool_name: &str,
        args_summary: &str,
        started_at_ms: u64,
        duration: Duration,
    ) {
        self.spans.push(SpanEvent {
            kind: AgentSpanKind::ToolCall { name: tool_name.to_string() },
            content: truncate(args_summary, 500),
            started_at_ms,
            duration_ms: duration.as_millis() as u64,
            input_tokens: 0,
            output_tokens: 0,
        });
    }

    /// Record a tool result (observation returned to the model).
    pub fn record_observation(
        &mut self,
        tool_name: &str,
        content: &str,
        started_at_ms: u64,
        duration: Duration,
    ) {
        self.spans.push(SpanEvent {
            kind: AgentSpanKind::Observation { tool_name: tool_name.to_string() },
            content: truncate(content, 500),
            started_at_ms,
            duration_ms: duration.as_millis() as u64,
            input_tokens: 0,
            output_tokens: 0,
        });
    }

    /// Record the final answer (last model response).
    pub fn record_final_answer(
        &mut self,
        content: &str,
        duration: Duration,
        usage: TokenUsage,
    ) {
        if self.ttft_ms.is_none() {
            self.ttft_ms = Some(self.elapsed_ms());
        }
        self.meter.record(usage.input_tokens + usage.output_tokens);
        let started = self.elapsed_ms().saturating_sub(duration.as_millis() as u64);
        self.spans.push(SpanEvent {
            kind: AgentSpanKind::FinalAnswer,
            content: truncate(content, 500),
            started_at_ms: started,
            duration_ms: duration.as_millis() as u64,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
        });
    }

    /// Finalise and return the completed [`RunTrace`].
    pub fn build(self, tool_call_count: usize, iterations: usize) -> RunTrace {
        let total_input: u32 = self.spans.iter().map(|s| s.input_tokens).sum();
        let total_output: u32 = self.spans.iter().map(|s| s.output_tokens).sum();
        let total_duration_ms = self.run_start.elapsed().as_millis() as u64;
        let context_utilization_pct = self.meter.utilization_pct();

        let estimated_cost_usd = self
            .pricing
            .iter()
            .find(|p| p.model_id == self.model_id)
            .map(|p| {
                (total_input as f64 / 1_000_000.0) * p.input_cost_per_1m
                    + (total_output as f64 / 1_000_000.0) * p.output_cost_per_1m
            });

        RunTrace {
            run_id: self.run_id,
            agent_name: self.agent_name,
            model_id: self.model_id,
            spans: self.spans,
            ttft_ms: self.ttft_ms,
            total_duration_ms,
            total_input_tokens: total_input,
            total_output_tokens: total_output,
            estimated_cost_usd,
            context_utilization_pct,
            tool_call_count,
            iterations,
        }
    }
}

/// Truncate `s` to at most `max_chars` characters at a char boundary.
fn truncate(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        return s.to_string();
    }
    // Walk back to a valid UTF-8 char boundary.
    let mut boundary = max_chars;
    while !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!("{}…", &s[..boundary])
}

// ── TraceStore ────────────────────────────────────────────────────────────────

/// Pluggable backend for persisting and querying run traces (§13.9).
///
/// Implementations include [`FileTraceStore`] (NDJSON) and
/// [`InMemoryTraceStore`] (tests).  Users can supply their own for
/// Postgres, SQLite, S3, etc.
#[async_trait]
pub trait TraceStore: Send + Sync {
    /// Append a completed trace.  Must be atomic or idempotent.
    async fn append(&self, trace: &RunTrace) -> Result<()>;
    /// Return the most recent `limit` traces in insertion order.
    async fn list(&self, limit: usize) -> Result<Vec<RunTrace>>;
}

// ── FileTraceStore ────────────────────────────────────────────────────────────

/// Append-only NDJSON file backend (§13.9).
///
/// Each [`RunTrace`] is serialised as a single JSON line.  Compatible
/// with `jq`, `grep`, log-ship pipelines (Fluentd, Vector), and most
/// log-ingestion platforms.
pub struct FileTraceStore {
    path: PathBuf,
}

impl FileTraceStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait]
impl TraceStore for FileTraceStore {
    async fn append(&self, trace: &RunTrace) -> Result<()> {
        let line = serde_json::to_string(trace)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        file.write_all(format!("{line}\n").as_bytes()).await?;
        Ok(())
    }

    async fn list(&self, limit: usize) -> Result<Vec<RunTrace>> {
        let content = tokio::fs::read_to_string(&self.path).await?;
        let traces: Vec<RunTrace> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .take(limit)
            .collect();
        Ok(traces)
    }
}

// ── InMemoryTraceStore ────────────────────────────────────────────────────────

/// In-process trace store suitable for tests and short-lived processes.
#[derive(Default, Clone)]
pub struct InMemoryTraceStore {
    traces: Arc<Mutex<Vec<RunTrace>>>,
}

impl InMemoryTraceStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clone and return all stored traces.
    pub async fn all(&self) -> Vec<RunTrace> {
        self.traces.lock().await.clone()
    }
}

#[async_trait]
impl TraceStore for InMemoryTraceStore {
    async fn append(&self, trace: &RunTrace) -> Result<()> {
        self.traces.lock().await.push(trace.clone());
        Ok(())
    }

    async fn list(&self, limit: usize) -> Result<Vec<RunTrace>> {
        Ok(self.traces.lock().await.iter().take(limit).cloned().collect())
    }
}

// ── SamplingFilter ────────────────────────────────────────────────────────────

/// Probabilistic gate for live evaluations (§13.10).
///
/// Avoids running expensive scorers on every request in production.
/// Uses UUID v4 entropy (OS CSPRNG) — no external `rand` crate required.
///
/// # Example
/// ```
/// use openswarm_core::telemetry::SamplingFilter;
///
/// let filter = SamplingFilter::new(0.1); // 10 % of traffic
/// if filter.should_sample() {
///     // run expensive scorer
/// }
/// ```
#[derive(Debug, Clone)]
pub struct SamplingFilter {
    rate: f64,
}

impl SamplingFilter {
    /// Create a filter with the given sample rate clamped to `[0.0, 1.0]`.
    pub fn new(rate: f64) -> Self {
        Self { rate: rate.clamp(0.0, 1.0) }
    }

    /// Sample every request (rate = 1.0).
    pub fn always() -> Self {
        Self::new(1.0)
    }

    /// Sample no requests (rate = 0.0).
    pub fn never() -> Self {
        Self::new(0.0)
    }

    /// Returns `true` with probability equal to `rate`.
    pub fn should_sample(&self) -> bool {
        if self.rate >= 1.0 {
            return true;
        }
        if self.rate <= 0.0 {
            return false;
        }
        // First 8 bytes of UUID v4 are uniformly random (OS CSPRNG).
        let u = uuid::Uuid::new_v4();
        let bytes: [u8; 8] = u.as_bytes()[..8].try_into().expect("8 bytes");
        let n = u64::from_be_bytes(bytes);
        let threshold = (self.rate * u64::MAX as f64) as u64;
        n < threshold
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── ContextMeter ──────────────────────────────────────────────────────────

    #[test]
    fn context_meter_utilization() {
        let mut m = ContextMeter::new(1_000);
        m.record(500);
        assert!((m.utilization() - 0.5).abs() < 1e-9);
        assert!((m.utilization_pct() - 50.0).abs() < 1e-9);
        assert!(m.is_near_limit(0.4));
        assert!(!m.is_near_limit(0.9));
    }

    #[test]
    fn context_meter_clamps_at_one() {
        let mut m = ContextMeter::new(1_000);
        m.record(1_500);
        assert!((m.utilization() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn context_meter_zero_max() {
        let m = ContextMeter::new(0);
        assert!(m.utilization() == 0.0);
    }

    // ── RunTraceBuilder ───────────────────────────────────────────────────────

    #[test]
    fn trace_builder_full_run() {
        let mut b = RunTraceBuilder::new("myagent", "claude-opus-4-6", 200_000);
        b.record_thought(
            "Let me think about this…",
            Duration::from_millis(400),
            TokenUsage { input_tokens: 100, output_tokens: 50, ..Default::default() },
        );
        b.record_tool_call("web_search", r#"{"q":"rust"}"#, 400, Duration::from_millis(200));
        b.record_observation("web_search", "Rust is a systems language.", 600, Duration::from_millis(10));
        b.record_final_answer(
            "Rust is great.",
            Duration::from_millis(300),
            TokenUsage { input_tokens: 200, output_tokens: 30, ..Default::default() },
        );

        let trace = b.build(1, 2);
        assert_eq!(trace.agent_name, "myagent");
        assert_eq!(trace.model_id, "claude-opus-4-6");
        assert_eq!(trace.spans.len(), 4);
        assert_eq!(trace.total_input_tokens, 300);
        assert_eq!(trace.total_output_tokens, 80);
        assert!(trace.estimated_cost_usd.is_some());
        assert!(trace.estimated_cost_usd.unwrap() > 0.0);
        assert!(trace.ttft_ms.is_some());
        assert_eq!(trace.tool_call_count, 1);
        assert_eq!(trace.iterations, 2);
    }

    #[test]
    fn trace_compute_cost_custom_pricing() {
        let b = RunTraceBuilder::new("a", "my-model", 0);
        let mut trace = b.build(0, 1);
        trace.total_input_tokens = 1_000_000;
        trace.total_output_tokens = 500_000;
        let pricing = vec![ModelPricing {
            model_id: "my-model",
            input_cost_per_1m: 10.0,
            output_cost_per_1m: 20.0,
        }];
        let cost = trace.compute_cost(&pricing).unwrap();
        // 1M input @ $10/1M + 0.5M output @ $20/1M = $10 + $10 = $20
        assert!((cost - 20.0).abs() < 1e-6);
    }

    #[test]
    fn truncate_at_boundary() {
        let s = "hello world";
        assert_eq!(truncate(s, 5), "hello…");
        assert_eq!(truncate(s, 100), "hello world");
    }

    // ── SamplingFilter ────────────────────────────────────────────────────────

    #[test]
    fn sampling_always() {
        let f = SamplingFilter::always();
        assert!((0..20).all(|_| f.should_sample()));
    }

    #[test]
    fn sampling_never() {
        let f = SamplingFilter::never();
        assert!((0..20).all(|_| !f.should_sample()));
    }

    #[test]
    fn sampling_statistical_50pct() {
        // With 50 % rate and 1000 trials, expect 400–600 true values.
        let f = SamplingFilter::new(0.5);
        let hits = (0..1_000).filter(|_| f.should_sample()).count();
        assert!(hits > 400 && hits < 600, "hits={hits} not near 500");
    }

    // ── FileTraceStore ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn file_trace_store_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("traces.ndjson");
        let store = FileTraceStore::new(&path);

        let trace = RunTraceBuilder::new("agent", "gpt-4o-mini", 0).build(0, 1);
        store.append(&trace).await.unwrap();
        store.append(&trace).await.unwrap(); // two lines

        let list = store.list(10).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].model_id, "gpt-4o-mini");
    }

    // ── InMemoryTraceStore ────────────────────────────────────────────────────

    #[tokio::test]
    async fn in_memory_trace_store() {
        let store = InMemoryTraceStore::new();
        let trace = RunTraceBuilder::new("agent", "gpt-4o", 0).build(0, 1);

        store.append(&trace).await.unwrap();
        store.append(&trace).await.unwrap();

        let list = store.list(1).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(store.all().await.len(), 2);
    }
}
