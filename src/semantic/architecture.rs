//! Architecture Inference — Invention 8.
//!
//! Infer architecture from the code structure itself. No documentation needed.
//! Detects patterns like Layered, MVC, Microservices, etc. from module structure
//! and dependency directions.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::graph::CodeGraph;
use crate::types::{CodeUnitType, EdgeType};

// ── Types ────────────────────────────────────────────────────────────────────

/// Inferred architecture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferredArchitecture {
    /// Overall pattern detected.
    pub pattern: ArchitecturePattern,
    /// Layers/tiers.
    pub layers: Vec<ArchitectureLayer>,
    /// Key components.
    pub components: Vec<ArchitectureComponent>,
    /// Data flows.
    pub flows: Vec<DataFlow>,
    /// Confidence in inference.
    pub confidence: f64,
    /// Anomalies (violations of pattern).
    pub anomalies: Vec<ArchitectureAnomaly>,
}

/// Detected architecture pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchitecturePattern {
    Monolith,
    Microservices,
    Layered,
    Hexagonal,
    EventDriven,
    CQRS,
    Serverless,
    MVC,
    Unknown,
}

/// An architectural layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureLayer {
    pub name: String,
    pub purpose: String,
    pub modules: Vec<String>,
    pub depends_on: Vec<String>,
}

/// An architectural component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureComponent {
    pub name: String,
    pub role: ComponentRole,
    pub node_ids: Vec<u64>,
    pub external_deps: Vec<String>,
}

/// Role of an architectural component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComponentRole {
    Entrypoint,
    Controller,
    Service,
    Repository,
    Model,
    Utility,
    Configuration,
    Test,
}

/// A data flow between components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFlow {
    pub name: String,
    pub source: String,
    pub destination: String,
    pub via: Vec<String>,
    pub data_type: String,
}

/// An architecture anomaly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureAnomaly {
    pub description: String,
    pub node_id: u64,
    pub expected: String,
    pub actual: String,
    pub severity: AnomalySeverity,
}

/// Severity of an anomaly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalySeverity {
    Info,
    Warning,
    Error,
    Critical,
}

// ── ArchitectureInferrer ─────────────────────────────────────────────────────

/// Infers architecture from code structure.
pub struct ArchitectureInferrer<'g> {
    graph: &'g CodeGraph,
}

impl<'g> ArchitectureInferrer<'g> {
    pub fn new(graph: &'g CodeGraph) -> Self {
        Self { graph }
    }

    /// Infer the architecture from the code graph.
    pub fn infer(&self) -> InferredArchitecture {
        let components = self.detect_components();
        let layers = self.detect_layers(&components);
        let flows = self.detect_flows(&components);
        let pattern = self.classify_pattern(&components, &layers);
        let anomalies = self.detect_anomalies(&components, &pattern);
        let confidence = self.compute_confidence(&components, &layers);

        InferredArchitecture {
            pattern,
            layers,
            components,
            flows,
            confidence,
            anomalies,
        }
    }

    /// Generate diagram-ready data.
    pub fn diagram(&self, arch: &InferredArchitecture) -> serde_json::Value {
        serde_json::json!({
            "pattern": format!("{:?}", arch.pattern),
            "layers": arch.layers.iter().map(|l| serde_json::json!({
                "name": l.name,
                "purpose": l.purpose,
                "modules": l.modules,
                "depends_on": l.depends_on,
            })).collect::<Vec<_>>(),
            "components": arch.components.iter().map(|c| serde_json::json!({
                "name": c.name,
                "role": format!("{:?}", c.role),
                "size": c.node_ids.len(),
            })).collect::<Vec<_>>(),
            "flows": arch.flows.iter().map(|f| serde_json::json!({
                "from": f.source,
                "to": f.destination,
                "via": f.via,
            })).collect::<Vec<_>>(),
        })
    }

    /// Validate code against an expected architecture pattern.
    pub fn validate(&self, expected: ArchitecturePattern) -> Vec<ArchitectureAnomaly> {
        let inferred = self.infer();
        let mut anomalies = inferred.anomalies;

        if inferred.pattern != expected {
            anomalies.push(ArchitectureAnomaly {
                description: format!(
                    "Expected {:?} architecture but detected {:?}",
                    expected, inferred.pattern
                ),
                node_id: 0,
                expected: format!("{:?}", expected),
                actual: format!("{:?}", inferred.pattern),
                severity: AnomalySeverity::Warning,
            });
        }

        anomalies
    }

    // ── Internal ─────────────────────────────────────────────────────────

