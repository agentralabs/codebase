//! Multi-Codebase Compare Enhancement — Invention 10.
//!
//! Goes beyond symbol-level comparison to structural, conceptual, and
//! pattern-level differences between codebases loaded in a workspace.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::graph::CodeGraph;
use crate::types::CodeUnitType;

// ── Types ────────────────────────────────────────────────────────────────────

/// Structural diff between two codebases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuralDiff {
    /// Modules/directories in A but not B.
    pub only_in_a: Vec<String>,
    /// Modules/directories in B but not A.
    pub only_in_b: Vec<String>,
    /// Modules present in both but with different structure.
    pub modified: Vec<ModuleDiff>,
}

/// Diff of a single module between two codebases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDiff {
    /// Module name or path.
    pub module: String,
    /// Symbols in A but not B.
    pub symbols_only_a: Vec<String>,
    /// Symbols in B but not A.
    pub symbols_only_b: Vec<String>,
    /// Symbols present in both (possibly with different types/signatures).
    pub common_symbols: Vec<String>,
}

/// Conceptual diff — how high-level concepts differ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptualDiff {
    /// Concept name (e.g., "authentication", "error_handling").
    pub concept: String,
    /// How it appears in codebase A.
    pub in_a: Vec<String>,
    /// How it appears in codebase B.
    pub in_b: Vec<String>,
    /// Key differences.
    pub differences: Vec<String>,
}

/// Pattern diff — how design patterns differ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternDiff {
    /// Pattern name.
    pub pattern: String,
    /// Instances in A.
    pub instances_a: usize,
    /// Instances in B.
    pub instances_b: usize,
    /// Notable variations between A and B.
    pub variations: Vec<PatternVariation>,
}

/// A variation in how a pattern is applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternVariation {
    /// Description of the variation.
    pub description: String,
    /// Which codebase (A or B).
    pub source: String,
}

/// Full comparison result between two codebases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebaseComparison {
    /// Label for codebase A.
    pub label_a: String,
    /// Label for codebase B.
    pub label_b: String,
    /// Structural differences.
    pub structural: StructuralDiff,
    /// Conceptual differences.
    pub conceptual: Vec<ConceptualDiff>,
    /// Pattern differences.
    pub patterns: Vec<PatternDiff>,
    /// Summary statistics.
    pub summary: ComparisonSummary,
}

/// Summary statistics of a comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonSummary {
    /// Total units in A.
    pub units_a: usize,
    /// Total units in B.
    pub units_b: usize,
    /// Number of common symbols.
    pub common_symbols: usize,
    /// Symbols unique to A.
    pub unique_to_a: usize,
    /// Symbols unique to B.
    pub unique_to_b: usize,
    /// Similarity score 0.0–1.0.
    pub similarity: f64,
}

/// A migration step for porting from A to B.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStep {
    /// Order of this step.
    pub order: usize,
    /// What to migrate.
    pub description: String,
    /// Source symbols involved.
    pub source_symbols: Vec<String>,
    /// Estimated effort (low/medium/high).
    pub effort: String,
    /// Dependencies (other steps that must come first).
    pub dependencies: Vec<usize>,
}

// ── CodebaseComparer ─────────────────────────────────────────────────────────

/// Compares two codebases at multiple levels.
pub struct CodebaseComparer<'a, 'b> {
    graph_a: &'a CodeGraph,
    graph_b: &'b CodeGraph,
    label_a: String,
    label_b: String,
}

impl<'a, 'b> CodebaseComparer<'a, 'b> {
    pub fn new(
        graph_a: &'a CodeGraph,
        label_a: &str,
        graph_b: &'b CodeGraph,
        label_b: &str,
    ) -> Self {
        Self {
            graph_a,
            graph_b,
            label_a: label_a.to_string(),
            label_b: label_b.to_string(),
        }
    }

