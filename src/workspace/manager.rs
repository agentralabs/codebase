//! Workspace manager — owns workspaces, provides cross-context queries.
//!
//! The [`WorkspaceManager`] is the top-level entry point. It creates workspaces,
//! adds codebase contexts to them, and exposes query, compare, and
//! cross-reference operations that span multiple [`CodeGraph`] instances.

use std::collections::HashMap;

use crate::graph::CodeGraph;

// ---------------------------------------------------------------------------
// ContextRole
// ---------------------------------------------------------------------------

/// The role a codebase plays within a workspace.
///
/// Roles are used to distinguish the *intent* behind each loaded codebase.
/// For example, in a C++ to Rust migration the legacy C++ graph is `Source`
/// and the new Rust graph is `Target`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextRole {
    /// The original codebase being migrated from or analysed.
    Source,
    /// The destination codebase being migrated to or built.
    Target,
    /// An auxiliary codebase used for reference (e.g., a library API).
    Reference,
    /// A codebase loaded solely for side-by-side comparison.
    Comparison,
}

impl ContextRole {
    /// Parse a role from a string (case-insensitive).
    ///
    /// Returns `None` if the string does not match any known role.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "source" => Some(Self::Source),
            "target" => Some(Self::Target),
            "reference" => Some(Self::Reference),
            "comparison" => Some(Self::Comparison),
            _ => None,
        }
    }

    /// A human-readable label for this role.
    pub fn label(&self) -> &str {
        match self {
            Self::Source => "source",
            Self::Target => "target",
            Self::Reference => "reference",
            Self::Comparison => "comparison",
        }
    }
}

impl std::fmt::Display for ContextRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// ---------------------------------------------------------------------------
// CodebaseContext
// ---------------------------------------------------------------------------

/// A single codebase loaded into a workspace.
///
/// Each context wraps a [`CodeGraph`] and carries metadata about the role it
/// plays, its root path on disk, and an optional language hint.
#[derive(Debug)]
pub struct CodebaseContext {
    /// Unique identifier within the workspace (e.g., `"ctx-1"`).
    pub id: String,
    /// Role this codebase plays in the workspace.
    pub role: ContextRole,
    /// Root path of the codebase on disk.
    pub path: String,
    /// Primary language of the codebase, if known.
    pub language: Option<String>,
    /// The parsed code graph.
    pub graph: CodeGraph,
}

// ---------------------------------------------------------------------------
// Workspace
// ---------------------------------------------------------------------------

/// A named collection of codebase contexts that can be queried together.
#[derive(Debug)]
pub struct Workspace {
    /// Unique workspace identifier (e.g., `"ws-1"`).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Codebase contexts in this workspace.
    pub contexts: Vec<CodebaseContext>,
    /// Creation timestamp (Unix epoch microseconds).
    pub created_at: u64,
}

// ---------------------------------------------------------------------------
// Query result types
// ---------------------------------------------------------------------------

/// The result of a cross-context symbol query for a single context.
#[derive(Debug)]
pub struct CrossContextResult {
    /// Which context produced these matches.
    pub context_id: String,
    /// Role of the matching context.
    pub context_role: ContextRole,
    /// Matching symbols within the context.
    pub matches: Vec<SymbolMatch>,
}

/// A single symbol that matched a query.
#[derive(Debug)]
pub struct SymbolMatch {
    /// Code unit ID within its graph.
    pub unit_id: u64,
    /// Simple symbol name.
    pub name: String,
    /// Fully qualified name.
    pub qualified_name: String,
    /// Human-readable type label (from [`CodeUnitType::label`]).
    pub unit_type: String,
    /// File path where the symbol is defined.
    pub file_path: String,
}

/// Side-by-side comparison of a symbol across all contexts.
#[derive(Debug)]
pub struct Comparison {
    /// The symbol name being compared.
    pub symbol: String,
    /// Per-context comparison entries.
    pub contexts: Vec<ContextComparison>,
    /// Semantic similarity score between the matched units (0.0 = unrelated,
    /// 1.0 = identical). Defaults to 0.0 when no vector comparison is possible.
    pub semantic_match: f32,
    /// Structural differences observed across contexts.
    pub structural_diff: Vec<String>,
}

