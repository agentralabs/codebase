//! Citation Engine — Invention 4.
//!
//! Every claim about code MUST be backed by a citation to the actual graph node.
//! Transforms grounding from binary (exists / doesn't exist) into rich evidence
//! with source locations, code snippets, and citation strength.

use serde::{Deserialize, Serialize};

use crate::graph::CodeGraph;
use crate::types::CodeUnit;

use super::engine::extract_code_references;

// ── Types ────────────────────────────────────────────────────────────────────

/// A grounded claim about code with full citations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundedClaim {
    /// The claim being made.
    pub claim: String,
    /// Citations proving the claim.
    pub citations: Vec<Citation>,
    /// Confidence based on citation strength.
    pub confidence: f64,
    /// Is this claim fully grounded?
    pub fully_grounded: bool,
}

/// A citation to a specific code node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    /// The node being cited.
    pub node_id: u64,
    /// Specific location in source.
    pub location: CodeLocation,
    /// The actual code being cited (signature or name).
    pub code_snippet: String,
    /// How this supports the claim.
    pub relevance: String,
    /// Strength of evidence.
    pub strength: CitationStrength,
}

/// Precise location in source code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeLocation {
    pub file: String,
    pub start_line: u32,
    pub end_line: u32,
    pub start_col: u32,
    pub end_col: u32,
}

/// Strength of a citation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CitationStrength {
    /// Directly proves the claim.
    Direct,
    /// Strongly supports the claim.
    Strong,
    /// Partially supports.
    Partial,
    /// Weak/circumstantial.
    Weak,
}

/// A claim that couldn't be grounded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UngroundedClaim {
    /// The claim attempted.
    pub claim: String,
    /// Why it couldn't be grounded.
    pub reason: UngroundedReason,
    /// What would be needed to ground it.
    pub requirements: Vec<String>,
}

/// Why a claim couldn't be grounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UngroundedReason {
    /// No matching code found.
    NotFound,
    /// Code found but doesn't support claim.
    Contradicted,
    /// Ambiguous (multiple interpretations).
    Ambiguous,
    /// Outside indexed scope.
    OutOfScope,
}

// ── CitationEngine ───────────────────────────────────────────────────────────

/// Engine that produces rich citations for code claims.
pub struct CitationEngine<'g> {
    graph: &'g CodeGraph,
}

impl<'g> CitationEngine<'g> {
    /// Create a new citation engine backed by the given code graph.
    pub fn new(graph: &'g CodeGraph) -> Self {
        Self { graph }
    }

    /// Ground a natural-language claim with full citations.
    pub fn ground_claim(&self, claim: &str) -> GroundedClaim {
        let refs = extract_code_references(claim);

        if refs.is_empty() {
            return GroundedClaim {
                claim: claim.to_string(),
                citations: Vec::new(),
                confidence: 0.0,
                fully_grounded: false,
            };
        }

        let mut citations = Vec::new();
        let mut matched = 0usize;

        for reference in &refs {
            let found = self.find_citations(reference);
            if !found.is_empty() {
                matched += 1;
                citations.extend(found);
            }
        }

        let confidence = if refs.is_empty() {
            0.0
        } else {
            matched as f64 / refs.len() as f64
        };

        GroundedClaim {
            claim: claim.to_string(),
            fully_grounded: matched == refs.len(),
            confidence,
            citations,
        }
    }

    /// Build a citation for a specific node by ID.
    pub fn cite_node(&self, unit_id: u64) -> Option<Citation> {
        let unit = self.graph.get_unit(unit_id)?;
        Some(self.citation_from_unit(unit, "direct reference", CitationStrength::Direct))
    }

    /// Verify if a specific claim is true (simpler API).
    pub fn verify_claim(&self, claim: &str) -> bool {
        let grounded = self.ground_claim(claim);
        grounded.fully_grounded
    }