    /// Full structural + conceptual + pattern comparison.
    pub fn compare(&self) -> CodebaseComparison {
        let structural = self.compare_structural();
        let conceptual = self.compare_conceptual();
        let patterns = self.compare_patterns();

        // Compute summary
        let names_a: std::collections::HashSet<String> = self
            .graph_a
            .units()
            .iter()
            .map(|u| u.name.to_lowercase())
            .collect();
        let names_b: std::collections::HashSet<String> = self
            .graph_b
            .units()
            .iter()
            .map(|u| u.name.to_lowercase())
            .collect();

        let common: std::collections::HashSet<&String> = names_a.intersection(&names_b).collect();
        let unique_a = names_a.len() - common.len();
        let unique_b = names_b.len() - common.len();

        let total = names_a.len() + names_b.len();
        let similarity = if total > 0 {
            (common.len() * 2) as f64 / total as f64
        } else {
            0.0
        };

        CodebaseComparison {
            label_a: self.label_a.clone(),
            label_b: self.label_b.clone(),
            structural,
            conceptual,
            patterns,
            summary: ComparisonSummary {
                units_a: self.graph_a.unit_count(),
                units_b: self.graph_b.unit_count(),
                common_symbols: common.len(),
                unique_to_a: unique_a,
                unique_to_b: unique_b,
                similarity,
            },
        }
    }

    /// Compare how a specific concept is implemented across both codebases.
    pub fn compare_concept(&self, concept: &str) -> ConceptualDiff {
        let keywords: Vec<&str> = concept.split_whitespace().collect();

        let find_matches = |graph: &CodeGraph| -> Vec<String> {
            graph
                .units()
                .iter()
                .filter(|u| {
                    let name_lower = u.name.to_lowercase();
                    keywords
                        .iter()
                        .any(|kw| name_lower.contains(&kw.to_lowercase()))
                })
                .map(|u| format!("{} ({})", u.name, u.unit_type.label()))
                .collect()
        };

        let in_a = find_matches(self.graph_a);
        let in_b = find_matches(self.graph_b);

        let mut differences = Vec::new();
        if in_a.is_empty() && !in_b.is_empty() {
            differences.push(format!("'{}' not found in {}", concept, self.label_a));
        } else if !in_a.is_empty() && in_b.is_empty() {
            differences.push(format!("'{}' not found in {}", concept, self.label_b));
        } else if in_a.len() != in_b.len() {
            differences.push(format!(
                "Different number of implementations: {} in {}, {} in {}",
                in_a.len(),
                self.label_a,
                in_b.len(),
                self.label_b
            ));
        }

        ConceptualDiff {
            concept: concept.to_string(),
            in_a,
            in_b,
            differences,
        }
    }