/// Comparison data for one context within a [`Comparison`].
#[derive(Debug)]
pub struct ContextComparison {
    /// Context identifier.
    pub context_id: String,
    /// Context role.
    pub role: ContextRole,
    /// Whether the symbol was found in this context.
    pub found: bool,
    /// Type label if the symbol was found.
    pub unit_type: Option<String>,
    /// Signature string if the symbol was found.
    pub signature: Option<String>,
    /// File path if the symbol was found.
    pub file_path: Option<String>,
}

/// Cross-reference report showing where a symbol exists and where it is missing.
#[derive(Debug)]
pub struct CrossReference {
    /// The symbol being referenced.
    pub symbol: String,
    /// Contexts (and their roles) where the symbol was found.
    pub found_in: Vec<(String, ContextRole)>,
    /// Contexts (and their roles) where the symbol is absent.
    pub missing_from: Vec<(String, ContextRole)>,
}

// ---------------------------------------------------------------------------
// WorkspaceManager
// ---------------------------------------------------------------------------

/// Owns all workspaces and provides the public API for multi-context operations.
///
/// # Design
///
/// Workspaces and contexts are identified by auto-generated string IDs
/// (`"ws-N"`, `"ctx-N"`). The manager tracks which workspace is currently
/// *active* to serve as a convenient default in CLI/MCP workflows.
#[derive(Debug)]
pub struct WorkspaceManager {
    /// All workspaces, keyed by ID.
    workspaces: HashMap<String, Workspace>,
    /// The currently active workspace ID, if any.
    active: Option<String>,
    /// Monotonically increasing counter used to generate unique IDs.
    next_id: u64,
}

impl WorkspaceManager {
    // -- Construction -------------------------------------------------------

    /// Create a new, empty workspace manager.
    pub fn new() -> Self {
        Self {
            workspaces: HashMap::new(),
            active: None,
            next_id: 1,
        }
    }

    // -- Workspace lifecycle ------------------------------------------------

    /// Create a new workspace with the given name.
    ///
    /// The workspace becomes the active workspace and its generated ID is
    /// returned (e.g., `"ws-1"`).
    pub fn create(&mut self, name: &str) -> String {
        let id = format!("ws-{}", self.next_id);
        self.next_id += 1;

        let workspace = Workspace {
            id: id.clone(),
            name: name.to_string(),
            contexts: Vec::new(),
            created_at: crate::types::now_micros(),
        };

        self.workspaces.insert(id.clone(), workspace);
        self.active = Some(id.clone());
        id
    }

    /// Add a codebase context to an existing workspace.
    ///
    /// Returns the generated context ID (e.g., `"ctx-2"`) or an error if the
    /// workspace does not exist.
    pub fn add_context(
        &mut self,
        workspace_id: &str,
        path: &str,
        role: ContextRole,
        language: Option<String>,
        graph: CodeGraph,
    ) -> Result<String, String> {
        let ctx_id = format!("ctx-{}", self.next_id);
        self.next_id += 1;

        let workspace = self
            .workspaces
            .get_mut(workspace_id)
            .ok_or_else(|| format!("workspace '{}' not found", workspace_id))?;

        workspace.contexts.push(CodebaseContext {
            id: ctx_id.clone(),
            role,
            path: path.to_string(),
            language,
            graph,
        });

        Ok(ctx_id)
    }

    /// Return a reference to the given workspace.
    pub fn list(&self, workspace_id: &str) -> Result<&Workspace, String> {
        self.workspaces
            .get(workspace_id)
            .ok_or_else(|| format!("workspace '{}' not found", workspace_id))
    }

    /// Return the ID of the currently active workspace, if any.
    pub fn get_active(&self) -> Option<&str> {
        self.active.as_deref()
    }

    // -- Cross-context queries ----------------------------------------------

    /// Search **all** contexts in a workspace for symbols whose name contains
    /// `query` (case-insensitive substring match).
    ///
    /// Returns one [`CrossContextResult`] per context that has at least one
    /// matching symbol.
    pub fn query_all(
        &self,
        workspace_id: &str,
        query: &str,
    ) -> Result<Vec<CrossContextResult>, String> {
        let workspace = self
            .workspaces
            .get(workspace_id)
            .ok_or_else(|| format!("workspace '{}' not found", workspace_id))?;

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for ctx in &workspace.contexts {
            let matches = Self::search_graph(&ctx.graph, &query_lower);
            if !matches.is_empty() {
                results.push(CrossContextResult {
                    context_id: ctx.id.clone(),
                    context_role: ctx.role.clone(),
                    matches,
                });
            }
        }

        Ok(results)
    }

