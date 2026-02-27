//! Concept Navigation — Invention 7.
//!
//! Navigate by CONCEPT, not by filename or keyword. "Where is authentication
//! handled?" finds the answer without guessing filenames.

use serde::{Deserialize, Serialize};

use crate::graph::CodeGraph;
use crate::types::CodeUnitType;

// ── Types ────────────────────────────────────────────────────────────────────

/// A semantic concept in the codebase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeConcept {
    /// Concept name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Nodes that implement this concept.
    pub implementations: Vec<ConceptImplementation>,
    /// Related concepts.
    pub related: Vec<String>,
    /// Confidence this concept exists.
    pub confidence: f64,
}

/// A node implementing a concept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptImplementation {
    /// The implementing node.
    pub node_id: u64,
    /// Node name.
    pub name: String,
    /// File path.
    pub file_path: String,
    /// How strongly it implements the concept.
    pub strength: f64,
    /// What aspect it implements.
    pub aspect: String,
}

/// Query for navigating to a concept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptQuery {
    /// Natural language description.
    pub description: String,
    /// Optional constraints.
    pub constraints: Vec<ConceptConstraint>,
}

/// Constraint on a concept query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConceptConstraint {
    /// Must be in specific module.
    InModule(String),
    /// Must be specific type.
    OfType(String),
    /// Must have specific pattern.
    HasPattern(String),
}

// ── Built-in concept definitions ─────────────────────────────────────────────

struct ConceptDef {
    name: &'static str,
    description: &'static str,
    keywords: &'static [&'static str],
    related: &'static [&'static str],
}

const CONCEPTS: &[ConceptDef] = &[
    ConceptDef {
        name: "Authentication",
        description: "User identity verification and access control",
        keywords: &[
            "auth",
            "login",
            "logout",
            "token",
            "jwt",
            "oauth",
            "password",
            "session",
            "credential",
        ],
        related: &["Authorization", "User Management", "Security"],
    },
    ConceptDef {
        name: "Payment",
        description: "Financial transaction processing",
        keywords: &[
            "payment", "charge", "stripe", "paypal", "billing", "checkout", "invoice", "refund",
        ],
        related: &["User Management", "Configuration"],
    },
    ConceptDef {
        name: "User Management",
        description: "User accounts, profiles, and registration",
        keywords: &[
            "user",
            "account",
            "profile",
            "registration",
            "signup",
            "onboard",
        ],
        related: &["Authentication", "Database"],
    },
    ConceptDef {
        name: "Database",
        description: "Data persistence and querying",
        keywords: &[
            "database",
            "db",
            "query",
            "sql",
            "migration",
            "schema",
            "repository",
            "orm",
            "model",
        ],
        related: &["Caching", "Configuration"],
    },
    ConceptDef {
        name: "API",
        description: "External interface endpoints",
        keywords: &[
            "api",
            "endpoint",
            "route",
            "handler",
            "controller",
            "rest",
            "graphql",
            "grpc",
        ],
        related: &["Authentication", "Error Handling"],
    },
    ConceptDef {
        name: "Logging",
        description: "Application logging and telemetry",
        keywords: &[
            "log",
            "logger",
            "trace",
            "debug",
            "warn",
            "error",
            "metric",
            "telemetry",
            "monitor",
        ],
        related: &["Error Handling", "Configuration"],
    },
    ConceptDef {
        name: "Configuration",
        description: "Application configuration and settings",
        keywords: &[
            "config",
            "setting",
            "env",
            "environment",
            "option",
            "feature_flag",
        ],
        related: &["Logging"],
    },
    ConceptDef {
        name: "Testing",
        description: "Test infrastructure and utilities",
        keywords: &[
            "test",
            "mock",
            "stub",
            "fixture",
            "assert",
            "spec",
            "bench",
            "integration_test",
        ],
        related: &["Error Handling"],
    },
    ConceptDef {
        name: "Error Handling",
        description: "Error management and recovery",
        keywords: &[
            "error",
            "exception",
            "retry",
            "fallback",
            "panic",
            "throw",
            "recover",
            "result",
        ],
        related: &["Logging", "API"],
    },
    ConceptDef {
        name: "Caching",
        description: "Data caching for performance",
        keywords: &[
            "cache",
            "memoize",
            "lru",
            "ttl",
            "redis",
            "memcached",
            "invalidate",
        ],
        related: &["Database", "Configuration"],
    },
    ConceptDef {
        name: "Rate Limiting",
        description: "Request throttling and rate control",
        keywords: &["rate_limit", "throttle", "backoff", "quota", "bucket"],
        related: &["Authentication", "API"],
    },
    ConceptDef {
        name: "Security",
        description: "Security measures and vulnerability protection",
        keywords: &[
            "security", "encrypt", "decrypt", "hash", "sanitize", "xss", "csrf", "cors",
        ],
        related: &["Authentication", "API"],
    },
];

// ── ConceptNavigator ─────────────────────────────────────────────────────────

/// Navigate code by semantic concepts.
pub struct ConceptNavigator<'g> {
    graph: &'g CodeGraph,
}

impl<'g> ConceptNavigator<'g> {
    pub fn new(graph: &'g CodeGraph) -> Self {
        Self { graph }
    }

