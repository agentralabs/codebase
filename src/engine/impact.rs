//! Enhanced Impact Analysis — Invention 1.
//!
//! Trace forward through the dependency graph to predict ALL affected code
//! before a change is made. Enriches the basic impact analysis with
//! ProposedChange, ImpactType classification, BlastRadius, and Mitigations.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::graph::traversal::{self, Direction, TraversalOptions};
use crate::graph::CodeGraph;
use crate::types::{CodeUnitType, EdgeType};

// ── Types ────────────────────────────────────────────────────────────────────

/// A proposed change to analyse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedChange {
    /// What's being changed.
    pub target: u64,
    /// Type of change.
    pub change_type: ChangeType,
    /// Description.
    pub description: String,
}

/// Type of proposed change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    /// Signature change (params, return type).
    Signature,
    /// Behavior change (same signature, different logic).
    Behavior,
    /// Deletion.
    Deletion,
    /// Rename.
    Rename,
    /// Move to different module.
    Move,
}

/// How a node is impacted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImpactType {
    /// Will definitely break (type error, missing function).
    WillBreak,
    /// Might break (behavior change, edge cases).
    MightBreak,
    /// Needs review (semantic dependency).
    NeedsReview,
    /// Safe (only reads, doesn't depend on changed behavior).
    Safe,
}

/// A node impacted by a change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactedNode {
    /// The affected node.
    pub node_id: u64,
    /// Path from change to this node.
    pub impact_path: Vec<u64>,
    /// Distance from original change.
    pub distance: u32,
    /// How it's affected.
    pub impact_type: ImpactType,
    /// Confidence this will actually break.
    pub break_probability: f64,
}

/// Total blast radius of a change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastRadius {
    /// Number of files affected.
    pub files_affected: usize,
    /// Number of functions affected.
    pub functions_affected: usize,
    /// Number of modules affected.
    pub modules_affected: usize,
    /// Lines of code in blast radius.
    pub loc_affected: usize,
    /// Test files affected.
    pub tests_affected: usize,
}

/// Risk level of the change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Safe to change.
    Low,
    /// Review recommended.
    Medium,
    /// Careful review required.
    High,
    /// Architectural implications.
    Critical,
}

/// A suggested mitigation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mitigation {
    /// Description of the mitigation.
    pub description: String,
    /// Estimated effort.
    pub effort: String,
    /// How much this reduces risk (0.0 - 1.0).
    pub risk_reduction: f64,
}

/// Full enhanced impact analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedImpactResult {
    /// The change being analyzed.
    pub change: ProposedChange,
    /// Directly affected nodes (distance = 1).
    pub direct_impact: Vec<ImpactedNode>,
    /// Transitively affected nodes (distance > 1).
    pub transitive_impact: Vec<ImpactedNode>,
    /// Risk assessment.
    pub risk_level: RiskLevel,
    /// Total blast radius.
    pub blast_radius: BlastRadius,
    /// Suggested mitigations.
    pub mitigations: Vec<Mitigation>,
}

// ── ImpactAnalyzer ───────────────────────────────────────────────────────────

/// Analyzes the impact of proposed changes on the codebase.
pub struct ImpactAnalyzer<'g> {
    graph: &'g CodeGraph,
}

impl<'g> ImpactAnalyzer<'g> {
    pub fn new(graph: &'g CodeGraph) -> Self {
        Self { graph }
    }