    /// Search a **single** context for symbols whose name contains `query`
    /// (case-insensitive substring match).
    pub fn query_context(
        &self,
        workspace_id: &str,
        context_id: &str,
        query: &str,
    ) -> Result<Vec<SymbolMatch>, String> {
        let workspace = self
            .workspaces
            .get(workspace_id)
            .ok_or_else(|| format!("workspace '{}' not found", workspace_id))?;

        let ctx = workspace
            .contexts
            .iter()
            .find(|c| c.id == context_id)
            .ok_or_else(|| {
                format!(
                    "context '{}' not found in workspace '{}'",
                    context_id, workspace_id
                )
            })?;

        let query_lower = query.to_lowercase();
        Ok(Self::search_graph(&ctx.graph, &query_lower))
    }

    /// Compare a symbol across **all** contexts in a workspace.
    ///
    /// For every context the method records whether the symbol was found and,
    /// if so, its type, signature, and file path. Structural differences
    /// (e.g., different types or signatures) are collected into
    /// [`Comparison::structural_diff`].
    pub fn compare(&self, workspace_id: &str, symbol: &str) -> Result<Comparison, String> {
        let workspace = self
            .workspaces
            .get(workspace_id)
            .ok_or_else(|| format!("workspace '{}' not found", workspace_id))?;

        let symbol_lower = symbol.to_lowercase();
        let mut ctx_comparisons = Vec::new();
        let mut structural_diff = Vec::new();

        // Collect per-context matches (take the first exact-name hit per context).
        let mut first_sig: Option<String> = None;
        let mut first_type: Option<String> = None;

        for ctx in &workspace.contexts {
            let unit = ctx
                .graph
                .units()
                .iter()
                .find(|u| u.name.to_lowercase() == symbol_lower);

            match unit {
                Some(u) => {
                    let sig = u.signature.clone();
                    let utype = u.unit_type.label().to_string();
                    let fpath = u.file_path.display().to_string();

                    // Detect structural differences against the first occurrence.
                    if let Some(ref first) = first_sig {
                        if sig.as_deref().unwrap_or("") != first.as_str() {
                            structural_diff.push(format!(
                                "signature differs in {}: '{}' vs '{}'",
                                ctx.id,
                                sig.as_deref().unwrap_or("<none>"),
                                first,
                            ));
                        }
                    } else {
                        first_sig = Some(sig.as_deref().unwrap_or("").to_string());
                    }

                    if let Some(ref first) = first_type {
                        if utype != *first {
                            structural_diff.push(format!(
                                "type differs in {}: '{}' vs '{}'",
                                ctx.id, utype, first,
                            ));
                        }
                    } else {
                        first_type = Some(utype.clone());
                    }

                    ctx_comparisons.push(ContextComparison {
                        context_id: ctx.id.clone(),
                        role: ctx.role.clone(),
                        found: true,
                        unit_type: Some(utype),
                        signature: sig,
                        file_path: Some(fpath),
                    });
                }
                None => {
                    ctx_comparisons.push(ContextComparison {
                        context_id: ctx.id.clone(),
                        role: ctx.role.clone(),
                        found: false,
                        unit_type: None,
                        signature: None,
                        file_path: None,
                    });
                }
            }
        }

        Ok(Comparison {
            symbol: symbol.to_string(),
            contexts: ctx_comparisons,
            semantic_match: 0.0, // TODO: compute cosine similarity when vectors are populated
            structural_diff,
        })
    }

    /// Build a cross-reference report for a symbol across all contexts.
    ///
    /// Returns lists of contexts where the symbol was found and where it is
    /// missing (both annotated with their [`ContextRole`]).
    pub fn cross_reference(
        &self,
        workspace_id: &str,
        symbol: &str,
    ) -> Result<CrossReference, String> {
        let workspace = self
            .workspaces
            .get(workspace_id)
            .ok_or_else(|| format!("workspace '{}' not found", workspace_id))?;

        let symbol_lower = symbol.to_lowercase();
        let mut found_in = Vec::new();
        let mut missing_from = Vec::new();

        for ctx in &workspace.contexts {
            let exists = ctx
                .graph
                .units()
                .iter()
                .any(|u| u.name.to_lowercase() == symbol_lower);

            if exists {
                found_in.push((ctx.id.clone(), ctx.role.clone()));
            } else {
                missing_from.push((ctx.id.clone(), ctx.role.clone()));
            }
        }

        Ok(CrossReference {
            symbol: symbol.to_string(),
            found_in,
            missing_from,
        })
    }

