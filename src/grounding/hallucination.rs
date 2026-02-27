//! Hallucination Detector — Invention 5.
//!
//! Automatically detect when AI output contradicts the actual codebase.
//! Builds on the Citation Engine to classify ungrounded claims by type
//! and severity.

use serde::{Deserialize, Serialize};

use crate::graph::CodeGraph;

use super::citation::{Citation, CitationEngine, GroundedClaim};
use super::engine::extract_code_references;

// ── Types ────────────────────────────────────────────────────────────────────

/// Result of checking AI output for hallucinations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HallucinationCheck {
    /// The AI output that was checked.
    pub ai_output: String,
    /// Detected hallucinations.
    pub hallucinations: Vec<Hallucination>,
    /// Claims that were verified.
    pub verified_claims: Vec<GroundedClaim>,
    /// Overall hallucination score (0 = none, 1 = all hallucinated).
    pub hallucination_score: f64,
    /// Is this output safe to use?
    pub safe_to_use: bool,
}

/// A single detected hallucination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hallucination {
    /// The hallucinated claim.
    pub claim: String,
    /// Type of hallucination.
    pub hallucination_type: HallucinationType,
    /// What's actually true.
    pub reality: String,
    /// Evidence for reality.
    pub evidence: Vec<Citation>,
    /// Severity.
    pub severity: HallucinationSeverity,
}

/// Type of hallucination detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HallucinationType {
    /// Function/class doesn't exist.
    NonExistent,
    /// Exists but does something different.
    WrongBehavior,
    /// Wrong signature (params, return type).
    WrongSignature,
    /// Wrong location (different file/module).
    WrongLocation,
    /// Was true, no longer.
    Outdated,
    /// Invented feature.
    InventedFeature,
}

/// Severity of a hallucination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HallucinationSeverity {
    /// Minor inaccuracy.
    Minor,
    /// Would cause confusion.
    Moderate,
    /// Would cause errors.
    Severe,
    /// Would cause security/data issues.
    Critical,
}

// ── HallucinationDetector ────────────────────────────────────────────────────

/// Detector that finds hallucinations in AI output about code.
pub struct HallucinationDetector<'g> {
    citation_engine: CitationEngine<'g>,
    graph: &'g CodeGraph,
}

impl<'g> HallucinationDetector<'g> {
    /// Create a new detector backed by the given code graph.
    pub fn new(graph: &'g CodeGraph) -> Self {
        Self {
            citation_engine: CitationEngine::new(graph),
            graph,
        }
    }

    /// Check AI output for hallucinations.
    pub fn check_output(&self, ai_output: &str) -> HallucinationCheck {
        let sentences = split_into_claims(ai_output);
        let mut hallucinations = Vec::new();
        let mut verified_claims = Vec::new();
        let mut total_claims = 0usize;

        for sentence in &sentences {
            let refs = extract_code_references(sentence);
            if refs.is_empty() {
                continue; // Skip non-code sentences
            }
            total_claims += 1;

            let grounded = self.citation_engine.ground_claim(sentence);
            if grounded.fully_grounded {
                verified_claims.push(grounded);
            } else {
                // Classify the hallucination
                for reference in &refs {
                    if let Some(hallucination) = self.classify_hallucination(sentence, reference) {
                        hallucinations.push(hallucination);
                    }
                }
            }
        }

        let hallucination_score = if total_claims == 0 {
            0.0
        } else {
            hallucinations.len() as f64 / total_claims as f64
        };

        HallucinationCheck {
            ai_output: ai_output.to_string(),
            hallucinations,
            verified_claims,
            hallucination_score: hallucination_score.min(1.0),
            safe_to_use: hallucination_score < 0.3,
        }
    }

    /// Suggest fixes for detected hallucinations.
    pub fn suggest_fixes(&self, check: &HallucinationCheck) -> Vec<String> {
        let mut fixes = Vec::new();

        for h in &check.hallucinations {
            match h.hallucination_type {
                HallucinationType::NonExistent => {
                    // Try to find similar symbols
                    let refs = extract_code_references(&h.claim);
                    for r in &refs {
                        let similar = self.find_similar_names(r);
                        if !similar.is_empty() {
                            fixes.push(format!(
                                "Replace '{}' with one of: {}",
                                r,
                                similar.join(", ")
                            ));
                        } else {
                            fixes.push(format!("Remove reference to non-existent '{}'", r));
                        }
                    }
                }
                HallucinationType::WrongLocation => {
                    if !h.evidence.is_empty() {
                        fixes.push(format!(
                            "Correct location: actually in {}",
                            h.evidence[0].location.file
                        ));
                    }
                }
                HallucinationType::WrongSignature => {
                    if !h.evidence.is_empty() {
                        fixes.push(format!("Correct signature: {}", h.evidence[0].code_snippet));
                    }
                }
                _ => {
                    fixes.push(format!("Review claim: {}", h.claim));
                }
            }
        }

        fixes
    }