    /// Analyze the full impact of a proposed change.
    pub fn analyze(&self, change: ProposedChange, max_depth: u32) -> EnhancedImpactResult {
        let dependency_edges = vec![
            EdgeType::Calls,
            EdgeType::Imports,
            EdgeType::Inherits,
            EdgeType::Implements,
            EdgeType::UsesType,
            EdgeType::References,
            EdgeType::Returns,
            EdgeType::ParamType,
            EdgeType::Overrides,
        ];

        // BFS backward from target to find all dependents
        let options = TraversalOptions {
            max_depth: max_depth as i32,
            edge_types: dependency_edges,
            direction: Direction::Backward,
        };

        let traversal = traversal::bfs(self.graph, change.target, &options);

        // Build parent map for path reconstruction
        let mut parent_map: HashMap<u64, u64> = HashMap::new();
        let mut visited_order: Vec<(u64, u32)> = Vec::new();

        // Re-do BFS to track parents
        {
            let opts = TraversalOptions {
                max_depth: max_depth as i32,
                edge_types: vec![
                    EdgeType::Calls,
                    EdgeType::Imports,
                    EdgeType::Inherits,
                    EdgeType::Implements,
                    EdgeType::UsesType,
                    EdgeType::References,
                ],
                direction: Direction::Backward,
            };
            let mut visited = HashSet::new();
            let mut queue = std::collections::VecDeque::new();
            visited.insert(change.target);
            queue.push_back((change.target, 0u32));

            while let Some((current, depth)) = queue.pop_front() {
                visited_order.push((current, depth));
                if opts.max_depth >= 0 && depth >= opts.max_depth as u32 {
                    continue;
                }
                for edge in self.graph.edges_to(current) {
                    if !opts.edge_types.is_empty() && !opts.edge_types.contains(&edge.edge_type) {
                        continue;
                    }
                    if visited.insert(edge.source_id) {
                        parent_map.insert(edge.source_id, current);
                        queue.push_back((edge.source_id, depth + 1));
                    }
                }
            }
        }

        let mut direct_impact = Vec::new();
        let mut transitive_impact = Vec::new();

        for &(node_id, depth) in &traversal {
            if node_id == change.target {
                continue;
            }

            let impact_path = self.reconstruct_path(node_id, change.target, &parent_map);
            let impact_type = self.classify_impact(&change, node_id, depth);
            let break_probability = self.compute_break_probability(&change, node_id, depth);

            let impacted = ImpactedNode {
                node_id,
                impact_path,
                distance: depth,
                impact_type,
                break_probability,
            };

            if depth == 1 {
                direct_impact.push(impacted);
            } else {
                transitive_impact.push(impacted);
            }
        }

        let blast_radius = self.compute_blast_radius(&direct_impact, &transitive_impact);
        let risk_level = self.assess_risk(&blast_radius, &direct_impact, &transitive_impact);
        let mitigations = self.generate_mitigations(&change, &risk_level, &blast_radius);

        EnhancedImpactResult {
            change,
            direct_impact,
            transitive_impact,
            risk_level,
            blast_radius,
            mitigations,
        }
    }

    /// Find the shortest impact path between two nodes.
    pub fn impact_path(&self, from: u64, to: u64) -> Option<Vec<u64>> {
        traversal::shortest_path(self.graph, from, to, &[])
    }

