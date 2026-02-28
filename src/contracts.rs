//! Contracts bridge — implements agentic-sdk v0.2.0 traits for Codebase.
//!
//! This module provides `CodebaseSister`, a contracts-compliant wrapper
//! around the core `CodeGraph` + grounding engine. It implements:
//!
//! - `Sister` — lifecycle management
//! - `WorkspaceManagement` — named multi-graph workspaces
//! - `Grounding` — code claim verification via existing GroundingEngine
//! - `Queryable` — unified query interface
//! - `FileFormatReader/FileFormatWriter` — .acb file I/O
//!
//! The MCP server can use `CodebaseSister` instead of raw graph + engine
//! to get compile-time contracts compliance.

use agentic_sdk::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::format::{AcbReader, AcbWriter};
use crate::graph::CodeGraph;
use crate::grounding::{self, Grounded};
use crate::types::{AcbError, CodeUnitType, FileHeader, DEFAULT_DIMENSION};

// ═══════════════════════════════════════════════════════════════════
// ERROR BRIDGE: AcbError → SisterError
// ═══════════════════════════════════════════════════════════════════

impl From<AcbError> for SisterError {
    fn from(e: AcbError) -> Self {
        match &e {
            AcbError::UnitNotFound(id) => SisterError::not_found(format!("code unit {}", id)),
            AcbError::PathNotFound(path) => {
                SisterError::not_found(format!("path {}", path.display()))
            }
            AcbError::InvalidMagic => {
                SisterError::new(ErrorCode::VersionMismatch, "Invalid .acb magic bytes")
            }
            AcbError::UnsupportedVersion(v) => SisterError::new(
                ErrorCode::VersionMismatch,
                format!("Unsupported .acb version: {}", v),
            ),
            AcbError::DimensionMismatch { expected, got } => SisterError::new(
                ErrorCode::InvalidInput,
                format!("Dimension mismatch: expected {}, got {}", expected, got),
            ),
            AcbError::UnsupportedLanguage(lang) => SisterError::new(
                ErrorCode::CodebaseError,
                format!("Unsupported language: {}", lang),
            ),
            AcbError::ParseError { path, message } => SisterError::new(
                ErrorCode::CodebaseError,
                format!("Parse error in {}: {}", path.display(), message),
            ),
            AcbError::SemanticError(msg) => {
                SisterError::new(ErrorCode::CodebaseError, format!("Semantic error: {}", msg))
            }
            AcbError::GitError(msg) => {
                SisterError::new(ErrorCode::CodebaseError, format!("Git error: {}", msg))
            }
            AcbError::Truncated => {
                SisterError::new(ErrorCode::StorageError, "File is empty or truncated")
            }
            AcbError::Corrupt(offset) => SisterError::new(
                ErrorCode::ChecksumMismatch,
                format!("Corrupt data at offset {}", offset),
            ),
            AcbError::Io(io_err) => {
                SisterError::new(ErrorCode::StorageError, format!("I/O error: {}", io_err))
            }
            AcbError::Compression(msg) => SisterError::new(
                ErrorCode::StorageError,
                format!("Compression error: {}", msg),
            ),
            _ => SisterError::new(ErrorCode::CodebaseError, e.to_string()),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// CODEBASE SISTER — The contracts-compliant facade
// ═══════════════════════════════════════════════════════════════════

/// Contracts-compliant Codebase sister.
///
/// Wraps `CodeGraph` and implements all v0.2.0 traits.
/// Uses workspace management (multiple named graphs) instead of sessions.
pub struct CodebaseSister {
    /// All loaded workspaces: context_id -> (name, graph)
    workspaces: HashMap<ContextId, (String, CodeGraph)>,
    /// Active workspace context
    active: Option<ContextId>,
    /// File path for the default graph
    file_path: Option<PathBuf>,
    start_time: Instant,
}

impl CodebaseSister {
    /// Create from an existing graph (for migration from existing code).
    pub fn from_graph(graph: CodeGraph, file_path: Option<PathBuf>) -> Self {
        let mut workspaces = HashMap::new();
        let default_id = ContextId::new();
        workspaces.insert(default_id, ("default".to_string(), graph));

        Self {
            workspaces,
            active: Some(default_id),
            file_path,
            start_time: Instant::now(),
        }
    }

    /// Get a reference to the active graph.
    pub fn active_graph(&self) -> Option<&CodeGraph> {
        self.active
            .and_then(|id| self.workspaces.get(&id))
            .map(|(_, g)| g)
    }

    /// Get a mutable reference to the active graph.
    pub fn active_graph_mut(&mut self) -> Option<&mut CodeGraph> {
        self.active
            .and_then(|id| self.workspaces.get_mut(&id))
            .map(|(_, g)| g)
    }

    /// Get a grounding engine for the active graph.
    fn grounding_engine(&self) -> Option<grounding::GroundingEngine<'_>> {
        self.active_graph().map(grounding::GroundingEngine::new)
    }
}

// ═══════════════════════════════════════════════════════════════════
// SISTER TRAIT
// ═══════════════════════════════════════════════════════════════════

impl Sister for CodebaseSister {
    const SISTER_TYPE: SisterType = SisterType::Codebase;
    const FILE_EXTENSION: &'static str = "acb";

    fn init(config: SisterConfig) -> SisterResult<Self>
    where
        Self: Sized,
    {
        let dimension = config
            .get_option::<usize>("dimension")
            .unwrap_or(DEFAULT_DIMENSION);

        let file_path = config.data_path.clone();

        let graph = if let Some(ref path) = file_path {
            if path.exists() {
                AcbReader::read_from_file(path).map_err(SisterError::from)?
            } else if config.create_if_missing {
                CodeGraph::new(dimension)
            } else {
                return Err(SisterError::new(
                    ErrorCode::NotFound,
                    format!("Codebase file not found: {}", path.display()),
                ));
            }
        } else {
            CodeGraph::new(dimension)
        };

        Ok(Self::from_graph(graph, file_path))
    }

    fn health(&self) -> HealthStatus {
        let (unit_count, edge_count) = self
            .active_graph()
            .map(|g| (g.unit_count(), g.edge_count()))
            .unwrap_or((0, 0));

        HealthStatus {
            healthy: true,
            status: Status::Ready,
            uptime: self.start_time.elapsed(),
            resources: ResourceUsage {
                memory_bytes: (unit_count * 512) + (edge_count * 64), // rough estimate
                disk_bytes: 0,
                open_handles: if self.file_path.is_some() { 1 } else { 0 },
            },
            warnings: vec![],
            last_error: None,
        }
    }

    fn version(&self) -> Version {
        Version::new(0, 3, 0) // matches agentic-codebase crate version
    }

    fn shutdown(&mut self) -> SisterResult<()> {
        // Save active graph to file if path is set
        if let (Some(ref path), Some(graph)) = (&self.file_path, self.active_graph()) {
            let writer = AcbWriter::new(graph.dimension());
            writer
                .write_to_file(graph, path)
                .map_err(SisterError::from)?;
        }

        self.workspaces.clear();
        self.active = None;
        Ok(())
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::new("symbol_lookup", "Look up symbols by name in the code graph"),
            Capability::new(
                "impact_analysis",
                "Analyse the impact of changing a code unit",
            ),
            Capability::new("list_units", "List code units filtered by type"),
            Capability::new(
                "graph_stats",
                "Get summary statistics about a loaded code graph",
            ),
            Capability::new(
                "codebase_ground",
                "Verify a claim about code has graph evidence",
            ),
            Capability::new("codebase_evidence", "Get graph evidence for a symbol name"),
            Capability::new("codebase_suggest", "Find symbols similar to a name"),
            Capability::new("workspace_create", "Create a multi-codebase workspace"),
            Capability::new(
                "workspace_query",
                "Search across all codebases in workspace",
            ),
            Capability::new(
                "analysis_log",
                "Log intent and context behind a code analysis",
            ),
        ]
    }
}

// ═══════════════════════════════════════════════════════════════════
// WORKSPACE MANAGEMENT
// ═══════════════════════════════════════════════════════════════════

impl WorkspaceManagement for CodebaseSister {
    fn create_workspace(&mut self, name: &str) -> SisterResult<ContextId> {
        let id = ContextId::new();
        let dimension = self
            .active_graph()
            .map(|g| g.dimension())
            .unwrap_or(DEFAULT_DIMENSION);
        self.workspaces
            .insert(id, (name.to_string(), CodeGraph::new(dimension)));
        Ok(id)
    }

