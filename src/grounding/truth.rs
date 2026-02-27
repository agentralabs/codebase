//! Truth Maintenance — Invention 6.
//!
//! Track which claims have been invalidated by recent changes.
//! Codebase changes; AI's knowledge becomes stale.

use serde::{Deserialize, Serialize};

use crate::graph::CodeGraph;

use super::citation::{CitationEngine, GroundedClaim};

// ── Types ────────────────────────────────────────────────────────────────────

/// Record of a previously-true claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintainedTruth {
    /// The claim.
    pub claim: GroundedClaim,
    /// When it was established (unix timestamp).
    pub established_at: u64,
    /// Current status.
    pub status: TruthStatus,
    /// If invalidated, what changed.
    pub invalidation: Option<TruthInvalidation>,
}

/// Current status of a maintained truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TruthStatus {
    /// Still true.
    Valid,
    /// Changed, needs review.
    Stale,
    /// Definitely no longer true.
    Invalidated,
    /// Code was deleted.
    Deleted,
}

/// Details about how a truth was invalidated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruthInvalidation {
    /// When it was invalidated (unix timestamp).
    pub invalidated_at: u64,
    /// What change invalidated it.
    pub change: String,
    /// What's true now.
    pub new_truth: Option<String>,
}

// ── TruthMaintainer ──────────────────────────────────────────────────────────

/// Maintains a set of truths and checks them against the current graph.
pub struct TruthMaintainer<'g> {
    citation_engine: CitationEngine<'g>,
    truths: Vec<MaintainedTruth>,
}

impl<'g> TruthMaintainer<'g> {
    /// Create a new truth maintainer.
    pub fn new(graph: &'g CodeGraph) -> Self {
        Self {
            citation_engine: CitationEngine::new(graph),
            truths: Vec::new(),
        }
    }

    /// Register a truth that should be maintained.
    pub fn register_truth(&mut self, claim: &str) -> MaintainedTruth {
        let grounded = self.citation_engine.ground_claim(claim);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let truth = MaintainedTruth {
            claim: grounded,
            established_at: now,
            status: TruthStatus::Valid,
            invalidation: None,
        };

        self.truths.push(truth.clone());
        truth
    }

    /// Check if a historical claim is still true against the current graph.
    pub fn check_truth(&self, claim: &str) -> TruthStatus {
        let grounded = self.citation_engine.ground_claim(claim);
        if grounded.fully_grounded {
            TruthStatus::Valid
        } else if grounded.citations.is_empty() {
            TruthStatus::Deleted
        } else {
            TruthStatus::Stale
        }
    }

    /// Refresh all maintained truths against the current graph.
    pub fn refresh_all(&mut self) -> Vec<MaintainedTruth> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut results = Vec::new();

        for truth in &mut self.truths {
            let new_status = {
                let grounded = self.citation_engine.ground_claim(&truth.claim.claim);
                if grounded.fully_grounded {
                    TruthStatus::Valid
                } else if grounded.citations.is_empty() {
                    TruthStatus::Deleted
                } else {
                    TruthStatus::Stale
                }
            };

            if new_status != TruthStatus::Valid && truth.status == TruthStatus::Valid {
                truth.status = new_status;
                truth.invalidation = Some(TruthInvalidation {
                    invalidated_at: now,
                    change: "Graph changed since truth was established".to_string(),
                    new_truth: None,
                });
            }

            results.push(truth.clone());
        }

        results
    }

    /// Get a diff of what changed between two graph versions.
    /// Compares the current truth set status against the original.
    pub fn truth_diff(&self) -> Vec<MaintainedTruth> {
        self.truths
            .iter()
            .filter(|t| t.status != TruthStatus::Valid)
            .cloned()
            .collect()
    }

    /// Get all maintained truths.
    pub fn truths(&self) -> &[MaintainedTruth] {
        &self.truths
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CodeUnit, CodeUnitType, Language, Span};
    use std::path::PathBuf;

    fn test_graph() -> CodeGraph {
        let mut graph = CodeGraph::with_default_dimension();
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Python,
            "process_payment".to_string(),
            "payments.stripe.process_payment".to_string(),
            PathBuf::from("src/payments/stripe.py"),
            Span::new(10, 0, 30, 0),
        ));
        graph
    }

    #[test]
    fn register_truth_is_valid() {
        let graph = test_graph();
        let mut maintainer = TruthMaintainer::new(&graph);
        let truth = maintainer.register_truth("process_payment exists");
        assert_eq!(truth.status, TruthStatus::Valid);
    }

    #[test]
    fn check_truth_valid() {
        let graph = test_graph();
        let maintainer = TruthMaintainer::new(&graph);
        let status = maintainer.check_truth("process_payment exists in codebase");
        assert_eq!(status, TruthStatus::Valid);
    }

    #[test]
    fn check_truth_deleted() {
        let graph = test_graph();
        let maintainer = TruthMaintainer::new(&graph);
        let status = maintainer.check_truth("nonexistent_func does something");
        assert_eq!(status, TruthStatus::Deleted);
    }

    #[test]
    fn truth_diff_empty_when_valid() {
        let graph = test_graph();
        let mut maintainer = TruthMaintainer::new(&graph);
        maintainer.register_truth("process_payment exists");
        let diff = maintainer.truth_diff();
        assert!(diff.is_empty());
    }
}
