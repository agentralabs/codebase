//! Pattern Extraction — Invention 12.
//!
//! Extract implicit patterns and make them explicit, enforceable.
//! Codebase has patterns, but they're implicit. New code doesn't follow them.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::graph::CodeGraph;
use crate::types::CodeUnitType;

// ── Types ────────────────────────────────────────────────────────────────────

/// An extracted pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedPattern {
    /// Pattern name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Where it's used.
    pub instances: Vec<PatternInstance>,
    /// The pattern structure.
    pub structure: PatternStructure,
    /// Confidence it's intentional.
    pub confidence: f64,
    /// Violations (code that should follow but doesn't).
    pub violations: Vec<PatternViolation>,
}

/// An instance of a pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternInstance {
    /// Node ID.
    pub node_id: u64,
    /// Name.
    pub name: String,
    /// File.
    pub file_path: String,
    /// How well it matches.
    pub match_strength: f64,
    /// Any deviations.
    pub deviations: Vec<String>,
}

/// Structure of a pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternStructure {
    /// Template description.
    pub template: String,
    /// Required elements.
    pub required: Vec<String>,
    /// Optional elements.
    pub optional: Vec<String>,
    /// Anti-patterns (what NOT to do).
    pub anti_patterns: Vec<String>,
}

/// A pattern violation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternViolation {
    /// Node ID.
    pub node_id: u64,
    /// Node name.
    pub name: String,
    /// What's wrong.
    pub violation: String,
    /// How to fix.
    pub suggested_fix: String,
    /// Severity.
    pub severity: ViolationSeverity,
}

/// Severity of a violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViolationSeverity {
    Info,
    Warning,
    Error,
}

// ── PatternExtractor ─────────────────────────────────────────────────────────

/// Extracts and validates patterns in the codebase.
pub struct PatternExtractor<'g> {
    graph: &'g CodeGraph,
}

impl<'g> PatternExtractor<'g> {
    pub fn new(graph: &'g CodeGraph) -> Self {
        Self { graph }
    }

    /// Extract all detected patterns from the codebase.
    pub fn extract_patterns(&self) -> Vec<ExtractedPattern> {
        let mut patterns = Vec::new();

        patterns.extend(self.detect_naming_patterns());
        patterns.extend(self.detect_structural_patterns());

        // Sort by confidence
        patterns.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        patterns
    }

    /// Check code against detected patterns.
    pub fn check_patterns(&self, unit_id: u64) -> Vec<PatternViolation> {
        let patterns = self.extract_patterns();
        let mut violations = Vec::new();

        if let Some(unit) = self.graph.get_unit(unit_id) {
            for pattern in &patterns {
                // Check if this unit should follow the pattern
                let should_follow = pattern.instances.iter().any(|inst| {
                    // Same file directory or same module prefix
                    let unit_path = unit.file_path.display().to_string();
                    let inst_path = &inst.file_path;
                    unit_path
                        .rsplit_once('/')
                        .map(|(d, _)| inst_path.starts_with(d))
                        .unwrap_or(false)
                });

                if should_follow && !pattern.instances.iter().any(|inst| inst.node_id == unit_id) {
                    violations.push(PatternViolation {
                        node_id: unit_id,
                        name: unit.name.clone(),
                        violation: format!("Does not follow '{}' pattern", pattern.name),
                        suggested_fix: format!("Apply pattern: {}", pattern.structure.template),
                        severity: ViolationSeverity::Warning,
                    });
                }
            }
        }

        violations
    }

    /// Suggest patterns for new code based on location.
    pub fn suggest_patterns(&self, file_path: &str) -> Vec<ExtractedPattern> {
        let patterns = self.extract_patterns();
        patterns
            .into_iter()
            .filter(|p| {
                p.instances.iter().any(|inst| {
                    // Same directory
                    file_path
                        .rsplit_once('/')
                        .map(|(d, _)| inst.file_path.starts_with(d))
                        .unwrap_or(false)
                })
            })
            .collect()
    }

    // ── Internal ─────────────────────────────────────────────────────────