    // -- Internal helpers ---------------------------------------------------

    /// Search a single graph for units whose name contains `query_lower`.
    ///
    /// `query_lower` must already be lowercased.
    fn search_graph(graph: &CodeGraph, query_lower: &str) -> Vec<SymbolMatch> {
        graph
            .units()
            .iter()
            .filter(|u| u.name.to_lowercase().contains(query_lower))
            .map(|u| SymbolMatch {
                unit_id: u.id,
                name: u.name.clone(),
                qualified_name: u.qualified_name.clone(),
                unit_type: u.unit_type.label().to_string(),
                file_path: u.file_path.display().to_string(),
            })
            .collect()
    }
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CodeUnit, CodeUnitType, Language, Span};
    use std::path::PathBuf;

    /// Build a tiny graph with one function unit for testing.
    fn make_graph(name: &str, sig: Option<&str>) -> CodeGraph {
        let mut g = CodeGraph::with_default_dimension();
        let mut unit = CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            name.to_string(),
            format!("crate::{}", name),
            PathBuf::from(format!("src/{}.rs", name)),
            Span::new(1, 0, 10, 0),
        );
        if let Some(s) = sig {
            unit.signature = Some(s.to_string());
        }
        g.add_unit(unit);
        g
    }

    #[test]
    fn create_workspace_sets_active() {
        let mut mgr = WorkspaceManager::new();
        let id = mgr.create("test-ws");
        assert_eq!(mgr.get_active(), Some(id.as_str()));
    }

    #[test]
    fn add_context_and_list() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create("migration");
        let ctx = mgr
            .add_context(&ws, "/src/cpp", ContextRole::Source, Some("C++".into()), make_graph("foo", None))
            .unwrap();
        assert!(ctx.starts_with("ctx-"));

        let workspace = mgr.list(&ws).unwrap();
        assert_eq!(workspace.contexts.len(), 1);
        assert_eq!(workspace.contexts[0].role, ContextRole::Source);
    }

    #[test]
    fn query_all_finds_symbol() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create("q");
        mgr.add_context(&ws, "/a", ContextRole::Source, None, make_graph("process", None)).unwrap();
        mgr.add_context(&ws, "/b", ContextRole::Target, None, make_graph("other", None)).unwrap();

        let results = mgr.query_all(&ws, "proc").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matches[0].name, "process");
    }

    #[test]
    fn query_context_single() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create("q2");
        let ctx = mgr
            .add_context(&ws, "/a", ContextRole::Source, None, make_graph("alpha", None))
            .unwrap();

        let matches = mgr.query_context(&ws, &ctx, "alph").unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "alpha");
    }

    #[test]
    fn compare_detects_signature_diff() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create("cmp");
        mgr.add_context(&ws, "/a", ContextRole::Source, None, make_graph("foo", Some("(int) -> bool"))).unwrap();
        mgr.add_context(&ws, "/b", ContextRole::Target, None, make_graph("foo", Some("(i32) -> bool"))).unwrap();

        let cmp = mgr.compare(&ws, "foo").unwrap();
        assert_eq!(cmp.contexts.len(), 2);
        assert!(cmp.contexts[0].found);
        assert!(cmp.contexts[1].found);
        assert!(!cmp.structural_diff.is_empty());
    }

    #[test]
    fn cross_reference_found_and_missing() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create("xref");
        mgr.add_context(&ws, "/a", ContextRole::Source, None, make_graph("bar", None)).unwrap();
        mgr.add_context(&ws, "/b", ContextRole::Target, None, make_graph("other", None)).unwrap();

        let xref = mgr.cross_reference(&ws, "bar").unwrap();
        assert_eq!(xref.found_in.len(), 1);
        assert_eq!(xref.missing_from.len(), 1);
    }

    #[test]
    fn context_role_roundtrip() {
        for label in &["source", "target", "reference", "comparison"] {
            let role = ContextRole::from_str(label).unwrap();
            assert_eq!(role.label(), *label);
        }
        assert!(ContextRole::from_str("invalid").is_none());
    }

    #[test]
    fn workspace_not_found_error() {
        let mgr = WorkspaceManager::new();
        assert!(mgr.list("ws-999").is_err());
        assert!(mgr.query_all("ws-999", "x").is_err());
    }
}
