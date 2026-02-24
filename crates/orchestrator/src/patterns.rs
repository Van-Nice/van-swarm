//! Ensemble / voting patterns for multi-agent reliability (checklist §15.6).
//!
//! These helpers take a set of candidate answers (strings) and return the
//! agreed-upon answer using either majority vote or cosine-similarity
//! clustering.

// ─────────────────────────────────────────────────────────────────────────────
// Majority vote (§15.6)
// ─────────────────────────────────────────────────────────────────────────────

/// Choose the most common answer from `candidates` using exact-string majority
/// vote (§15.6).
///
/// Returns `None` if `candidates` is empty.  Ties are broken by the first
/// winner encountered (stable across equal counts).
///
/// ```
/// use vanswarm_orchestrator::patterns::majority_vote;
/// let candidates = vec!["Paris", "Paris", "Berlin"];
/// assert_eq!(majority_vote(&candidates), Some("Paris"));
/// ```
pub fn majority_vote<'a>(candidates: &'a [&'a str]) -> Option<&'a str> {
    if candidates.is_empty() {
        return None;
    }

    // Count occurrences of each unique answer.
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for &c in candidates {
        if let Some(entry) = counts.iter_mut().find(|(k, _)| *k == c) {
            entry.1 += 1;
        } else {
            counts.push((c, 1));
        }
    }

    counts.into_iter().max_by_key(|(_, count)| *count).map(|(winner, _)| winner)
}

/// Majority vote over owned `String` candidates.
///
/// Returns `None` if `candidates` is empty.
///
/// ```
/// use vanswarm_orchestrator::patterns::majority_vote_owned;
/// let candidates = vec!["Paris".to_string(), "Paris".to_string(), "Berlin".to_string()];
/// assert_eq!(majority_vote_owned(&candidates).as_deref(), Some("Paris"));
/// ```
pub fn majority_vote_owned(candidates: &[String]) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    let refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
    majority_vote(&refs).map(str::to_owned)
}

// ─────────────────────────────────────────────────────────────────────────────
// Similarity-based consensus (§15.6)
// ─────────────────────────────────────────────────────────────────────────────

/// Compute a Jaccard word-overlap similarity score ∈ [0, 1] between two
/// strings.
fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let a_words: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let b_words: std::collections::HashSet<&str> = b.split_whitespace().collect();
    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count();
    if union == 0 {
        return 1.0; // both empty → identical
    }
    intersection as f64 / union as f64
}

/// Choose the candidate that has the highest average word-overlap similarity
/// with all other candidates (§15.6 similarity consensus).
///
/// This is more robust than exact-string majority vote when different agents
/// paraphrase the same answer differently.
///
/// Returns `None` if `candidates` is empty.
///
/// ```
/// use vanswarm_orchestrator::patterns::similarity_vote;
/// let candidates = vec![
///     "The Eiffel Tower is in Paris, France",
///     "The Eiffel Tower is located in Paris",
///     "It's in Berlin",
/// ];
/// let winner = similarity_vote(&candidates).unwrap();
/// assert!(winner.contains("Paris"));
/// ```
pub fn similarity_vote<'a>(candidates: &'a [&'a str]) -> Option<&'a str> {
    if candidates.is_empty() {
        return None;
    }
    if candidates.len() == 1 {
        return Some(candidates[0]);
    }

    let mut best_idx = 0;
    let mut best_avg = -1.0_f64;

    for (i, &candidate) in candidates.iter().enumerate() {
        let avg_sim = candidates
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, &other)| jaccard_similarity(candidate, other))
            .sum::<f64>()
            / (candidates.len() - 1) as f64;

        if avg_sim > best_avg {
            best_avg = avg_sim;
            best_idx = i;
        }
    }

    Some(candidates[best_idx])
}

/// [`similarity_vote`] for owned `String` candidates.
pub fn similarity_vote_owned(candidates: &[String]) -> Option<String> {
    let refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
    similarity_vote(&refs).map(str::to_owned)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn majority_vote_clear_winner() {
        let c = vec!["A", "B", "A", "A", "B"];
        assert_eq!(majority_vote(&c), Some("A"));
    }

    #[test]
    fn majority_vote_single() {
        assert_eq!(majority_vote(&["only"]), Some("only"));
    }

    #[test]
    fn majority_vote_empty() {
        let empty: Vec<&str> = vec![];
        assert_eq!(majority_vote(&empty), None);
    }

    #[test]
    fn majority_vote_owned_delegates() {
        let c = vec!["X".to_string(), "Y".to_string(), "X".to_string()];
        assert_eq!(majority_vote_owned(&c).as_deref(), Some("X"));
    }

    #[test]
    fn similarity_vote_picks_consensus() {
        let c = vec![
            "The sky is blue and clear",
            "The sky is blue today",
            "Pineapple pizza is divisive",
        ];
        let winner = similarity_vote(&c).unwrap();
        assert!(winner.contains("sky") && winner.contains("blue"));
    }

    #[test]
    fn similarity_vote_empty() {
        let empty: Vec<&str> = vec![];
        assert_eq!(similarity_vote(&empty), None);
    }

    #[test]
    fn jaccard_identical() {
        assert!((jaccard_similarity("hello world", "hello world") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn jaccard_disjoint() {
        assert!((jaccard_similarity("foo bar", "baz qux") - 0.0).abs() < 1e-9);
    }
}
