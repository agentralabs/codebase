//! Enhanced Code Prophecy — Invention 2.
//!
//! Simulate the future state of the codebase based on current trajectory
//! and proposed changes. "Should I refactor this?" answered with data.

use serde::{Deserialize, Serialize};

use crate::graph::CodeGraph;
use crate::types::CodeUnitType;

// ── Types ────────────────────────────────────────────────────────────────────

/// A prophecy about code evolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeProphecy {
    /// What we're prophesying about.
    pub subject: ProphecySubject,
    /// Time horizon.
    pub horizon: ProphecyHorizon,
    /// Predicted outcomes.
    pub predictions: Vec<EnhancedPrediction>,
    /// Confidence in prophecy.
    pub confidence: f64,
    /// Evidence supporting prophecy.
    pub evidence: Vec<ProphecyEvidence>,
}

/// What the prophecy is about.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProphecySubject {
    /// Specific function/class.
    Node(u64),
    /// Entire module.
    Module(String),
    /// Architectural pattern.
    Pattern(String),
}

/// Time horizon for predictions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProphecyHorizon {
    /// Next few changes.
    Immediate,
    /// Next sprint/week.
    ShortTerm,
    /// Next month.
    MediumTerm,
    /// Next quarter.
    LongTerm,
}

/// A single prediction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedPrediction {
    /// What will happen.
    pub outcome: String,
    /// Probability (0.0 - 1.0).
    pub probability: f64,
    /// Is this good or bad?
    pub sentiment: Sentiment,
    /// What triggers this outcome.
    pub trigger: String,
}

/// Sentiment of a prediction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sentiment {
    Positive,
    Neutral,
    Negative,
    Critical,
}

/// Evidence supporting a prophecy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProphecyEvidence {
    /// Type of evidence.
    pub evidence_type: EvidenceType,
    /// The evidence.
    pub description: String,
    /// Weight in prediction.
    pub weight: f64,
}

/// Type of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceType {
    /// Historical pattern in this codebase.
    Historical,
    /// Structural analysis.
    Structural,
    /// Complexity metrics.
    Complexity,
    /// Dependency analysis.
    Dependency,
    /// Industry pattern.
    IndustryPattern,
}

// ── EnhancedProphecyEngine ───────────────────────────────────────────────────

/// Enhanced prophecy engine with evidence-backed predictions.
pub struct EnhancedProphecyEngine<'g> {
    graph: &'g CodeGraph,
}

impl<'g> EnhancedProphecyEngine<'g> {
    pub fn new(graph: &'g CodeGraph) -> Self {
        Self { graph }
    }

    /// Generate a prophecy for a given subject and horizon.
    pub fn prophecy(&self, subject: ProphecySubject, horizon: ProphecyHorizon) -> CodeProphecy {
        let (predictions, evidence) = match &subject {
            ProphecySubject::Node(id) => self.prophesy_node(*id, horizon),
            ProphecySubject::Module(name) => self.prophesy_module(name, horizon),
            ProphecySubject::Pattern(name) => self.prophesy_pattern(name, horizon),
        };

        let confidence = if evidence.is_empty() {
            0.3
        } else {
            let avg_weight: f64 =
                evidence.iter().map(|e| e.weight).sum::<f64>() / evidence.len() as f64;
            avg_weight.min(1.0)
        };

        CodeProphecy {
            subject,
            horizon,
            predictions,
            confidence,
            evidence,
        }
    }

    /// "What if" scenario analysis.
    pub fn prophecy_if(
        &self,
        subject: ProphecySubject,
        scenario: &str,
        horizon: ProphecyHorizon,
    ) -> CodeProphecy {
        let mut prophecy = self.prophecy(subject, horizon);

        // Add scenario-specific predictions
        prophecy.predictions.push(EnhancedPrediction {
            outcome: format!("If {}: additional changes likely needed", scenario),
            probability: 0.6,
            sentiment: Sentiment::Neutral,
            trigger: scenario.to_string(),
        });

        prophecy
    }