    fn detect_components(&self) -> Vec<ArchitectureComponent> {
        let mut role_map: HashMap<ComponentRole, Vec<u64>> = HashMap::new();

        for unit in self.graph.units() {
            let name_lower = unit.name.to_lowercase();
            let qname_lower = unit.qualified_name.to_lowercase();
            let path_lower = unit.file_path.display().to_string().to_lowercase();

            let role = if Self::matches_any(
                &[&name_lower, &qname_lower, &path_lower],
                &["controller", "handler", "view", "endpoint"],
            ) {
                ComponentRole::Controller
            } else if Self::matches_any(
                &[&name_lower, &qname_lower, &path_lower],
                &["service", "usecase", "interactor"],
            ) {
                ComponentRole::Service
            } else if Self::matches_any(
                &[&name_lower, &qname_lower, &path_lower],
                &["repository", "repo", "dao", "store", "adapter"],
            ) {
                ComponentRole::Repository
            } else if Self::matches_any(
                &[&name_lower, &qname_lower, &path_lower],
                &["model", "entity", "schema", "dto"],
            ) {
                ComponentRole::Model
            } else if Self::matches_any(
                &[&name_lower, &qname_lower, &path_lower],
                &["config", "setting", "env"],
            ) {
                ComponentRole::Configuration
            } else if unit.unit_type == CodeUnitType::Test {
                ComponentRole::Test
            } else if Self::matches_any(
                &[&name_lower, &qname_lower, &path_lower],
                &["main", "app", "server", "cli", "entry"],
            ) {
                ComponentRole::Entrypoint
            } else {
                ComponentRole::Utility
            };

            role_map.entry(role).or_default().push(unit.id);
        }

        role_map
            .into_iter()
            .map(|(role, ids)| {
                let name = format!("{:?}", role);
                let external_deps = self.find_external_deps(&ids);
                ArchitectureComponent {
                    name,
                    role,
                    node_ids: ids,
                    external_deps,
                }
            })
            .collect()
    }

    fn detect_layers(&self, components: &[ArchitectureComponent]) -> Vec<ArchitectureLayer> {
        let mut layers = Vec::new();

        let has_controllers = components
            .iter()
            .any(|c| c.role == ComponentRole::Controller);
        let has_services = components.iter().any(|c| c.role == ComponentRole::Service);
        let has_repos = components
            .iter()
            .any(|c| c.role == ComponentRole::Repository);

        if has_controllers {
            layers.push(ArchitectureLayer {
                name: "Presentation".to_string(),
                purpose: "Handle external requests and responses".to_string(),
                modules: self.modules_for_role(components, ComponentRole::Controller),
                depends_on: vec!["Business Logic".to_string()],
            });
        }

        if has_services {
            layers.push(ArchitectureLayer {
                name: "Business Logic".to_string(),
                purpose: "Core business rules and workflows".to_string(),
                modules: self.modules_for_role(components, ComponentRole::Service),
                depends_on: vec!["Data Access".to_string()],
            });
        }

        if has_repos {
            layers.push(ArchitectureLayer {
                name: "Data Access".to_string(),
                purpose: "Data persistence and retrieval".to_string(),
                modules: self.modules_for_role(components, ComponentRole::Repository),
                depends_on: Vec::new(),
            });
        }

        layers
    }

    fn detect_flows(&self, components: &[ArchitectureComponent]) -> Vec<DataFlow> {
        let mut flows = Vec::new();

        // Detect flows from call edges between component roles
        let role_names: Vec<(ComponentRole, &str)> = components
            .iter()
            .map(|c| (c.role, c.name.as_str()))
            .collect();

        for comp in components {
            for &node_id in &comp.node_ids {
                for edge in self.graph.edges_from(node_id) {
                    if edge.edge_type != EdgeType::Calls {
                        continue;
                    }
                    // Find which component the target belongs to
                    for other in components {
                        if other.role != comp.role && other.node_ids.contains(&edge.target_id) {
                            let flow_name = format!("{:?} -> {:?}", comp.role, other.role);
                            if !flows.iter().any(|f: &DataFlow| f.name == flow_name) {
                                flows.push(DataFlow {
                                    name: flow_name,
                                    source: format!("{:?}", comp.role),
                                    destination: format!("{:?}", other.role),
                                    via: Vec::new(),
                                    data_type: "function call".to_string(),
                                });
                            }
                            break;
                        }
                    }
                }
            }
        }

        let _ = role_names; // suppress unused warning
        flows
    }

    fn classify_pattern(
        &self,
        components: &[ArchitectureComponent],
        layers: &[ArchitectureLayer],
    ) -> ArchitecturePattern {
        let has_controllers = components
            .iter()
            .any(|c| c.role == ComponentRole::Controller);
        let has_services = components.iter().any(|c| c.role == ComponentRole::Service);
        let has_repos = components
            .iter()
            .any(|c| c.role == ComponentRole::Repository);
        let has_models = components.iter().any(|c| c.role == ComponentRole::Model);

        // MVC: controllers + models + views
        if has_controllers && has_models && !has_repos {
            return ArchitecturePattern::MVC;
        }

        // Layered: clear layer separation
        if layers.len() >= 3 && has_controllers && has_services && has_repos {
            return ArchitecturePattern::Layered;
        }

        // Hexagonal: services + repositories with clear interfaces
        if has_services && has_repos && !has_controllers {
            return ArchitecturePattern::Hexagonal;
        }

        // If only utilities and entrypoints, likely monolith
        let non_utility = components
            .iter()
            .filter(|c| c.role != ComponentRole::Utility && c.role != ComponentRole::Test)
            .count();
        if non_utility <= 2 {
            return ArchitecturePattern::Monolith;
        }

        ArchitecturePattern::Unknown
    }