    /// Find code implementing a concept.
    pub fn find_concept(&self, query: ConceptQuery) -> Vec<CodeConcept> {
        let query_lower = query.description.to_lowercase();
        let mut results = Vec::new();

        for def in CONCEPTS {
            let name_match = def.name.to_lowercase().contains(&query_lower)
                || query_lower.contains(&def.name.to_lowercase());
            let keyword_match = def.keywords.iter().any(|k| query_lower.contains(k));

            if name_match || keyword_match {
                let concept = self.build_concept(def, &query.constraints);
                if !concept.implementations.is_empty() {
                    results.push(concept);
                }
            }
        }

        // Sort by confidence descending
        results.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// Map all concepts in the codebase.
    pub fn map_all_concepts(&self) -> Vec<CodeConcept> {
        let mut results = Vec::new();
        for def in CONCEPTS {
            let concept = self.build_concept(def, &[]);
            if !concept.implementations.is_empty() {
                results.push(concept);
            }
        }
        results.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// Explain how a specific concept is implemented.
    pub fn explain_concept(&self, name: &str) -> Option<CodeConcept> {
        let name_lower = name.to_lowercase();
        for def in CONCEPTS {
            if def.name.to_lowercase() == name_lower {
                let concept = self.build_concept(def, &[]);
                return Some(concept);
            }
        }
        None
    }

    // ── Internal ─────────────────────────────────────────────────────────

    fn build_concept(&self, def: &ConceptDef, constraints: &[ConceptConstraint]) -> CodeConcept {
        let mut implementations = Vec::new();

        for unit in self.graph.units() {
            let name_lower = unit.name.to_lowercase();
            let qname_lower = unit.qualified_name.to_lowercase();

            // Score how well this unit matches the concept
            let mut score = 0.0f64;
            for keyword in def.keywords {
                if name_lower.contains(keyword) {
                    score += 0.4;
                }
                if qname_lower.contains(keyword) {
                    score += 0.2;
                }
                if let Some(ref doc) = unit.doc_summary {
                    if doc.to_lowercase().contains(keyword) {
                        score += 0.15;
                    }
                }
            }

            // Type bonus
            if unit.unit_type == CodeUnitType::Function || unit.unit_type == CodeUnitType::Type {
                score += 0.05;
            }

            if score < 0.3 {
                continue;
            }

            // Apply constraints
            let passes_constraints = constraints.iter().all(|c| match c {
                ConceptConstraint::InModule(m) => qname_lower.contains(&m.to_lowercase()),
                ConceptConstraint::OfType(t) => {
                    unit.unit_type.label().to_lowercase() == t.to_lowercase()
                }
                ConceptConstraint::HasPattern(_) => true, // Simplified
            });

            if !passes_constraints {
                continue;
            }

            let aspect = if unit.unit_type == CodeUnitType::Function {
                "implementation".to_string()
            } else if unit.unit_type == CodeUnitType::Type {
                "definition".to_string()
            } else if unit.unit_type == CodeUnitType::Test {
                "test".to_string()
            } else {
                "usage".to_string()
            };

            implementations.push(ConceptImplementation {
                node_id: unit.id,
                name: unit.name.clone(),
                file_path: unit.file_path.display().to_string(),
                strength: score.min(1.0),
                aspect,
            });
        }

        // Sort by strength descending
        implementations.sort_by(|a, b| {
            b.strength
                .partial_cmp(&a.strength)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let confidence = if implementations.is_empty() {
            0.0
        } else {
            let top_strength = implementations[0].strength;
            (top_strength * 0.6 + (implementations.len() as f64 * 0.1).min(0.4)).min(1.0)
        };

        CodeConcept {
            name: def.name.to_string(),
            description: def.description.to_string(),
            implementations,
            related: def.related.iter().map(|s| s.to_string()).collect(),
            confidence,
        }
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
            "authenticate_user".to_string(),
            "auth.authenticate_user".to_string(),
            PathBuf::from("src/auth.py"),
            Span::new(1, 0, 30, 0),
        ));
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Python,
            "process_payment".to_string(),
            "payments.process_payment".to_string(),
            PathBuf::from("src/payments.py"),
            Span::new(1, 0, 20, 0),
        ));
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Python,
            "cache_result".to_string(),
            "utils.cache_result".to_string(),
            PathBuf::from("src/utils.py"),
            Span::new(1, 0, 15, 0),
        ));
        graph
    }

    #[test]
    fn find_authentication_concept() {
        let graph = test_graph();
        let nav = ConceptNavigator::new(&graph);
        let results = nav.find_concept(ConceptQuery {
            description: "authentication".to_string(),
            constraints: Vec::new(),
        });
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Authentication");
        assert!(!results[0].implementations.is_empty());
    }

    #[test]
    fn map_all_finds_concepts() {
        let graph = test_graph();
        let nav = ConceptNavigator::new(&graph);
        let all = nav.map_all_concepts();
        // Should find at least Authentication, Payment, Caching
        assert!(all.len() >= 2);
    }

    #[test]
    fn explain_concept_works() {
        let graph = test_graph();
        let nav = ConceptNavigator::new(&graph);
        let explained = nav.explain_concept("Payment");
        assert!(explained.is_some());
        let c = explained.unwrap();
        assert!(!c.implementations.is_empty());
    }
}