    /// Generate an ordered migration plan from A to B.
    pub fn migration_plan(&self) -> Vec<MigrationStep> {
        let names_a: std::collections::HashSet<String> = self
            .graph_a
            .units()
            .iter()
            .map(|u| u.name.clone())
            .collect();
        let names_b: std::collections::HashSet<String> = self
            .graph_b
            .units()
            .iter()
            .map(|u| u.name.clone())
            .collect();

        let mut steps = Vec::new();
        let mut order = 1;

        // Step 1: Types first (they're dependencies for functions)
        let types_to_port: Vec<String> = self
            .graph_a
            .units()
            .iter()
            .filter(|u| u.unit_type == CodeUnitType::Type && !names_b.contains(&u.name))
            .map(|u| u.name.clone())
            .collect();

        if !types_to_port.is_empty() {
            steps.push(MigrationStep {
                order,
                description: format!("Port {} type definitions", types_to_port.len()),
                source_symbols: types_to_port,
                effort: "medium".to_string(),
                dependencies: Vec::new(),
            });
            order += 1;
        }

        // Step 2: Functions
        let fns_to_port: Vec<String> = self
            .graph_a
            .units()
            .iter()
            .filter(|u| u.unit_type == CodeUnitType::Function && !names_b.contains(&u.name))
            .map(|u| u.name.clone())
            .collect();

        if !fns_to_port.is_empty() {
            let dep = if order > 1 { vec![1] } else { Vec::new() };
            steps.push(MigrationStep {
                order,
                description: format!("Port {} functions", fns_to_port.len()),
                source_symbols: fns_to_port,
                effort: "high".to_string(),
                dependencies: dep,
            });
            order += 1;
        }

        // Step 3: Tests
        let tests_to_port: Vec<String> = self
            .graph_a
            .units()
            .iter()
            .filter(|u| u.unit_type == CodeUnitType::Test && !names_b.contains(&u.name))
            .map(|u| u.name.clone())
            .collect();

        if !tests_to_port.is_empty() {
            let dep = if order > 1 {
                vec![order - 1]
            } else {
                Vec::new()
            };
            steps.push(MigrationStep {
                order,
                description: format!("Port {} tests", tests_to_port.len()),
                source_symbols: tests_to_port,
                effort: "medium".to_string(),
                dependencies: dep,
            });
        }

        // Step 4: Remaining symbols not already covered
        let covered: std::collections::HashSet<String> = steps
            .iter()
            .flat_map(|s| s.source_symbols.iter().cloned())
            .collect();

        let remaining: Vec<String> = names_a
            .difference(&names_b)
            .filter(|n| !covered.contains(*n))
            .cloned()
            .collect();

        if !remaining.is_empty() {
            let prev_order = steps.last().map(|s| s.order).unwrap_or(0);
            steps.push(MigrationStep {
                order: prev_order + 1,
                description: format!("Port {} remaining symbols", remaining.len()),
                source_symbols: remaining,
                effort: "low".to_string(),
                dependencies: if prev_order > 0 {
                    vec![prev_order]
                } else {
                    Vec::new()
                },
            });
        }

        steps
    }

    // ── Internal ─────────────────────────────────────────────────────────

    fn compare_structural(&self) -> StructuralDiff {
        let dirs_a = self.extract_directories(self.graph_a);
        let dirs_b = self.extract_directories(self.graph_b);

        let only_in_a: Vec<String> = dirs_a
            .keys()
            .filter(|d| !dirs_b.contains_key(*d))
            .cloned()
            .collect();
        let only_in_b: Vec<String> = dirs_b
            .keys()
            .filter(|d| !dirs_a.contains_key(*d))
            .cloned()
            .collect();

        let mut modified = Vec::new();
        for (dir, syms_a) in &dirs_a {
            if let Some(syms_b) = dirs_b.get(dir) {
                let set_a: std::collections::HashSet<&String> = syms_a.iter().collect();
                let set_b: std::collections::HashSet<&String> = syms_b.iter().collect();

                let only_a: Vec<String> = set_a.difference(&set_b).map(|s| (*s).clone()).collect();
                let only_b_list: Vec<String> =
                    set_b.difference(&set_a).map(|s| (*s).clone()).collect();
                let common: Vec<String> =
                    set_a.intersection(&set_b).map(|s| (*s).clone()).collect();

                if !only_a.is_empty() || !only_b_list.is_empty() {
                    modified.push(ModuleDiff {
                        module: dir.clone(),
                        symbols_only_a: only_a,
                        symbols_only_b: only_b_list,
                        common_symbols: common,
                    });
                }
            }
        }

        StructuralDiff {
            only_in_a,
            only_in_b,
            modified,
        }
    }

    fn compare_conceptual(&self) -> Vec<ConceptualDiff> {
        let concepts = [
            "auth", "payment", "user", "database", "api", "error", "config", "cache", "log",
        ];

        concepts
            .iter()
            .map(|c| self.compare_concept(c))
            .filter(|d| !d.in_a.is_empty() || !d.in_b.is_empty())
            .collect()
    }