    /// Compare prophecies of different approaches.
    pub fn prophecy_compare(
        &self,
        subject_a: ProphecySubject,
        subject_b: ProphecySubject,
        horizon: ProphecyHorizon,
    ) -> (CodeProphecy, CodeProphecy) {
        let a = self.prophecy(subject_a, horizon);
        let b = self.prophecy(subject_b, horizon);
        (a, b)
    }

    // ── Internal ─────────────────────────────────────────────────────────

    fn prophesy_node(
        &self,
        id: u64,
        _horizon: ProphecyHorizon,
    ) -> (Vec<EnhancedPrediction>, Vec<ProphecyEvidence>) {
        let mut predictions = Vec::new();
        let mut evidence = Vec::new();

        if let Some(unit) = self.graph.get_unit(id) {
            // Complexity analysis
            if unit.complexity > 15 {
                predictions.push(EnhancedPrediction {
                    outcome: "High risk of bugs due to complexity".to_string(),
                    probability: 0.7,
                    sentiment: Sentiment::Negative,
                    trigger: format!("Cyclomatic complexity: {}", unit.complexity),
                });
                evidence.push(ProphecyEvidence {
                    evidence_type: EvidenceType::Complexity,
                    description: format!("Complexity score: {} (threshold: 15)", unit.complexity),
                    weight: 0.8,
                });
            }

            // Change frequency analysis
            if unit.change_count > 10 {
                predictions.push(EnhancedPrediction {
                    outcome: "Frequently modified — likely needs refactoring".to_string(),
                    probability: 0.6,
                    sentiment: Sentiment::Negative,
                    trigger: format!("{} changes recorded", unit.change_count),
                });
                evidence.push(ProphecyEvidence {
                    evidence_type: EvidenceType::Historical,
                    description: format!("Changed {} times", unit.change_count),
                    weight: 0.7,
                });
            }

            // Stability analysis
            if unit.stability_score < 0.3 {
                predictions.push(EnhancedPrediction {
                    outcome: "Unstable code — expect more changes".to_string(),
                    probability: 0.8,
                    sentiment: Sentiment::Negative,
                    trigger: format!("Stability score: {:.2}", unit.stability_score),
                });
                evidence.push(ProphecyEvidence {
                    evidence_type: EvidenceType::Structural,
                    description: format!("Stability score: {:.2}", unit.stability_score),
                    weight: 0.8,
                });
            }

            // Dependency analysis
            let incoming = self.graph.edges_to(id).len();
            let outgoing = self.graph.edges_from(id).len();
            if incoming > 10 {
                predictions.push(EnhancedPrediction {
                    outcome: "High coupling — changes here affect many dependents".to_string(),
                    probability: 0.75,
                    sentiment: Sentiment::Critical,
                    trigger: format!("{} incoming dependencies", incoming),
                });
                evidence.push(ProphecyEvidence {
                    evidence_type: EvidenceType::Dependency,
                    description: format!("{} dependents, {} dependencies", incoming, outgoing),
                    weight: 0.9,
                });
            }

            // Default positive prediction if nothing concerning
            if predictions.is_empty() {
                predictions.push(EnhancedPrediction {
                    outcome: "Code appears stable with manageable complexity".to_string(),
                    probability: 0.7,
                    sentiment: Sentiment::Positive,
                    trigger: "No risk factors detected".to_string(),
                });
            }
        }

        (predictions, evidence)
    }