    // ── Internal ─────────────────────────────────────────────────────────

    fn classify_hallucination(&self, sentence: &str, reference: &str) -> Option<Hallucination> {
        // Check if the reference exists at all
        let mut found = false;
        let mut found_unit = None;
        for unit in self.graph.units() {
            if unit.name == reference {
                found = true;
                found_unit = Some(unit);
                break;
            }
        }

        if !found {
            // Check case-insensitive
            let lower = reference.to_lowercase();
            for unit in self.graph.units() {
                if unit.name.to_lowercase() == lower {
                    found = true;
                    found_unit = Some(unit);
                    break;
                }
            }
        }

        if !found {
            // Symbol doesn't exist at all
            return Some(Hallucination {
                claim: sentence.to_string(),
                hallucination_type: HallucinationType::NonExistent,
                reality: format!("No symbol '{}' exists in the codebase", reference),
                evidence: Vec::new(),
                severity: HallucinationSeverity::Severe,
            });
        }

        // Symbol exists — check if the claim about it is wrong
        if let Some(unit) = found_unit {
            let sentence_lower = sentence.to_lowercase();

            // Check for wrong location claims
            if sentence_lower.contains("in ") || sentence_lower.contains("file") {
                let file_str = unit.file_path.display().to_string();
                // If the sentence mentions a file path that doesn't match
                let words: Vec<&str> = sentence.split_whitespace().collect();
                for word in &words {
                    let w = word.trim_matches(|c: char| {
                        !c.is_alphanumeric() && c != '/' && c != '.' && c != '_'
                    });
                    if w.contains('/') && w.contains('.') && !file_str.contains(w) {
                        let citation = self.citation_engine.cite_node(unit.id);
                        return Some(Hallucination {
                            claim: sentence.to_string(),
                            hallucination_type: HallucinationType::WrongLocation,
                            reality: format!("'{}' is in {}", reference, file_str),
                            evidence: citation.into_iter().collect(),
                            severity: HallucinationSeverity::Moderate,
                        });
                    }
                }
            }
        }

        None
    }

    fn find_similar_names(&self, name: &str) -> Vec<String> {
        let lower = name.to_lowercase();
        let mut results: Vec<(String, usize)> = Vec::new();

        for unit in self.graph.units() {
            let u_lower = unit.name.to_lowercase();
            if u_lower.starts_with(&lower) || lower.starts_with(&u_lower) {
                if !results.iter().any(|(n, _)| *n == unit.name) {
                    results.push((unit.name.clone(), 0));
                }
            }
        }

        results.sort_by_key(|(_, d)| *d);
        results.into_iter().take(5).map(|(n, _)| n).collect()
    }
}

/// Split text into individual claim sentences.
fn split_into_claims(text: &str) -> Vec<String> {
    text.split(|c: char| c == '.' || c == '\n')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.len() > 5)
        .collect()
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
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Type,
            Language::Rust,
            "CodeGraph".to_string(),
            "crate::graph::CodeGraph".to_string(),
            PathBuf::from("src/graph/code_graph.rs"),
            Span::new(17, 0, 250, 0),
        ));
        graph
    }

    #[test]
    fn detect_nonexistent_hallucination() {
        let graph = test_graph();
        let detector = HallucinationDetector::new(&graph);
        let check = detector.check_output("The send_invoice function handles billing");
        assert!(!check.hallucinations.is_empty());
        assert_eq!(
            check.hallucinations[0].hallucination_type,
            HallucinationType::NonExistent
        );
    }

    #[test]
    fn verified_output_is_safe() {
        let graph = test_graph();
        let detector = HallucinationDetector::new(&graph);
        let check = detector.check_output("The process_payment function exists in the codebase");
        assert!(check.safe_to_use);
    }

    #[test]
    fn suggest_fixes_for_nonexistent() {
        let graph = test_graph();
        let detector = HallucinationDetector::new(&graph);
        let check = detector.check_output("The process_paymnt function works");
        let fixes = detector.suggest_fixes(&check);
        // Should suggest the correct name
        assert!(!fixes.is_empty());
    }

    #[test]
    fn hallucination_score_range() {
        let graph = test_graph();
        let detector = HallucinationDetector::new(&graph);
        let check = detector.check_output("Normal text without code references");
        assert!(check.hallucination_score >= 0.0 && check.hallucination_score <= 1.0);
    }

    #[test]
    fn split_claims_works() {
        let claims = split_into_claims("First claim. Second claim.\nThird claim");
        assert_eq!(claims.len(), 3);
    }
}