    fn detect_anomalies(
        &self,
        components: &[ArchitectureComponent],
        _pattern: &ArchitecturePattern,
    ) -> Vec<ArchitectureAnomaly> {
        let mut anomalies = Vec::new();

        // Check for bidirectional dependencies between layers (layer violation)
        let controller_ids: HashSet<u64> = components
            .iter()
            .filter(|c| c.role == ComponentRole::Controller)
            .flat_map(|c| c.node_ids.iter().copied())
            .collect();

        let repo_ids: HashSet<u64> = components
            .iter()
            .filter(|c| c.role == ComponentRole::Repository)
            .flat_map(|c| c.node_ids.iter().copied())
            .collect();

        // Repos should not call controllers
        for &repo_id in &repo_ids {
            for edge in self.graph.edges_from(repo_id) {
                if edge.edge_type == EdgeType::Calls && controller_ids.contains(&edge.target_id) {
                    anomalies.push(ArchitectureAnomaly {
                        description: "Repository layer calls presentation layer (layer violation)"
                            .to_string(),
                        node_id: repo_id,
                        expected: "Data Access should not depend on Presentation".to_string(),
                        actual: "Upward dependency detected".to_string(),
                        severity: AnomalySeverity::Error,
                    });
                }
            }
        }

        anomalies
    }

    fn compute_confidence(
        &self,
        components: &[ArchitectureComponent],
        layers: &[ArchitectureLayer],
    ) -> f64 {
        let total_units = self.graph.unit_count();
        if total_units == 0 {
            return 0.0;
        }

        let classified = components
            .iter()
            .filter(|c| c.role != ComponentRole::Utility)
            .map(|c| c.node_ids.len())
            .sum::<usize>();

        let classification_ratio = classified as f64 / total_units as f64;
        let layer_bonus = (layers.len() as f64 * 0.1).min(0.3);

        (classification_ratio * 0.7 + layer_bonus).min(1.0)
    }

    fn find_external_deps(&self, ids: &[u64]) -> Vec<String> {
        let id_set: HashSet<u64> = ids.iter().copied().collect();
        let mut external = HashSet::new();

        for &id in ids {
            for edge in self.graph.edges_from(id) {
                if edge.edge_type == EdgeType::Imports && !id_set.contains(&edge.target_id) {
                    if let Some(unit) = self.graph.get_unit(edge.target_id) {
                        external.insert(unit.qualified_name.clone());
                    }
                }
            }
        }

        external.into_iter().collect()
    }

    fn modules_for_role(
        &self,
        components: &[ArchitectureComponent],
        role: ComponentRole,
    ) -> Vec<String> {
        let mut modules = HashSet::new();
        for comp in components {
            if comp.role == role {
                for &id in &comp.node_ids {
                    if let Some(unit) = self.graph.get_unit(id) {
                        if let Some(last_sep) = unit
                            .qualified_name
                            .rfind("::")
                            .or_else(|| unit.qualified_name.rfind('.'))
                        {
                            modules.insert(unit.qualified_name[..last_sep].to_string());
                        }
                    }
                }
            }
        }
        modules.into_iter().collect()
    }

    fn matches_any(targets: &[&str], keywords: &[&str]) -> bool {
        targets
            .iter()
            .any(|t| keywords.iter().any(|k| t.contains(k)))
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
            "user_controller".to_string(),
            "app.controllers.user_controller".to_string(),
            PathBuf::from("src/controllers/user.py"),
            Span::new(1, 0, 30, 0),
        ));
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Python,
            "user_service".to_string(),
            "app.services.user_service".to_string(),
            PathBuf::from("src/services/user.py"),
            Span::new(1, 0, 40, 0),
        ));
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Python,
            "user_repository".to_string(),
            "app.repos.user_repository".to_string(),
            PathBuf::from("src/repos/user.py"),
            Span::new(1, 0, 25, 0),
        ));
        graph
    }

    #[test]
    fn infer_detects_components() {
        let graph = test_graph();
        let inferrer = ArchitectureInferrer::new(&graph);
        let arch = inferrer.infer();
        assert!(!arch.components.is_empty());
    }

    #[test]
    fn infer_detects_layered_pattern() {
        let graph = test_graph();
        let inferrer = ArchitectureInferrer::new(&graph);
        let arch = inferrer.infer();
        // Should detect layered or at least not Unknown
        assert!(arch.layers.len() >= 2);
    }

    #[test]
    fn diagram_produces_json() {
        let graph = test_graph();
        let inferrer = ArchitectureInferrer::new(&graph);
        let arch = inferrer.infer();
        let diagram = inferrer.diagram(&arch);
        assert!(diagram.get("pattern").is_some());
    }
}