    fn prophesy_module(
        &self,
        module_name: &str,
        _horizon: ProphecyHorizon,
    ) -> (Vec<EnhancedPrediction>, Vec<ProphecyEvidence>) {
        let mut predictions = Vec::new();
        let mut evidence = Vec::new();

        // Find units in this module
        let module_units: Vec<_> = self
            .graph
            .units()
            .iter()
            .filter(|u| u.qualified_name.starts_with(module_name))
            .collect();

        if module_units.is_empty() {
            predictions.push(EnhancedPrediction {
                outcome: format!("Module '{}' not found in codebase", module_name),
                probability: 1.0,
                sentiment: Sentiment::Neutral,
                trigger: "Module not indexed".to_string(),
            });
            return (predictions, evidence);
        }

        let avg_complexity: f64 = module_units
            .iter()
            .map(|u| u.complexity as f64)
            .sum::<f64>()
            / module_units.len() as f64;
        let total_changes: u32 = module_units.iter().map(|u| u.change_count).sum();
        let function_count = module_units
            .iter()
            .filter(|u| u.unit_type == CodeUnitType::Function)
            .count();

        evidence.push(ProphecyEvidence {
            evidence_type: EvidenceType::Structural,
            description: format!(
                "{} units, {} functions, avg complexity: {:.1}",
                module_units.len(),
                function_count,
                avg_complexity
            ),
            weight: 0.7,
        });

        if avg_complexity > 10.0 {
            predictions.push(EnhancedPrediction {
                outcome: "Module complexity is growing — consider refactoring".to_string(),
                probability: 0.65,
                sentiment: Sentiment::Negative,
                trigger: format!("Average complexity: {:.1}", avg_complexity),
            });
        }

        if total_changes > 50 {
            predictions.push(EnhancedPrediction {
                outcome: "Hotspot module — high change velocity".to_string(),
                probability: 0.7,
                sentiment: Sentiment::Negative,
                trigger: format!("{} total changes across module", total_changes),
            });
        }

        if predictions.is_empty() {
            predictions.push(EnhancedPrediction {
                outcome: "Module appears healthy".to_string(),
                probability: 0.7,
                sentiment: Sentiment::Positive,
                trigger: "No risk factors detected".to_string(),
            });
        }

        (predictions, evidence)
    }

    fn prophesy_pattern(
        &self,
        _pattern_name: &str,
        _horizon: ProphecyHorizon,
    ) -> (Vec<EnhancedPrediction>, Vec<ProphecyEvidence>) {
        // Pattern-level prophecy is heuristic-based
        let predictions = vec![EnhancedPrediction {
            outcome: "Pattern analysis requires more data points".to_string(),
            probability: 0.5,
            sentiment: Sentiment::Neutral,
            trigger: "Insufficient pattern data".to_string(),
        }];
        let evidence = vec![ProphecyEvidence {
            evidence_type: EvidenceType::IndustryPattern,
            description: "Pattern-level predictions require historical commit data".to_string(),
            weight: 0.3,
        }];
        (predictions, evidence)
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
        let mut unit = CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "complex_func".to_string(),
            "mod::complex_func".to_string(),
            PathBuf::from("src/complex.rs"),
            Span::new(1, 0, 100, 0),
        );
        unit.complexity = 25;
        unit.change_count = 15;
        unit.stability_score = 0.2;
        graph.add_unit(unit);
        graph
    }

    #[test]
    fn prophecy_detects_complexity() {
        let graph = test_graph();
        let engine = EnhancedProphecyEngine::new(&graph);
        let prophecy = engine.prophecy(ProphecySubject::Node(0), ProphecyHorizon::ShortTerm);
        assert!(!prophecy.predictions.is_empty());
        assert!(prophecy
            .predictions
            .iter()
            .any(|p| p.sentiment == Sentiment::Negative));
    }

    #[test]
    fn prophecy_has_evidence() {
        let graph = test_graph();
        let engine = EnhancedProphecyEngine::new(&graph);
        let prophecy = engine.prophecy(ProphecySubject::Node(0), ProphecyHorizon::MediumTerm);
        assert!(!prophecy.evidence.is_empty());
    }

    #[test]
    fn prophecy_compare_returns_pair() {
        let graph = test_graph();
        let engine = EnhancedProphecyEngine::new(&graph);
        let (a, b) = engine.prophecy_compare(
            ProphecySubject::Node(0),
            ProphecySubject::Module("mod".to_string()),
            ProphecyHorizon::LongTerm,
        );
        assert!(!a.predictions.is_empty());
        assert!(!b.predictions.is_empty());
    }
}