    fn detect_naming_patterns(&self) -> Vec<ExtractedPattern> {
        let mut prefix_groups: HashMap<String, Vec<(u64, String, String)>> = HashMap::new();
        let mut suffix_groups: HashMap<String, Vec<(u64, String, String)>> = HashMap::new();

        for unit in self.graph.units() {
            if unit.unit_type != CodeUnitType::Function && unit.unit_type != CodeUnitType::Type {
                continue;
            }

            let name = &unit.name;

            // Detect prefix patterns (e.g., get_*, create_*, handle_*)
            if let Some(prefix) = name.split('_').next() {
                if prefix.len() >= 3 {
                    prefix_groups
                        .entry(format!("{}_*", prefix))
                        .or_default()
                        .push((unit.id, name.clone(), unit.file_path.display().to_string()));
                }
            }

            // Detect suffix patterns (e.g., *_handler, *_service, *_controller)
            if let Some(suffix) = name.rsplit('_').next() {
                if suffix.len() >= 4 {
                    suffix_groups
                        .entry(format!("*_{}", suffix))
                        .or_default()
                        .push((unit.id, name.clone(), unit.file_path.display().to_string()));
                }
            }
        }

        let mut patterns = Vec::new();

        // Only report groups with 3+ members as patterns
        for (pattern_name, members) in prefix_groups.into_iter().chain(suffix_groups.into_iter()) {
            if members.len() < 3 {
                continue;
            }

            let instances: Vec<PatternInstance> = members
                .iter()
                .map(|(id, name, path)| PatternInstance {
                    node_id: *id,
                    name: name.clone(),
                    file_path: path.clone(),
                    match_strength: 1.0,
                    deviations: Vec::new(),
                })
                .collect();

            let confidence = (members.len() as f64 * 0.15).min(0.95);

            patterns.push(ExtractedPattern {
                name: format!("Naming: {}", pattern_name),
                description: format!(
                    "Functions/types following the '{}' naming pattern ({} instances)",
                    pattern_name,
                    members.len()
                ),
                instances,
                structure: PatternStructure {
                    template: pattern_name.clone(),
                    required: vec![format!("Follow '{}' naming convention", pattern_name)],
                    optional: Vec::new(),
                    anti_patterns: Vec::new(),
                },
                confidence,
                violations: Vec::new(),
            });
        }

        patterns
    }

    fn detect_structural_patterns(&self) -> Vec<ExtractedPattern> {
        let mut patterns = Vec::new();

        // Detect module organization patterns
        let mut dir_groups: HashMap<String, Vec<(u64, String, CodeUnitType)>> = HashMap::new();
        for unit in self.graph.units() {
            let dir = unit
                .file_path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            dir_groups
                .entry(dir)
                .or_default()
                .push((unit.id, unit.name.clone(), unit.unit_type));
        }

        for (dir, members) in &dir_groups {
            if members.len() < 3 || dir.is_empty() {
                continue;
            }

            // Check if all members are the same type (e.g., all functions, all types)
            let type_counts: HashMap<CodeUnitType, usize> =
                members.iter().fold(HashMap::new(), |mut acc, (_, _, t)| {
                    *acc.entry(*t).or_insert(0) += 1;
                    acc
                });

            if let Some((&dominant_type, &count)) = type_counts.iter().max_by_key(|(_, c)| *c) {
                if count as f64 / members.len() as f64 > 0.7 {
                    let instances: Vec<PatternInstance> = members
                        .iter()
                        .filter(|(_, _, t)| *t == dominant_type)
                        .map(|(id, name, _)| PatternInstance {
                            node_id: *id,
                            name: name.clone(),
                            file_path: dir.clone(),
                            match_strength: 1.0,
                            deviations: Vec::new(),
                        })
                        .collect();

                    patterns.push(ExtractedPattern {
                        name: format!("Directory: {} is {}", dir, dominant_type.label()),
                        description: format!(
                            "Directory '{}' primarily contains {} ({}% of {})",
                            dir,
                            dominant_type.label(),
                            (count * 100) / members.len(),
                            members.len()
                        ),
                        instances,
                        structure: PatternStructure {
                            template: format!("Place {} in {}", dominant_type.label(), dir),
                            required: vec![format!(
                                "New {} should go in {}",
                                dominant_type.label(),
                                dir
                            )],
                            optional: Vec::new(),
                            anti_patterns: vec![format!(
                                "Don't place non-{} code in {}",
                                dominant_type.label(),
                                dir
                            )],
                        },
                        confidence: (count as f64 / members.len() as f64).min(0.9),
                        violations: Vec::new(),
                    });
                }
            }
        }

        patterns
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
        // Naming pattern: get_*
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "get_user".to_string(),
            "mod::get_user".to_string(),
            PathBuf::from("src/api.rs"),
            Span::new(1, 0, 10, 0),
        ));
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "get_order".to_string(),
            "mod::get_order".to_string(),
            PathBuf::from("src/api.rs"),
            Span::new(11, 0, 20, 0),
        ));
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "get_product".to_string(),
            "mod::get_product".to_string(),
            PathBuf::from("src/api.rs"),
            Span::new(21, 0, 30, 0),
        ));
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "create_user".to_string(),
            "mod::create_user".to_string(),
            PathBuf::from("src/api.rs"),
            Span::new(31, 0, 40, 0),
        ));
        graph
    }

    #[test]
    fn extract_naming_patterns() {
        let graph = test_graph();
        let extractor = PatternExtractor::new(&graph);
        let patterns = extractor.extract_patterns();
        // Should find get_* pattern (3 instances)
        assert!(patterns.iter().any(|p| p.name.contains("get_")));
    }

    #[test]
    fn suggest_patterns_for_file() {
        let graph = test_graph();
        let extractor = PatternExtractor::new(&graph);
        let suggestions = extractor.suggest_patterns("src/api.rs");
        assert!(!suggestions.is_empty());
    }
}
