//! SupervisorAgent / Router: classify input and route to model or task (§11.1).
//!
//! Use a `Router` to send simple tasks to fast/cheap models (Tier 1), planning/tool-use to
//! mid-tier (Tier 2), and complex reasoning to frontier models (Tier 3).

use async_trait::async_trait;

use crate::Result;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn always_tier1_routes() {
        let r = AlwaysTier1;
        assert_eq!(r.route("hello").await.unwrap(), Route::Tier1);
        assert_eq!(r.route("complex query").await.unwrap(), Route::Tier1);
    }
}