    fn switch_workspace(&mut self, id: ContextId) -> SisterResult<()> {
        if !self.workspaces.contains_key(&id) {
            return Err(SisterError::context_not_found(id.to_string()));
        }
        self.active = Some(id);
        Ok(())
    }

    fn current_workspace(&self) -> ContextId {
        self.active.unwrap_or_else(ContextId::default_context)
    }

    fn current_workspace_info(&self) -> SisterResult<ContextInfo> {
        let active_id = self
            .active
            .ok_or_else(|| SisterError::new(ErrorCode::InvalidState, "No active workspace"))?;
        let (name, graph) = self
            .workspaces
            .get(&active_id)
            .ok_or_else(|| SisterError::context_not_found(active_id.to_string()))?;

        Ok(ContextInfo {
            id: active_id,
            name: name.clone(),
            created_at: chrono::Utc::now(), // approximate
            updated_at: chrono::Utc::now(),
            item_count: graph.unit_count(),
            size_bytes: graph.unit_count() * 512,
            metadata: Metadata::new(),
        })
    }

    fn list_workspaces(&self) -> SisterResult<Vec<ContextSummary>> {
        Ok(self
            .workspaces
            .iter()
            .map(|(id, (name, graph))| ContextSummary {
                id: *id,
                name: name.clone(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                item_count: graph.unit_count(),
                size_bytes: graph.unit_count() * 512,
            })
            .collect())
    }

    fn delete_workspace(&mut self, id: ContextId) -> SisterResult<()> {
        if !self.workspaces.contains_key(&id) {
            return Err(SisterError::context_not_found(id.to_string()));
        }
        // Cannot delete the active workspace
        if self.active == Some(id) {
            return Err(SisterError::new(
                ErrorCode::InvalidState,
                "Cannot delete the active workspace",
            ));
        }
        self.workspaces.remove(&id);
        Ok(())
    }

    fn rename_workspace(&mut self, id: ContextId, new_name: &str) -> SisterResult<()> {
        let (name, _) = self
            .workspaces
            .get_mut(&id)
            .ok_or_else(|| SisterError::context_not_found(id.to_string()))?;
        *name = new_name.to_string();
        Ok(())
    }

    fn export_workspace(&self, id: ContextId) -> SisterResult<ContextSnapshot> {
        let (name, graph) = self
            .workspaces
            .get(&id)
            .ok_or_else(|| SisterError::context_not_found(id.to_string()))?;

        let writer = AcbWriter::new(graph.dimension());
        let mut data = Vec::new();
        writer
            .write_to(graph, &mut data)
            .map_err(SisterError::from)?;
        let checksum = *blake3::hash(&data).as_bytes();

        Ok(ContextSnapshot {
            sister_type: SisterType::Codebase,
            version: Version::new(0, 3, 0),
            context_info: ContextInfo {
                id,
                name: name.clone(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                item_count: graph.unit_count(),
                size_bytes: data.len(),
                metadata: Metadata::new(),
            },
            data,
            checksum,
            snapshot_at: chrono::Utc::now(),
        })
    }

    fn import_workspace(&mut self, snapshot: ContextSnapshot) -> SisterResult<ContextId> {
        if !snapshot.verify() {
            return Err(SisterError::new(
                ErrorCode::ChecksumMismatch,
                "Workspace snapshot checksum verification failed",
            ));
        }

        let mut cursor = std::io::Cursor::new(&snapshot.data);
        let graph = AcbReader::read_from(&mut cursor).map_err(SisterError::from)?;

        let id = ContextId::new();
        self.workspaces
            .insert(id, (snapshot.context_info.name, graph));
        Ok(id)
    }
}

// ═══════════════════════════════════════════════════════════════════
// GROUNDING (bridges existing GroundingEngine to contracts trait)
// ═══════════════════════════════════════════════════════════════════

impl agentic_sdk::prelude::Grounding for CodebaseSister {
    fn ground(&self, claim: &str) -> SisterResult<GroundingResult> {
        let engine = self.grounding_engine().ok_or_else(|| {
            SisterError::new(ErrorCode::InvalidState, "No active graph for grounding")
        })?;

        let result = engine.ground_claim(claim);

        match result {
            grounding::GroundingResult::Verified {
                evidence,
                confidence,
            } => {
                let contract_evidence: Vec<GroundingEvidence> = evidence
                    .iter()
                    .map(|e| {
                        GroundingEvidence::new(
                            &e.node_type,
                            format!("unit_{}", e.node_id),
                            confidence as f64,
                            &e.name,
                        )
                        .with_data("file_path", e.file_path.clone())
                        .with_data("line_number", e.line_number)
                        .with_data("snippet", e.snippet.clone())
                    })
                    .collect();

                Ok(GroundingResult::verified(claim, confidence as f64)
                    .with_evidence(contract_evidence)
                    .with_reason("All code references found in graph"))
            }
            grounding::GroundingResult::Partial {
                supported,
                unsupported,
                suggestions,
            } => {
                let total = supported.len() + unsupported.len();
                let confidence = if total > 0 {
                    supported.len() as f64 / total as f64
                } else {
                    0.0
                };

                Ok(GroundingResult::partial(claim, confidence)
                    .with_suggestions(suggestions)
                    .with_reason(format!(
                        "Found {}/{} references. Missing: {}",
                        supported.len(),
                        total,
                        unsupported.join(", ")
                    )))
            }
            grounding::GroundingResult::Ungrounded {
                claim: _,
                suggestions,
            } => Ok(GroundingResult::ungrounded(claim, "No graph backing found")
                .with_suggestions(suggestions)),
        }
    }

    fn evidence(&self, query: &str, max_results: usize) -> SisterResult<Vec<EvidenceDetail>> {
        let engine = self.grounding_engine().ok_or_else(|| {
            SisterError::new(ErrorCode::InvalidState, "No active graph for evidence")
        })?;

        let evidence = engine.find_evidence(query);

        Ok(evidence
            .into_iter()
            .take(max_results)
            .map(|e| EvidenceDetail {
                evidence_type: e.node_type.clone(),
                id: format!("unit_{}", e.node_id),
                score: 1.0, // exact match from grounding engine
                created_at: chrono::Utc::now(),
                source_sister: SisterType::Codebase,
                content: format!("{} ({})", e.name, e.file_path),
                data: {
                    let mut meta = Metadata::new();
                    if let Ok(v) = serde_json::to_value(&e.file_path) {
                        meta.insert("file_path".to_string(), v);
                    }
                    if let Ok(v) = serde_json::to_value(e.line_number) {
                        meta.insert("line_number".to_string(), v);
                    }
                    if let Ok(v) = serde_json::to_value(&e.snippet) {
                        meta.insert("snippet".to_string(), v);
                    }
                    meta
                },
            })
            .collect())
    }

    fn suggest(&self, query: &str, limit: usize) -> SisterResult<Vec<GroundingSuggestion>> {
        let engine = self.grounding_engine().ok_or_else(|| {
            SisterError::new(ErrorCode::InvalidState, "No active graph for suggestions")
        })?;

        let suggestions = engine.suggest_similar(query, limit);

        Ok(suggestions
            .into_iter()
            .enumerate()
            .map(|(i, name)| GroundingSuggestion {
                item_type: "code_symbol".to_string(),
                id: format!("suggestion_{}", i),
                relevance_score: 1.0 - (i as f64 * 0.1), // decreasing relevance
                description: name,
                data: Metadata::new(),
            })
            .collect())
    }
}

// ═══════════════════════════════════════════════════════════════════
// QUERYABLE
// ═══════════════════════════════════════════════════════════════════

impl Queryable for CodebaseSister {
    fn query(&self, query: Query) -> SisterResult<QueryResult> {
        let start = Instant::now();
        let graph = self
            .active_graph()
            .ok_or_else(|| SisterError::new(ErrorCode::InvalidState, "No active graph"))?;

        let results: Vec<serde_json::Value> = match query.query_type.as_str() {
            "list" => {
                let limit = query.limit.unwrap_or(50);
                let offset = query.offset.unwrap_or(0);
                let type_filter = query.get_string("unit_type");

                let units = graph.units();
                let filtered: Box<dyn Iterator<Item = &crate::types::CodeUnit>> =
                    if let Some(ref type_str) = type_filter {
                        let ut = parse_unit_type(type_str);
                        Box::new(units.iter().filter(move |u| Some(u.unit_type) == ut))
                    } else {
                        Box::new(units.iter())
                    };

                filtered
                    .skip(offset)
                    .take(limit)
                    .map(|u| {
                        serde_json::json!({
                            "id": u.id,
                            "name": u.name,
                            "qualified_name": u.qualified_name,
                            "unit_type": format!("{:?}", u.unit_type),
                            "language": format!("{:?}", u.language),
                            "file_path": u.file_path,
                        })
                    })
                    .collect()
            }
            "search" => {
                let text = query.get_string("text").unwrap_or_default();
                let max = query.limit.unwrap_or(20);

                graph
                    .find_units_by_name(&text)
                    .into_iter()
                    .take(max)
                    .map(|u| {
                        serde_json::json!({
                            "id": u.id,
                            "name": u.name,
                            "qualified_name": u.qualified_name,
                            "unit_type": format!("{:?}", u.unit_type),
                            "file_path": u.file_path,
                        })
                    })
                    .collect()
            }
            "get" => {
                let id_str = query.get_string("id").unwrap_or_default();
                let id: u64 = id_str.parse().unwrap_or(0);
                if let Some(u) = graph.get_unit(id) {
                    vec![serde_json::json!({
                        "id": u.id,
                        "name": u.name,
                        "qualified_name": u.qualified_name,
                        "unit_type": format!("{:?}", u.unit_type),
                        "language": format!("{:?}", u.language),
                        "file_path": u.file_path,
                        "span": { "start_line": u.span.start_line, "end_line": u.span.end_line },
                    })]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        };

        let total = graph.unit_count();
        let has_more = results.len() < total;

        Ok(QueryResult::new(query, results, start.elapsed()).with_pagination(total, has_more))
    }

    fn supports_query(&self, query_type: &str) -> bool {
        matches!(query_type, "list" | "search" | "get")
    }

    fn query_types(&self) -> Vec<QueryTypeInfo> {
        vec![
            QueryTypeInfo::new("list", "List code units with optional type filter").optional(vec![
                "limit",
                "offset",
                "unit_type",
            ]),
            QueryTypeInfo::new("search", "Search code units by name prefix")
                .required(vec!["text"])
                .optional(vec!["limit"]),
            QueryTypeInfo::new("get", "Get a specific code unit by ID").required(vec!["id"]),
        ]
    }
}

/// Parse a unit type string to CodeUnitType.
fn parse_unit_type(s: &str) -> Option<CodeUnitType> {
    match s.to_lowercase().as_str() {
        "module" => Some(CodeUnitType::Module),
        "function" => Some(CodeUnitType::Function),
        "type" => Some(CodeUnitType::Type),
        "symbol" => Some(CodeUnitType::Symbol),
        "import" => Some(CodeUnitType::Import),
        "parameter" => Some(CodeUnitType::Parameter),
        "test" => Some(CodeUnitType::Test),
        "doc" => Some(CodeUnitType::Doc),
        "config" => Some(CodeUnitType::Config),
        "pattern" => Some(CodeUnitType::Pattern),
        "trait" => Some(CodeUnitType::Trait),
        "impl" => Some(CodeUnitType::Impl),
        "macro" => Some(CodeUnitType::Macro),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════
// FILE FORMAT
// ═══════════════════════════════════════════════════════════════════

impl FileFormatReader for CodebaseSister {
    fn read_file(path: &Path) -> SisterResult<Self> {
        let graph = AcbReader::read_from_file(path).map_err(SisterError::from)?;
        Ok(Self::from_graph(graph, Some(path.to_path_buf())))
    }

    fn can_read(path: &Path) -> SisterResult<FileInfo> {
        let mut file = std::fs::File::open(path)
            .map_err(|e| SisterError::new(ErrorCode::StorageError, e.to_string()))?;
        let header = FileHeader::read_from(&mut file).map_err(SisterError::from)?;

        let metadata = std::fs::metadata(path)
            .map_err(|e| SisterError::new(ErrorCode::StorageError, e.to_string()))?;

        Ok(FileInfo {
            sister_type: SisterType::Codebase,
            version: Version::new(header.version as u8, 0, 0),
            created_at: chrono::Utc::now(),
            updated_at: chrono::DateTime::from(
                metadata
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            ),
            content_length: metadata.len(),
            needs_migration: header.version < 1,
            format_id: "ACDB".to_string(),
        })
    }

    fn file_version(path: &Path) -> SisterResult<Version> {
        let mut file = std::fs::File::open(path)
            .map_err(|e| SisterError::new(ErrorCode::StorageError, e.to_string()))?;
        let header = FileHeader::read_from(&mut file).map_err(SisterError::from)?;
        Ok(Version::new(header.version as u8, 0, 0))
    }

    fn migrate(_data: &[u8], _from_version: Version) -> SisterResult<Vec<u8>> {
        Err(SisterError::new(
            ErrorCode::NotImplemented,
            "No migration path available (only v1 exists)",
        ))
    }
}

impl FileFormatWriter for CodebaseSister {
    fn write_file(&self, path: &Path) -> SisterResult<()> {
        let graph = self
            .active_graph()
            .ok_or_else(|| SisterError::new(ErrorCode::InvalidState, "No active graph to write"))?;
        let writer = AcbWriter::new(graph.dimension());
        writer.write_to_file(graph, path).map_err(SisterError::from)
    }

    fn to_bytes(&self) -> SisterResult<Vec<u8>> {
        let graph = self.active_graph().ok_or_else(|| {
            SisterError::new(ErrorCode::InvalidState, "No active graph to serialize")
        })?;
        let writer = AcbWriter::new(graph.dimension());
        let mut buffer = Vec::new();
        writer
            .write_to(graph, &mut buffer)
            .map_err(SisterError::from)?;
        Ok(buffer)
    }
}

// ═══════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CodeUnitBuilder, Language, Span};

    fn make_test_sister() -> CodebaseSister {
        let config = SisterConfig::stateless().option("dimension", DEFAULT_DIMENSION);
        CodebaseSister::init(config).unwrap()
    }

    fn add_test_units(sister: &mut CodebaseSister) {
        let graph = sister.active_graph_mut().unwrap();
        graph.add_unit(
            CodeUnitBuilder::new(
                CodeUnitType::Function,
                Language::Rust,
                "process_payment",
                "payments::process_payment",
                "src/payments.rs",
                Span::new(10, 0, 50, 0),
            )
            .build(),
        );
        graph.add_unit(
            CodeUnitBuilder::new(
                CodeUnitType::Type,
                Language::Rust,
                "PaymentResult",
                "payments::PaymentResult",
                "src/payments.rs",
                Span::new(1, 0, 8, 0),
            )
            .build(),
        );
        graph.add_unit(
            CodeUnitBuilder::new(
                CodeUnitType::Function,
                Language::Rust,
                "validate_amount",
                "payments::validate_amount",
                "src/payments.rs",
                Span::new(55, 0, 70, 0),
            )
            .build(),
        );
    }

    #[test]
    fn test_sister_trait() {
        let sister = make_test_sister();
        assert_eq!(sister.sister_type(), SisterType::Codebase);
        assert_eq!(sister.file_extension(), "acb");
        assert_eq!(sister.mcp_prefix(), "codebase");
        assert!(sister.is_healthy());
        assert_eq!(sister.version(), Version::new(0, 3, 0));
        assert!(!sister.capabilities().is_empty());
    }

    #[test]
    fn test_workspace_management() {
        let mut sister = make_test_sister();

        // Default workspace exists
        let default_id = sister.current_workspace();
        assert!(!default_id.is_default());
        let info = sister.current_workspace_info().unwrap();
        assert_eq!(info.name, "default");

        // Create a new workspace
        let new_id = sister.create_workspace("feature_branch").unwrap();
        let workspaces = sister.list_workspaces().unwrap();
        assert_eq!(workspaces.len(), 2);

        // Switch to new workspace
        sister.switch_workspace(new_id).unwrap();
        assert_eq!(sister.current_workspace(), new_id);

        // Switch back
        sister.switch_workspace(default_id).unwrap();

        // Delete non-active workspace
        sister.delete_workspace(new_id).unwrap();
        let workspaces = sister.list_workspaces().unwrap();
        assert_eq!(workspaces.len(), 1);

        // Cannot delete active workspace
        assert!(sister.delete_workspace(default_id).is_err());
    }

    #[test]
    fn test_grounding() {
        let mut sister = make_test_sister();
        add_test_units(&mut sister);

        // Ground a claim with known symbols
        let result = agentic_sdk::prelude::Grounding::ground(&sister, "process_payment").unwrap();
        // Should find the function
        assert!(
            result.status == GroundingStatus::Verified || result.status == GroundingStatus::Partial,
            "Expected verified or partial, got {:?}",
            result.status
        );

        // Ground a claim with unknown symbol
        let result =
            agentic_sdk::prelude::Grounding::ground(&sister, "totally_fake_function_xyz").unwrap();
        assert_eq!(result.status, GroundingStatus::Ungrounded);
    }

    #[test]
    fn test_evidence() {
        let mut sister = make_test_sister();
        add_test_units(&mut sister);

        let evidence =
            agentic_sdk::prelude::Grounding::evidence(&sister, "process_payment", 10).unwrap();
        assert!(
            !evidence.is_empty(),
            "Expected evidence for 'process_payment'"
        );
        assert_eq!(evidence[0].source_sister, SisterType::Codebase);
    }

    #[test]
    fn test_suggest() {
        let mut sister = make_test_sister();
        add_test_units(&mut sister);

        let suggestions =
            agentic_sdk::prelude::Grounding::suggest(&sister, "process_pay", 5).unwrap();
        // Should suggest "process_payment" as a similar name
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_queryable_list() {
        let mut sister = make_test_sister();
        add_test_units(&mut sister);

        let result = sister.query(Query::list().limit(2)).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.has_more);
    }

    #[test]
    fn test_queryable_search() {
        let mut sister = make_test_sister();
        add_test_units(&mut sister);

        let result = sister.search("process").unwrap();
        assert!(!result.is_empty(), "Expected search results for 'process'");
    }

    #[test]
    fn test_queryable_types() {
        let sister = make_test_sister();
        assert!(sister.supports_query("list"));
        assert!(sister.supports_query("search"));
        assert!(sister.supports_query("get"));
        assert!(!sister.supports_query("recent"));

        let types = sister.query_types();
        assert_eq!(types.len(), 3);
    }

    #[test]
    fn test_error_bridge() {
        let err = AcbError::UnitNotFound(42);
        let sister_err: SisterError = err.into();
        assert_eq!(sister_err.code, ErrorCode::NotFound);
        assert!(sister_err.message.contains("42"));

        let err2 = AcbError::InvalidMagic;
        let sister_err2: SisterError = err2.into();
        assert_eq!(sister_err2.code, ErrorCode::VersionMismatch);

        let err3 = AcbError::UnsupportedLanguage("Fortran".to_string());
        let sister_err3: SisterError = err3.into();
        assert_eq!(sister_err3.code, ErrorCode::CodebaseError);
    }

    #[test]
    fn test_config_patterns() {
        let config = SisterConfig::new("/tmp/test.acb");
        let sister = CodebaseSister::init(config).unwrap();
        assert!(sister.is_healthy());

        let config2 = SisterConfig::stateless();
        let sister2 = CodebaseSister::init(config2).unwrap();
        assert!(sister2.is_healthy());
    }

    #[test]
    fn test_shutdown() {
        let mut sister = make_test_sister();
        sister.shutdown().unwrap();
        assert!(sister.active_graph().is_none());
    }

    #[test]
    fn test_file_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.acb");

        let mut sister = make_test_sister();
        add_test_units(&mut sister);

        // Write
        sister.write_file(&path).unwrap();

        // Read back
        let sister2 = CodebaseSister::read_file(&path).unwrap();
        assert_eq!(sister2.active_graph().unwrap().unit_count(), 3);

        // Can read check
        let info = CodebaseSister::can_read(&path).unwrap();
        assert_eq!(info.sister_type, SisterType::Codebase);
        assert_eq!(info.format_id, "ACDB");
    }
}