    /// Generate a visualization-ready JSON structure.
    pub fn visualize(&self, result: &EnhancedImpactResult) -> serde_json::Value {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        // Root node
        if let Some(unit) = self.graph.get_unit(result.change.target) {
            nodes.push(serde_json::json!({
                "id": result.change.target,
                "name": unit.name,
                "type": "change_target",
                "risk": "source",
            }));
        }

        // Impact nodes
        for impacted in result
            .direct_impact
            .iter()
            .chain(result.transitive_impact.iter())
        {
            if let Some(unit) = self.graph.get_unit(impacted.node_id) {
                nodes.push(serde_json::json!({
                    "id": impacted.node_id,
                    "name": unit.name,
                    "type": format!("{:?}", impacted.impact_type),
                    "distance": impacted.distance,
                    "break_probability": impacted.break_probability,
                }));

                if impacted.impact_path.len() >= 2 {
                    let from = impacted.impact_path[impacted.impact_path.len() - 2];
                    edges.push(serde_json::json!({
                        "from": from,
                        "to": impacted.node_id,
                    }));
                }
            }
        }

        serde_json::json!({
            "nodes": nodes,
            "edges": edges,
            "blast_radius": {
                "files": result.blast_radius.files_affected,
                "functions": result.blast_radius.functions_affected,
                "modules": result.blast_radius.modules_affected,
                "tests": result.blast_radius.tests_affected,
            },
            "risk_level": format!("{:?}", result.risk_level),
        })
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    fn reconstruct_path(&self, from: u64, to: u64, parent_map: &HashMap<u64, u64>) -> Vec<u64> {
        let mut path = vec![from];
        let mut current = from;
        let mut seen = HashSet::new();
        seen.insert(current);
        while let Some(&parent) = parent_map.get(&current) {
            if !seen.insert(parent) {
                break;
            }
            path.push(parent);
            if parent == to {
                break;
            }
            current = parent;
        }
        path
    }

    fn classify_impact(&self, change: &ProposedChange, node_id: u64, distance: u32) -> ImpactType {
        let has_test = self
            .graph
            .edges_to(node_id)
            .iter()
            .any(|e| e.edge_type == EdgeType::Tests);

        match change.change_type {
            ChangeType::Deletion => {
                if distance == 1 {
                    ImpactType::WillBreak
                } else {
                    ImpactType::MightBreak
                }
            }
            ChangeType::Signature => {
                if distance == 1 {
                    ImpactType::WillBreak
                } else {
                    ImpactType::NeedsReview
                }
            }
            ChangeType::Rename => {
                if distance == 1 {
                    ImpactType::WillBreak
                } else {
                    ImpactType::NeedsReview
                }
            }
            ChangeType::Behavior => {
                if has_test {
                    ImpactType::NeedsReview
                } else {
                    ImpactType::MightBreak
                }
            }
            ChangeType::Move => {
                if distance == 1 {
                    ImpactType::MightBreak
                } else {
                    ImpactType::Safe
                }
            }
        }
    }

    fn compute_break_probability(
        &self,
        change: &ProposedChange,
        _node_id: u64,
        distance: u32,
    ) -> f64 {
        let base = match change.change_type {
            ChangeType::Deletion => 0.95,
            ChangeType::Signature => 0.85,
            ChangeType::Rename => 0.80,
            ChangeType::Behavior => 0.50,
            ChangeType::Move => 0.40,
        };

        // Decay with distance
        let decay = 1.0 / (1.0 + distance as f64 * 0.5);
        (base * decay).min(1.0)
    }

    fn compute_blast_radius(
        &self,
        direct: &[ImpactedNode],
        transitive: &[ImpactedNode],
    ) -> BlastRadius {
        let all_nodes: Vec<u64> = direct
            .iter()
            .chain(transitive.iter())
            .map(|n| n.node_id)
            .collect();

        let mut files = HashSet::new();
        let mut modules = HashSet::new();
        let mut functions = 0usize;
        let mut loc = 0usize;
        let mut tests = 0usize;

        for &node_id in &all_nodes {
            if let Some(unit) = self.graph.get_unit(node_id) {
                files.insert(unit.file_path.display().to_string());

                // Extract module from qualified name
                if let Some(last_dot) = unit.qualified_name.rfind('.') {
                    modules.insert(unit.qualified_name[..last_dot].to_string());
                } else if let Some(last_sep) = unit.qualified_name.rfind("::") {
                    modules.insert(unit.qualified_name[..last_sep].to_string());
                }

                if unit.unit_type == CodeUnitType::Function {
                    functions += 1;
                }
                if unit.unit_type == CodeUnitType::Test {
                    tests += 1;
                }

                let lines = if unit.span.end_line > unit.span.start_line {
                    (unit.span.end_line - unit.span.start_line) as usize
                } else {
                    1
                };
                loc += lines;
            }
        }

        BlastRadius {
            files_affected: files.len(),
            functions_affected: functions,
            modules_affected: modules.len(),
            loc_affected: loc,
            tests_affected: tests,
        }
    }

    fn assess_risk(
        &self,
        blast_radius: &BlastRadius,
        direct: &[ImpactedNode],
        transitive: &[ImpactedNode],
    ) -> RiskLevel {
        let total = direct.len() + transitive.len();
        let will_break = direct
            .iter()
            .chain(transitive.iter())
            .filter(|n| n.impact_type == ImpactType::WillBreak)
            .count();

        if total == 0 {
            return RiskLevel::Low;
        }

        if will_break > 10 || blast_radius.files_affected > 20 {
            RiskLevel::Critical
        } else if will_break > 3 || blast_radius.files_affected > 10 || total > 30 {
            RiskLevel::High
        } else if will_break > 0 || total > 10 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        }
    }

