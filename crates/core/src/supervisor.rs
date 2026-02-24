//! SupervisorAgent / Router: classify input and route to model or task (§11.1).
//!
//! Use a `Router` to send simple tasks to fast/cheap models (Tier 1), planning/tool-use to
//! mid-tier (Tier 2), and complex reasoning to frontier models (Tier 3).

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    message::{CompletionRequest, ContentBlock, Message, Role},
    providers::ModelProvider,
    Result,
};

// ─────────────────────────────────────────────────────────────────────────────
// Route (§11.2–11.4)
// ─────────────────────────────────────────────────────────────────────────────

/// Routing decision: which tier (and optionally which model) should handle the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// Tier 1: simple tasks (intent, formatting, summarization) → fast/cheap model (e.g. Flash).
    Tier1,
    /// Tier 2: planning, tool use → mid-tier model (e.g. Gemini 2.5 Flash).
    Tier2,
    /// Tier 3: complex reasoning, research, coding → frontier model (e.g. Pro / O1).
    Tier3,
}

// ─────────────────────────────────────────────────────────────────────────────
// Router trait (§11.1)
// ─────────────────────────────────────────────────────────────────────────────

/// Classifies user input and returns a route so the caller can select the appropriate
/// model or agent (SupervisorAgent / Router pattern).
#[async_trait]
pub trait Router: Send + Sync {
    /// Classify the input and return the route (Tier1 / Tier2 / Tier3).
    async fn route(&self, input: &str) -> Result<Route>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Stub implementations
// ─────────────────────────────────────────────────────────────────────────────

/// Always returns Tier1. Use for tests or when all traffic goes to one model.
#[derive(Debug, Default)]
pub struct AlwaysTier1;

#[async_trait]
impl Router for AlwaysTier1 {
    async fn route(&self, _input: &str) -> Result<Route> {
        Ok(Route::Tier1)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// KeywordRouter — heuristic tier classification (§11.2–§11.4)
// ─────────────────────────────────────────────────────────────────────────────

/// A keyword-based [`Router`] that classifies inputs by substring matching (§11.2–§11.4).
///
/// Keywords are checked in priority order: Tier 1 first, then Tier 2. Any input
/// that does not match either list is routed to Tier 3 (complex reasoning).
///
/// # Example
///
/// ```rust,no_run
/// use vanswarm_core::supervisor::{KeywordRouter, Route, Router};
///
/// # async fn example() {
/// let router = KeywordRouter::default_keywords();
/// assert_eq!(router.route("summarize this document").await.unwrap(), Route::Tier1);
/// assert_eq!(router.route("write a Rust function").await.unwrap(), Route::Tier2);
/// assert_eq!(router.route("prove Fermat's last theorem").await.unwrap(), Route::Tier3);
/// # }
/// ```
pub struct KeywordRouter {
    tier1_keywords: Vec<String>,
    tier2_keywords: Vec<String>,
}

impl KeywordRouter {
    /// Create a router with custom keyword lists.
    pub fn new(
        tier1: impl IntoIterator<Item = impl Into<String>>,
        tier2: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            tier1_keywords: tier1.into_iter().map(Into::into).collect(),
            tier2_keywords: tier2.into_iter().map(Into::into).collect(),
        }
    }

    /// Create a router with sensible built-in defaults:
    ///
    /// * **Tier 1** — summarize, format, translate, classify, yes/no questions.
    /// * **Tier 2** — plan, search, code, write, create, analyze, compare.
    /// * **Tier 3** — everything else (default).
    pub fn default_keywords() -> Self {
        Self::new(
            [
                "summarize", "format", "translate", "classify", "list",
                "convert", "yes or no", "which is", "what is the", "who is",
            ],
            [
                "plan", "schedule", "search", "find", "code", "write",
                "create", "analyze", "compare", "implement", "generate",
                "fix", "debug", "refactor",
            ],
        )
    }
}

#[async_trait]
impl Router for KeywordRouter {
    async fn route(&self, input: &str) -> Result<Route> {
        let lower = input.to_lowercase();
        if self.tier1_keywords.iter().any(|kw| lower.contains(kw.as_str())) {
            return Ok(Route::Tier1);
        }
        if self.tier2_keywords.iter().any(|kw| lower.contains(kw.as_str())) {
            return Ok(Route::Tier2);
        }
        Ok(Route::Tier3)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LlmRouter — LLM-powered tier classification (§11.2–§11.4)
// ─────────────────────────────────────────────────────────────────────────────

/// An LLM-powered [`Router`] that asks a model to classify the input (§11.2–§11.4).
///
/// A compact routing prompt is sent to the configured provider/model.  The model
/// is expected to reply with exactly one of: `Tier1`, `Tier2`, `Tier3`.
/// Falls back to **Tier 3** on parse errors (safe-side: use the most capable model).
///
/// Use a cheap/fast model (e.g. `gemini-2.0-flash`) for low-latency routing.
pub struct LlmRouter {
    provider: Arc<dyn ModelProvider>,
    /// Model ID to use for classification (typically a fast, cheap model).
    pub model_id: String,
}

impl LlmRouter {
    pub fn new(provider: Arc<dyn ModelProvider>, model_id: impl Into<String>) -> Self {
        Self { provider, model_id: model_id.into() }
    }
}

#[async_trait]
impl Router for LlmRouter {
    async fn route(&self, input: &str) -> Result<Route> {
        let prompt = format!(
            "Classify the following user request into one tier:\n\
             - Tier1: simple tasks (summarize, format, translate, yes/no, short lookup)\n\
             - Tier2: planning, tool use, code generation, multi-step analysis\n\
             - Tier3: complex reasoning, research, mathematics, frontier-model tasks\n\
             \n\
             Reply with EXACTLY one word: Tier1, Tier2, or Tier3 — no explanation.\n\
             \n\
             Request: {input}"
        );
        let req = CompletionRequest::new(
            &self.model_id,
            vec![Message { role: Role::User, content: vec![ContentBlock::Text { text: prompt }] }],
        )
        .with_max_tokens(5);

        let resp = self.provider.complete(req).await?;
        let text: String = resp
            .message
            .content
            .iter()
            .filter_map(|b| if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None })
            .collect::<Vec<_>>()
            .join("");

        let route = if text.contains("Tier1") {
            Route::Tier1
        } else if text.contains("Tier2") {
            Route::Tier2
        } else {
            // Fail safe: route to Tier3 (most capable model).
            Route::Tier3
        };
        Ok(route)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TQGR — Trajectory-Quality Growth Rate (§11.8–11.11)
// ─────────────────────────────────────────────────────────────────────────────

/// The outcome of a single [`TqgrTracker::record`] call.
#[derive(Debug, Clone, PartialEq)]
pub enum TqgrDecision {
    /// Quality is still improving — continue the run.
    Continue,
    /// TQGR has been below `epsilon` for `patience` consecutive turns.
    /// The run should be forced to produce a final answer or be marked as failed.
    ForceEnd {
        /// Computed TQGR on the turn that triggered the decision.
        tqgr: f64,
        /// The latest quality score that was recorded.
        latest_quality: f64,
    },
}

/// Detects convergence (or stagnation) in agent runs using the
/// Trajectory-Quality Growth Rate metric (§11.8).
///
/// # How it works
///
/// After each agent turn, call [`TqgrTracker::record`] with a quality score
/// in `[0.0, 1.0]` (e.g. from a [`crate::evaluators::Scorer`] or a heuristic).
///
/// TQGR for turn *t* is defined as:
///
/// ```text
/// TQGR_t = (Q_t − Q_{t−1}) / max(Q_{t−1}, ε_floor)
/// ```
///
/// When TQGR < `epsilon` for `patience` consecutive turns, the tracker returns
/// [`TqgrDecision::ForceEnd`] signalling that the agent should be stopped.
///
/// # Example
/// ```
/// use vanswarm_core::supervisor::{TqgrTracker, TqgrDecision};
///
/// let mut tracker = TqgrTracker::new(0.05, 3); // ε=5%, patience=3 turns
/// assert_eq!(tracker.record(0.2), TqgrDecision::Continue);
/// assert_eq!(tracker.record(0.5), TqgrDecision::Continue); // big jump
/// assert_eq!(tracker.record(0.51), TqgrDecision::Continue); // small, count=1
/// assert_eq!(tracker.record(0.51), TqgrDecision::Continue); // small, count=2
/// // Third consecutive small gain → ForceEnd
/// assert!(matches!(tracker.record(0.51), TqgrDecision::ForceEnd { .. }));
/// ```
#[derive(Debug, Clone)]
pub struct TqgrTracker {
    /// Minimum growth rate to be considered "improving".
    pub epsilon: f64,
    /// Number of consecutive below-threshold turns before forcing end.
    pub patience: usize,
    /// History of quality scores (most recent last).
    quality_history: Vec<f64>,
    /// How many consecutive turns have been below `epsilon`.
    below_threshold_count: usize,
}

impl TqgrTracker {
    /// Create a new tracker.
    ///
    /// * `epsilon` — minimum TQGR to count as improvement (e.g. `0.05` = 5 %).
    /// * `patience` — how many consecutive stagnant turns before forcing end.
    pub fn new(epsilon: f64, patience: usize) -> Self {
        Self {
            epsilon: epsilon.max(0.0),
            patience: patience.max(1),
            quality_history: Vec::new(),
            below_threshold_count: 0,
        }
    }

    /// Record a quality score and return whether the run should continue.
    ///
    /// On the first call (no history), always returns [`TqgrDecision::Continue`].
    pub fn record(&mut self, quality: f64) -> TqgrDecision {
        let quality = quality.clamp(0.0, 1.0);

        if let Some(&prev) = self.quality_history.last() {
            let tqgr = self.compute_tqgr(quality, prev);
            if tqgr < self.epsilon {
                self.below_threshold_count += 1;
                if self.below_threshold_count >= self.patience {
                    self.quality_history.push(quality);
                    return TqgrDecision::ForceEnd { tqgr, latest_quality: quality };
                }
            } else {
                // Quality improving again — reset the patience counter.
                self.below_threshold_count = 0;
            }
        }

        self.quality_history.push(quality);
        TqgrDecision::Continue
    }

    /// Compute TQGR between two quality values.
    ///
    /// Uses `max(prev, 1e-6)` as the denominator to avoid division by zero.
    fn compute_tqgr(&self, current: f64, prev: f64) -> f64 {
        let denom = prev.max(1e-6);
        (current - prev) / denom
    }

    /// Return the current convergence score: latest quality × (1 − below_ratio).
    ///
    /// Exposed via the API for APM dashboards (§11.10).
    /// * `1.0` = high quality with no stagnation.
    /// * `0.0` = stagnant and/or low quality.
    pub fn convergence_score(&self) -> f64 {
        let latest = self.quality_history.last().copied().unwrap_or(0.0);
        let stagnation_ratio = if self.patience > 0 {
            self.below_threshold_count as f64 / self.patience as f64
        } else {
            0.0
        };
        (latest * (1.0 - stagnation_ratio)).clamp(0.0, 1.0)
    }

    /// Return the full quality history (oldest first).
    pub fn history(&self) -> &[f64] {
        &self.quality_history
    }

    /// Reset the tracker state (e.g. for a new run).
    pub fn reset(&mut self) {
        self.quality_history.clear();
        self.below_threshold_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn always_tier1_routes() {
        let r = AlwaysTier1;
        assert_eq!(r.route("hello").await.unwrap(), Route::Tier1);
        assert_eq!(r.route("complex query").await.unwrap(), Route::Tier1);
    }

    // ── TQGR ────────────────────────────────────────────────────────────────

    #[test]
    fn tqgr_first_call_always_continues() {
        let mut t = TqgrTracker::new(0.05, 3);
        assert_eq!(t.record(0.5), TqgrDecision::Continue);
    }

    #[test]
    fn tqgr_improving_runs_continue() {
        let mut t = TqgrTracker::new(0.05, 3);
        t.record(0.2);
        assert_eq!(t.record(0.5), TqgrDecision::Continue); // big jump
        assert_eq!(t.record(0.8), TqgrDecision::Continue); // still big
    }

    #[test]
    fn tqgr_patience_triggers_force_end() {
        let mut t = TqgrTracker::new(0.05, 3);
        t.record(0.5);
        // Three consecutive tiny gains (< 5 %)
        assert_eq!(t.record(0.501), TqgrDecision::Continue); // count=1
        assert_eq!(t.record(0.502), TqgrDecision::Continue); // count=2
        let decision = t.record(0.502); // count=3 → force end
        assert!(matches!(decision, TqgrDecision::ForceEnd { .. }));
    }

    #[test]
    fn tqgr_reset_after_improvement() {
        let mut t = TqgrTracker::new(0.05, 3);
        t.record(0.5);
        t.record(0.501); // below, count=1
        t.record(0.501); // below, count=2
        t.record(0.8);   // big jump → reset count
        // Now two more small gains — shouldn't trigger yet
        assert_eq!(t.record(0.801), TqgrDecision::Continue); // count=1
        assert_eq!(t.record(0.801), TqgrDecision::Continue); // count=2
    }

    #[test]
    fn tqgr_convergence_score_range() {
        let mut t = TqgrTracker::new(0.05, 3);
        t.record(0.8);
        let s = t.convergence_score();
        assert!(s >= 0.0 && s <= 1.0);
    }

    #[test]
    fn tqgr_history() {
        let mut t = TqgrTracker::new(0.05, 3);
        t.record(0.3);
        t.record(0.6);
        assert_eq!(t.history(), &[0.3, 0.6]);
    }
}