    /// Find claims in text that contradict the codebase.
    pub fn find_contradictions(&self, claim: &str) -> Vec<UngroundedClaim> {
        let refs = extract_code_references(claim);
        let mut contradictions = Vec::new();

        for reference in &refs {
            // Check if reference exists at all
            let exact = self.find_exact(reference);
            if exact.is_empty() {
                // Check if there's something similar (possible wrong name)
                let similar = self.find_similar(reference);
                let reason = if similar.is_empty() {
                    UngroundedReason::NotFound
                } else {
                    UngroundedReason::Contradicted
                };

                let mut requirements = Vec::new();
                if !similar.is_empty() {
                    requirements.push(format!(
                        "Did you mean: {}?",
                        similar
                            .iter()
                            .map(|u| u.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                } else {
                    requirements.push(format!("No symbol '{}' found in codebase", reference));
                }

                contradictions.push(UngroundedClaim {
                    claim: format!("Reference to '{}'", reference),
                    reason,
                    requirements,
                });
            }
        }

        contradictions
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    fn find_citations(&self, name: &str) -> Vec<Citation> {
        let mut results = Vec::new();

        // 1. Exact match — Direct strength
        for unit in self.graph.units() {
            if unit.name == name {
                results.push(self.citation_from_unit(
                    unit,
                    "exact name match",
                    CitationStrength::Direct,
                ));
            }
        }
        if !results.is_empty() {
            return results;
        }

        // 2. Qualified name contains — Strong strength
        for unit in self.graph.units() {
            if unit.qualified_name.contains(name) {
                results.push(self.citation_from_unit(
                    unit,
                    "qualified name match",
                    CitationStrength::Strong,
                ));
            }
        }
        if !results.is_empty() {
            return results;
        }

        // 3. Case-insensitive — Partial strength
        let lower = name.to_lowercase();
        for unit in self.graph.units() {
            if unit.name.to_lowercase() == lower {
                results.push(self.citation_from_unit(
                    unit,
                    "case-insensitive match",
                    CitationStrength::Partial,
                ));
            }
        }

        results
    }

    fn find_exact(&self, name: &str) -> Vec<&CodeUnit> {
        self.graph
            .units()
            .iter()
            .filter(|u| u.name == name)
            .collect()
    }

    fn find_similar(&self, name: &str) -> Vec<&CodeUnit> {
        let lower = name.to_lowercase();
        self.graph
            .units()
            .iter()
            .filter(|u| {
                let u_lower = u.name.to_lowercase();
                u_lower.starts_with(&lower)
                    || lower.starts_with(&u_lower)
                    || levenshtein_distance(&lower, &u_lower) <= name.len() / 3
            })
            .collect()
    }

    fn citation_from_unit(
        &self,
        unit: &CodeUnit,
        relevance: &str,
        strength: CitationStrength,
    ) -> Citation {
        Citation {
            node_id: unit.id,
            location: CodeLocation {
                file: unit.file_path.display().to_string(),
                start_line: unit.span.start_line,
                end_line: unit.span.end_line,
                start_col: unit.span.start_col,
                end_col: unit.span.end_col,
            },
            code_snippet: unit.signature.clone().unwrap_or_else(|| unit.name.clone()),
            relevance: relevance.to_string(),
            strength,
        }
    }
}

/// Simple Levenshtein distance for internal use.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
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
    fn ground_claim_verified() {
        let graph = test_graph();
        let engine = CitationEngine::new(&graph);
        let result = engine.ground_claim("The process_payment function exists");
        assert!(result.fully_grounded);
        assert!(!result.citations.is_empty());
        assert_eq!(result.citations[0].strength, CitationStrength::Direct);
    }

    #[test]
    fn ground_claim_ungrounded() {
        let graph = test_graph();
        let engine = CitationEngine::new(&graph);
        let result = engine.ground_claim("The send_invoice function sends emails");
        assert!(!result.fully_grounded);
        assert!(result.confidence < 1.0);
    }

    #[test]
    fn cite_node_returns_citation() {
        let graph = test_graph();
        let engine = CitationEngine::new(&graph);
        // unit IDs are assigned sequentially starting from 0
        let cite = engine.cite_node(0);
        assert!(cite.is_some());
        let c = cite.unwrap();
        assert_eq!(c.strength, CitationStrength::Direct);
    }

    #[test]
    fn find_contradictions_detects_missing() {
        let graph = test_graph();
        let engine = CitationEngine::new(&graph);
        let contradictions = engine.find_contradictions("The nonexistent_function does things");
        assert!(!contradictions.is_empty());
        assert_eq!(contradictions[0].reason, UngroundedReason::NotFound);
    }

    #[test]
    fn verify_claim_true() {
        let graph = test_graph();
        let engine = CitationEngine::new(&graph);
        assert!(engine.verify_claim("process_payment exists in the codebase"));
    }

    #[test]
    fn verify_claim_false() {
        let graph = test_graph();
        let engine = CitationEngine::new(&graph);
        assert!(!engine.verify_claim("The missing_func handles errors"));
    }
}
