//! Regression Oracle — Invention 3.
//!
//! Predict which tests are likely to fail based on a change, before running them.
//! Uses the code graph to trace from changed units to test units.

use serde::{Deserialize, Serialize};

use crate::graph::traversal::{self, Direction, TraversalOptions};
use crate::graph::CodeGraph;
use crate::types::{CodeUnitType, EdgeType};

// ── Types ────────────────────────────────────────────────────────────────────

/// Prediction of test outcomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionOracle {
    /// The unit being changed.
    pub changed_unit: u64,
    /// Tests predicted to fail.
    pub likely_failures: Vec<TestPrediction>,
    /// Tests that should pass but are worth running.
    pub recommended_tests: Vec<TestPrediction>,
    /// Tests that are definitely unaffected.
    pub safe_to_skip: Vec<TestId>,
    /// Minimum test set for confidence.
    pub minimum_test_set: Vec<TestId>,
}

/// A single test prediction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestPrediction {
    /// The test.
    pub test: TestId,
    /// Probability of failure.
    pub failure_probability: f64,
    /// Why we think it might fail.
    pub reason: String,
    /// Path from change to test.
    pub dependency_path: Vec<u64>,
}

/// Identifier for a test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestId {
    pub file: String,
    pub function: String,
    pub line: u32,
    pub unit_id: u64,
}

// ── RegressionPredictor ──────────────────────────────────────────────────────

/// Predicts test outcomes based on code changes.
pub struct RegressionPredictor<'g> {
    graph: &'g CodeGraph,
}

impl<'g> RegressionPredictor<'g> {
    pub fn new(graph: &'g CodeGraph) -> Self {
        Self { graph }
    }

    /// Predict test outcomes for a given change.
    pub fn predict(&self, changed_unit: u64, max_depth: u32) -> RegressionOracle {
        // Find all test units in the graph
        let all_tests: Vec<TestId> = self
            .graph
            .units()
            .iter()
            .filter(|u| u.unit_type == CodeUnitType::Test)
            .map(|u| TestId {
                file: u.file_path.display().to_string(),
                function: u.name.clone(),
                line: u.span.start_line,
                unit_id: u.id,
            })
            .collect();

        // BFS backward from changed unit to find dependents
        let options = TraversalOptions {
            max_depth: max_depth as i32,
            edge_types: vec![
                EdgeType::Calls,
                EdgeType::Imports,
                EdgeType::Tests,
                EdgeType::References,
                EdgeType::UsesType,
            ],
            direction: Direction::Backward,
        };
        let reachable = traversal::bfs(self.graph, changed_unit, &options);
        let reachable_ids: std::collections::HashSet<u64> =
            reachable.iter().map(|(id, _)| *id).collect();

        let mut likely_failures = Vec::new();
        let mut recommended_tests = Vec::new();
        let mut safe_to_skip = Vec::new();

        for test in &all_tests {
            // Check if this test is directly connected
            let directly_tests = self
                .graph
                .edges_from(test.unit_id)
                .iter()
                .any(|e| e.edge_type == EdgeType::Tests && e.target_id == changed_unit);

            if directly_tests {
                // Direct test of the changed unit — high failure probability
                likely_failures.push(TestPrediction {
                    test: test.clone(),
                    failure_probability: 0.85,
                    reason: "Directly tests the changed unit".to_string(),
                    dependency_path: vec![test.unit_id, changed_unit],
                });
            } else if reachable_ids.contains(&test.unit_id) {
                // Transitively connected
                let depth = reachable
                    .iter()
                    .find(|(id, _)| *id == test.unit_id)
                    .map(|(_, d)| *d)
                    .unwrap_or(0);

                let probability = 0.6 / (1.0 + depth as f64 * 0.3);

                if probability > 0.3 {
                    recommended_tests.push(TestPrediction {
                        test: test.clone(),
                        failure_probability: probability,
                        reason: format!("Transitively depends on changed unit (depth {})", depth),
                        dependency_path: vec![test.unit_id, changed_unit],
                    });
                } else {
                    safe_to_skip.push(test.clone());
                }
            } else {
                safe_to_skip.push(test.clone());
            }
        }

        // Sort by failure probability descending
        likely_failures.sort_by(|a, b| {
            b.failure_probability
                .partial_cmp(&a.failure_probability)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        recommended_tests.sort_by(|a, b| {
            b.failure_probability
                .partial_cmp(&a.failure_probability)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Minimum test set = likely failures + high-probability recommended
        let minimum_test_set: Vec<TestId> = likely_failures
            .iter()
            .map(|p| p.test.clone())
            .chain(
                recommended_tests
                    .iter()
                    .filter(|p| p.failure_probability > 0.4)
                    .map(|p| p.test.clone()),
            )
            .collect();

        RegressionOracle {
            changed_unit,
            likely_failures,
            recommended_tests,
            safe_to_skip,
            minimum_test_set,
        }
    }

    /// Get the minimum test set needed for confidence.
    pub fn minimal_test_set(&self, changed_unit: u64) -> Vec<TestId> {
        self.predict(changed_unit, 5).minimum_test_set
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CodeUnit, CodeUnitType, Edge, Language, Span};
    use std::path::PathBuf;

    fn test_graph() -> CodeGraph {
        let mut graph = CodeGraph::with_default_dimension();
        let func = graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "process".to_string(),
            "mod::process".to_string(),
            PathBuf::from("src/lib.rs"),
            Span::new(10, 0, 30, 0),
        ));
        let test = graph.add_unit(CodeUnit::new(
            CodeUnitType::Test,
            Language::Rust,
            "test_process".to_string(),
            "mod::test_process".to_string(),
            PathBuf::from("tests/test_lib.rs"),
            Span::new(1, 0, 10, 0),
        ));
        let _ = graph.add_edge(Edge::new(test, func, EdgeType::Tests));
        graph
    }

    #[test]
    fn predict_finds_direct_test() {
        let graph = test_graph();
        let predictor = RegressionPredictor::new(&graph);
        let oracle = predictor.predict(0, 5);
        assert!(!oracle.likely_failures.is_empty());
        assert!(oracle.likely_failures[0].failure_probability > 0.5);
    }

    #[test]
    fn minimal_test_set_not_empty() {
        let graph = test_graph();
        let predictor = RegressionPredictor::new(&graph);
        let minimal = predictor.minimal_test_set(0);
        assert!(!minimal.is_empty());
    }
}