    fn compare_patterns(&self) -> Vec<PatternDiff> {
        let suffixes = [
            "handler",
            "service",
            "controller",
            "repository",
            "factory",
            "manager",
        ];
        let mut diffs = Vec::new();

        for suffix in &suffixes {
            let count_a = self
                .graph_a
                .units()
                .iter()
                .filter(|u| u.name.to_lowercase().ends_with(suffix))
                .count();
            let count_b = self
                .graph_b
                .units()
                .iter()
                .filter(|u| u.name.to_lowercase().ends_with(suffix))
                .count();

            if count_a > 0 || count_b > 0 {
                let mut variations = Vec::new();
                if count_a > 0 && count_b == 0 {
                    variations.push(PatternVariation {
                        description: format!("*_{} pattern only used in {}", suffix, self.label_a),
                        source: self.label_a.clone(),
                    });
                } else if count_b > 0 && count_a == 0 {
                    variations.push(PatternVariation {
                        description: format!("*_{} pattern only used in {}", suffix, self.label_b),
                        source: self.label_b.clone(),
                    });
                }

                diffs.push(PatternDiff {
                    pattern: format!("*_{}", suffix),
                    instances_a: count_a,
                    instances_b: count_b,
                    variations,
                });
            }
        }

        diffs
    }

    fn extract_directories(&self, graph: &CodeGraph) -> HashMap<String, Vec<String>> {
        let mut dirs: HashMap<String, Vec<String>> = HashMap::new();
        for unit in graph.units() {
            let dir = unit
                .file_path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            dirs.entry(dir).or_default().push(unit.name.clone());
        }
        dirs
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CodeUnit, CodeUnitType, Language, Span};
    use std::path::PathBuf;

    fn graph_a() -> CodeGraph {
        let mut g = CodeGraph::with_default_dimension();
        g.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "process_payment".to_string(),
            "billing::process_payment".to_string(),
            PathBuf::from("src/billing.rs"),
            Span::new(1, 0, 20, 0),
        ));
        g.add_unit(CodeUnit::new(
            CodeUnitType::Type,
            Language::Rust,
            "PaymentResult".to_string(),
            "billing::PaymentResult".to_string(),
            PathBuf::from("src/billing.rs"),
            Span::new(21, 0, 30, 0),
        ));
        g.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "auth_user".to_string(),
            "auth::auth_user".to_string(),
            PathBuf::from("src/auth.rs"),
            Span::new(1, 0, 15, 0),
        ));
        g
    }

    fn graph_b() -> CodeGraph {
        let mut g = CodeGraph::with_default_dimension();
        g.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "process_payment".to_string(),
            "billing::process_payment".to_string(),
            PathBuf::from("src/billing.rs"),
            Span::new(1, 0, 25, 0),
        ));
        g.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "validate_payment".to_string(),
            "billing::validate_payment".to_string(),
            PathBuf::from("src/billing.rs"),
            Span::new(26, 0, 40, 0),
        ));
        g
    }

    #[test]
    fn compare_finds_differences() {
        let a = graph_a();
        let b = graph_b();
        let comparer = CodebaseComparer::new(&a, "legacy", &b, "new");
        let result = comparer.compare();

        assert_eq!(result.summary.units_a, 3);
        assert_eq!(result.summary.units_b, 2);
        assert!(result.summary.common_symbols >= 1); // process_payment
    }

    #[test]
    fn compare_concept() {
        let a = graph_a();
        let b = graph_b();
        let comparer = CodebaseComparer::new(&a, "legacy", &b, "new");
        let diff = comparer.compare_concept("payment");

        assert!(!diff.in_a.is_empty());
        assert!(!diff.in_b.is_empty());
    }

    #[test]
    fn migration_plan_orders_types_first() {
        let a = graph_a();
        let b = graph_b();
        let comparer = CodebaseComparer::new(&a, "legacy", &b, "new");
        let plan = comparer.migration_plan();

        assert!(!plan.is_empty());
        // Types should come before functions
        if plan.len() >= 2 {
            assert!(plan[0].description.contains("type"));
        }
    }
}