    fn generate_mitigations(
        &self,
        change: &ProposedChange,
        risk_level: &RiskLevel,
        blast_radius: &BlastRadius,
    ) -> Vec<Mitigation> {
        let mut mitigations = Vec::new();

        match change.change_type {
            ChangeType::Signature => {
                mitigations.push(Mitigation {
                    description: "Add compatibility wrapper that delegates to new signature"
                        .to_string(),
                    effort: "Low".to_string(),
                    risk_reduction: 0.7,
                });
                mitigations.push(Mitigation {
                    description: "Deprecate old signature with migration period".to_string(),
                    effort: "Medium".to_string(),
                    risk_reduction: 0.9,
                });
            }
            ChangeType::Deletion => {
                mitigations.push(Mitigation {
                    description: "Replace with deprecation warning first".to_string(),
                    effort: "Low".to_string(),
                    risk_reduction: 0.5,
                });
            }
            ChangeType::Rename => {
                mitigations.push(Mitigation {
                    description: "Add type alias or re-export from old name".to_string(),
                    effort: "Low".to_string(),
                    risk_reduction: 0.8,
                });
            }
            _ => {}
        }

        if *risk_level == RiskLevel::High || *risk_level == RiskLevel::Critical {
            mitigations.push(Mitigation {
                description: "Deploy incrementally with feature flags".to_string(),
                effort: "Medium".to_string(),
                risk_reduction: 0.6,
            });
        }

        if blast_radius.tests_affected == 0 {
            mitigations.push(Mitigation {
                description: "Add tests before making the change".to_string(),
                effort: "Medium".to_string(),
                risk_reduction: 0.4,
            });
        }

        mitigations
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

        // A -> B -> C chain
        let a = graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "func_a".to_string(),
            "mod::func_a".to_string(),
            PathBuf::from("src/a.rs"),
            Span::new(1, 0, 10, 0),
        ));
        let b = graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "func_b".to_string(),
            "mod::func_b".to_string(),
            PathBuf::from("src/b.rs"),
            Span::new(1, 0, 20, 0),
        ));
        let c = graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "func_c".to_string(),
            "mod::func_c".to_string(),
            PathBuf::from("src/c.rs"),
            Span::new(1, 0, 15, 0),
        ));

        // B calls A, C calls B
        let _ = graph.add_edge(Edge::new(b, a, EdgeType::Calls));
        let _ = graph.add_edge(Edge::new(c, b, EdgeType::Calls));

        graph
    }

    #[test]
    fn analyze_deletion_impact() {
        let graph = test_graph();
        let analyzer = ImpactAnalyzer::new(&graph);
        let change = ProposedChange {
            target: 0, // func_a
            change_type: ChangeType::Deletion,
            description: "Delete func_a".to_string(),
        };
        let result = analyzer.analyze(change, 5);
        assert!(!result.direct_impact.is_empty());
        assert_eq!(result.direct_impact[0].impact_type, ImpactType::WillBreak);
    }

    #[test]
    fn blast_radius_computed() {
        let graph = test_graph();
        let analyzer = ImpactAnalyzer::new(&graph);
        let change = ProposedChange {
            target: 0,
            change_type: ChangeType::Signature,
            description: "Change signature".to_string(),
        };
        let result = analyzer.analyze(change, 5);
        assert!(result.blast_radius.files_affected > 0);
    }

    #[test]
    fn mitigations_generated() {
        let graph = test_graph();
        let analyzer = ImpactAnalyzer::new(&graph);
        let change = ProposedChange {
            target: 0,
            change_type: ChangeType::Signature,
            description: "Change params".to_string(),
        };
        let result = analyzer.analyze(change, 5);
        assert!(!result.mitigations.is_empty());
    }

    #[test]
    fn visualize_produces_json() {
        let graph = test_graph();
        let analyzer = ImpactAnalyzer::new(&graph);
        let change = ProposedChange {
            target: 0,
            change_type: ChangeType::Behavior,
            description: "Change behavior".to_string(),
        };
        let result = analyzer.analyze(change, 3);
        let viz = analyzer.visualize(&result);
        assert!(viz.get("nodes").is_some());
        assert!(viz.get("edges").is_some());
    }
}
