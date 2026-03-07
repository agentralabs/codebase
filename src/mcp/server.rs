//! MCP server implementation.
//!
//! Synchronous JSON-RPC 2.0 server that exposes code graph operations
//! through the Model Context Protocol. All operations are in-process
//! with no async runtime required.

use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::engine::query::{ImpactParams, MatchMode, SymbolLookupParams};
use crate::engine::QueryEngine;
use crate::format::reader::AcbReader;
use crate::graph::CodeGraph;
use crate::grounding::{Grounded, GroundingEngine, GroundingResult};
use crate::types::{CodeUnitType, EdgeType};
use crate::workspace::{ContextRole, TranslationMap, TranslationStatus, WorkspaceManager};

use super::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

/// Inject token conservation parameters into every tool's inputSchema.
fn inject_token_conservation_params(tools: &mut [Value]) {
    let conservation_props = json!({
        "include_content": { "type": "boolean", "default": false, "description": "Return full content (default: IDs only)" },
        "intent": { "type": "string", "enum": ["exists", "ids", "summary", "fields", "full"], "description": "Extraction intent level" },
        "since": { "type": "integer", "description": "Only return changes since this Unix timestamp" },
        "token_budget": { "type": "integer", "description": "Maximum token budget for response" },
        "max_results": { "type": "integer", "default": 10, "description": "Maximum number of results" },
        "cursor": { "type": "string", "description": "Pagination cursor for next page" }
    });
    for tool in tools.iter_mut() {
        if let Some(schema) = tool.get_mut("inputSchema") {
            if let Some(props) = schema.get_mut("properties") {
                if let Some(props_obj) = props.as_object_mut() {
                    if let Some(conservation_obj) = conservation_props.as_object() {
                        for (k, v) in conservation_obj {
                            props_obj.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                    }
                }
            }
        }
    }
}

/// MCP server capability information.
const SERVER_NAME: &str = "agentic-codebase";
/// MCP server version.
const SERVER_VERSION: &str = "0.1.0";
/// MCP protocol version supported.
const PROTOCOL_VERSION: &str = "2024-11-05";

/// Record of a tool call or analysis context entry.
#[derive(Debug, Clone)]
pub struct OperationRecord {
    pub tool_name: String,
    pub summary: String,
    pub timestamp: u64,
    pub graph_name: Option<String>,
}

/// A synchronous MCP server that handles JSON-RPC 2.0 messages.
///
/// Holds loaded code graphs and dispatches tool/resource/prompt requests
/// to the appropriate handler.
#[derive(Debug)]
pub struct McpServer {
    /// Loaded code graphs keyed by name.
    graphs: HashMap<String, CodeGraph>,
    /// Query engine for executing queries.
    engine: QueryEngine,
    /// Whether the server has been initialised.
    initialized: bool,
    /// Log of operations with context for this session.
    operation_log: Vec<OperationRecord>,
    /// Timestamp when this session started.
    session_start_time: Option<u64>,
    /// Multi-context workspace manager.
    workspace_manager: WorkspaceManager,
    /// Translation maps keyed by workspace ID.
    translation_maps: HashMap<String, TranslationMap>,
    /// Deferred graph path for lazy loading on first tool call.
    deferred_graph: Option<(String, String)>,
}

impl McpServer {
    fn parse_unit_type(raw: &str) -> Option<CodeUnitType> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "module" | "modules" => Some(CodeUnitType::Module),
            "symbol" | "symbols" => Some(CodeUnitType::Symbol),
            "type" | "types" => Some(CodeUnitType::Type),
            "function" | "functions" => Some(CodeUnitType::Function),
            "parameter" | "parameters" => Some(CodeUnitType::Parameter),
            "import" | "imports" => Some(CodeUnitType::Import),
            "test" | "tests" => Some(CodeUnitType::Test),
            "doc" | "docs" | "document" | "documents" => Some(CodeUnitType::Doc),
            "config" | "configs" => Some(CodeUnitType::Config),
            "pattern" | "patterns" => Some(CodeUnitType::Pattern),
            "trait" | "traits" => Some(CodeUnitType::Trait),
            "impl" | "implementation" | "implementations" => Some(CodeUnitType::Impl),
            "macro" | "macros" => Some(CodeUnitType::Macro),
            _ => None,
        }
    }

    /// Create a new MCP server with no loaded graphs.
    pub fn new() -> Self {
        Self {
            graphs: HashMap::new(),
            engine: QueryEngine::new(),
            initialized: false,
            operation_log: Vec::new(),
            session_start_time: None,
            workspace_manager: WorkspaceManager::new(),
            translation_maps: HashMap::new(),
            deferred_graph: None,
        }
    }

    /// Load a code graph into the server under the given name.
    pub fn load_graph(&mut self, name: String, graph: CodeGraph) {
        self.graphs.insert(name, graph);
    }

    /// Set a deferred graph path for lazy loading.
    ///
    /// If no graphs are loaded when a tool is called, the server will
    /// attempt to load this graph automatically. This provides a safety
    /// net when startup auto-resolve fails (e.g. wrong CWD).
    pub fn set_deferred_graph(&mut self, name: String, path: String) {
        self.deferred_graph = Some((name, path));
    }

    /// Attempt to lazy-load the deferred graph. Called automatically
    /// before tool dispatch when no graphs are loaded.
    fn try_lazy_load(&mut self) {
        if let Some((name, path)) = self.deferred_graph.take() {
            match AcbReader::read_from_file(Path::new(&path)) {
                Ok(graph) => {
                    self.graphs.insert(name, graph);
                }
                Err(_) => {
                    // Re-store the deferred path so we don't lose it
                    self.deferred_graph = Some((name, path));
                }
            }
        }
    }

    /// Remove a loaded code graph.
    pub fn unload_graph(&mut self, name: &str) -> Option<CodeGraph> {
        self.graphs.remove(name)
    }

    /// Get a reference to a loaded graph by name.
    pub fn get_graph(&self, name: &str) -> Option<&CodeGraph> {
        self.graphs.get(name)
    }

    /// List all loaded graph names.
    pub fn graph_names(&self) -> Vec<&str> {
        self.graphs.keys().map(|s| s.as_str()).collect()
    }

    /// Check if the server has been initialised.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Handle a raw JSON-RPC message string.
    ///
    /// Parses the message, dispatches to the appropriate handler, and
    /// returns the serialised JSON-RPC response.
    pub fn handle_raw(&mut self, raw: &str) -> String {
        let response = match super::protocol::parse_request(raw) {
            Ok(request) => {
                if request.id.is_none() {
                    self.handle_notification(&request.method, &request.params);
                    return String::new();
                }
                self.handle_request(request)
            }
            Err(error_response) => error_response,
        };
        serde_json::to_string(&response).unwrap_or_else(|_| {
            r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Serialization failed"}}"#
                .to_string()
        })
    }

    /// Handle a parsed JSON-RPC request.
    pub fn handle_request(&mut self, request: JsonRpcRequest) -> JsonRpcResponse {
        let id = request.id.clone().unwrap_or(Value::Null);
        match request.method.as_str() {
            "initialize" => self.handle_initialize(id, &request.params),
            "shutdown" => self.handle_shutdown(id),
            "tools/list" => self.handle_tools_list(id),
            "tools/call" => self.handle_tools_call(id, &request.params),
            "resources/list" => self.handle_resources_list(id),
            "resources/read" => self.handle_resources_read(id, &request.params),
            "prompts/list" => self.handle_prompts_list(id),
            _ => JsonRpcResponse::error(id, JsonRpcError::method_not_found(&request.method)),
        }
    }

    /// Handle JSON-RPC notifications (messages without an `id`).
    ///
    /// Notification methods intentionally produce no response frame.
    fn handle_notification(&mut self, method: &str, _params: &Value) {
        if method == "notifications/initialized" {
            self.initialized = true;
        }
    }

    // ========================================================================
    // Method handlers
    // ========================================================================

    /// Handle the "initialize" method.
    fn handle_initialize(&mut self, id: Value, _params: &Value) -> JsonRpcResponse {
        self.initialized = true;
        self.session_start_time = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
        self.operation_log.clear();
        JsonRpcResponse::success(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": false, "listChanged": false },
                    "prompts": { "listChanged": false }
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION
                }
            }),
        )
    }

    /// Handle the "shutdown" method.
    fn handle_shutdown(&mut self, id: Value) -> JsonRpcResponse {
        self.initialized = false;
        JsonRpcResponse::success(id, json!(null))
    }

    /// Handle "tools/list".
    fn handle_tools_list(&self, id: Value) -> JsonRpcResponse {
        if mcp_tool_surface_is_compact() {
            return JsonRpcResponse::success(
                id,
                json!({
                    "tools": compact_tool_definitions()
                }),
            );
        }

        let mut tools_array = json!([
            {
                "name": "symbol_lookup",
                "description": "Look up symbols by name in the code graph",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "name": { "type": "string", "description": "Symbol name to search for" },
                        "mode": { "type": "string", "enum": ["exact", "prefix", "contains", "fuzzy"], "default": "prefix" },
                        "limit": { "type": "integer", "minimum": 1, "default": 10 }
                    },
                    "required": ["name"]
                }
            },
            {
                "name": "impact_analysis",
                "description": "Analyse the impact of changing a code unit",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "unit_id": { "type": "integer", "description": "Code unit ID to analyse" },
                        "max_depth": { "type": "integer", "minimum": 0, "default": 3 }
                    },
                    "required": ["unit_id"]
                }
            },
            {
                "name": "graph_stats",
                "description": "Get summary statistics about a loaded code graph",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" }
                    }
                }
            },
            {
                "name": "list_units",
                "description": "List code units in a graph, optionally filtered by type",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "unit_type": {
                            "type": "string",
                            "description": "Filter by unit type",
                            "enum": [
                                "module", "symbol", "type", "function", "parameter", "import",
                                "test", "doc", "config", "pattern", "trait", "impl", "macro"
                            ]
                        },
                        "limit": { "type": "integer", "default": 50 }
                    }
                }
            },
            {
                "name": "analysis_log",
                "description": "Log the intent and context behind a code analysis. Call this to record WHY you are performing a lookup or analysis",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "intent": {
                            "type": "string",
                            "description": "Why you are analysing — the goal or reason for the code query"
                        },
                        "finding": {
                            "type": "string",
                            "description": "What you found or concluded from the analysis"
                        },
                        "graph": {
                            "type": "string",
                            "description": "Optional graph name this analysis relates to"
                        },
                        "topic": {
                            "type": "string",
                            "description": "Optional topic or category (e.g., 'refactoring', 'bug-hunt')"
                        }
                    },
                    "required": ["intent"]
                }
            },
            {
                "name": "session_start",
                "description": "Start a new codebase interaction session",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "integer", "description": "Optional explicit session ID" },
                        "metadata": { "type": "object", "description": "Optional session metadata" }
                    }
                }
            },
            {
                "name": "session_end",
                "description": "End the current codebase interaction session",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "integer", "description": "Optional explicit session ID" },
                        "summary": { "type": "string", "description": "Optional session summary" }
                    }
                }
            },
            {
                "name": "codebase_session_resume",
                "description": "Load context from previous codebase interactions",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "Maximum number of recent tool calls", "default": 5 }
                    }
                }
            },
            // ── Grounding tools ──────────────────────────────────
            {
                "name": "codebase_ground",
                "description": "Verify a claim about code has graph evidence. Use before asserting code exists",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "claim": { "type": "string", "description": "The claim to verify (e.g., 'function validate_token exists')" },
                        "graph": { "type": "string", "description": "Graph name" },
                        "strict": { "type": "boolean", "description": "If true, partial matches return Ungrounded (default: false)", "default": false }
                    },
                    "required": ["claim"]
                }
            },
            {
                "name": "codebase_evidence",
                "description": "Get graph evidence for a symbol name",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Symbol name to find" },
                        "graph": { "type": "string", "description": "Graph name" },
                        "types": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Filter by type: function, struct, enum, module, trait (optional)"
                        }
                    },
                    "required": ["name"]
                }
            },
            {
                "name": "codebase_suggest",
                "description": "Find symbols similar to a name (for corrections)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Name to find similar matches for" },
                        "graph": { "type": "string", "description": "Graph name" },
                        "limit": { "type": "integer", "minimum": 1, "default": 5, "description": "Max suggestions (default: 5)" }
                    },
                    "required": ["name"]
                }
            },
            // ── Workspace tools ──────────────────────────────────
            {
                "name": "workspace_create",
                "description": "Create a workspace to load multiple codebases",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Workspace name (e.g., 'cpp-to-rust-migration')" }
                    },
                    "required": ["name"]
                }
            },
            {
                "name": "workspace_add",
                "description": "Add a codebase to an existing workspace",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "workspace": { "type": "string", "description": "Workspace name or id" },
                        "graph": { "type": "string", "description": "Name of a loaded graph to add" },
                        "path": { "type": "string", "description": "Path label for this codebase" },
                        "role": { "type": "string", "enum": ["source", "target", "reference", "comparison"], "description": "Role of this codebase" },
                        "language": { "type": "string", "description": "Optional language hint" }
                    },
                    "required": ["workspace", "graph", "role"]
                }
            },
            {
                "name": "workspace_list",
                "description": "List all contexts in a workspace",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "workspace": { "type": "string", "description": "Workspace name or id" }
                    },
                    "required": ["workspace"]
                }
            },
            {
                "name": "workspace_query",
                "description": "Search across all codebases in workspace",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "workspace": { "type": "string", "description": "Workspace name or id" },
                        "query": { "type": "string", "description": "Search query" },
                        "roles": { "type": "array", "items": { "type": "string" }, "description": "Filter by role (optional)" }
                    },
                    "required": ["workspace", "query"]
                }
            },
            {
                "name": "workspace_compare",
                "description": "Compare a symbol between source and target",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "workspace": { "type": "string", "description": "Workspace name or id" },
                        "symbol": { "type": "string", "description": "Symbol to compare" }
                    },
                    "required": ["workspace", "symbol"]
                }
            },
            {
                "name": "workspace_xref",
                "description": "Find where symbol exists/doesn't exist across contexts",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "workspace": { "type": "string", "description": "Workspace name or id" },
                        "symbol": { "type": "string", "description": "Symbol to find" }
                    },
                    "required": ["workspace", "symbol"]
                }
            },
            // ── Translation tools ────────────────────────────────
            {
                "name": "translation_record",
                "description": "Record source→target symbol mapping",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "workspace": { "type": "string", "description": "Workspace name or id" },
                        "source_symbol": { "type": "string", "description": "Symbol in source codebase" },
                        "target_symbol": { "type": "string", "description": "Symbol in target (null if not ported)" },
                        "status": { "type": "string", "enum": ["not_started", "in_progress", "ported", "verified", "skipped"], "description": "Porting status" },
                        "notes": { "type": "string", "description": "Optional notes" }
                    },
                    "required": ["workspace", "source_symbol", "status"]
                }
            },
            {
                "name": "translation_progress",
                "description": "Get migration progress statistics",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "workspace": { "type": "string", "description": "Workspace name or id" }
                    },
                    "required": ["workspace"]
                }
            },
            {
                "name": "translation_remaining",
                "description": "List symbols not yet ported",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "workspace": { "type": "string", "description": "Workspace name or id" },
                        "module": { "type": "string", "description": "Filter by module (optional)" }
                    },
                    "required": ["workspace"]
                }
            },
            // ── Invention 1: Enhanced Impact Analysis ────────────
            {
                "name": "impact_analyze",
                "description": "Analyze the full impact of a proposed code change with blast radius and risk assessment",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "unit_id": { "type": "integer", "description": "Target code unit ID" },
                        "change_type": { "type": "string", "enum": ["signature", "behavior", "deletion", "rename", "move"], "default": "behavior" },
                        "max_depth": { "type": "integer", "minimum": 1, "default": 5 }
                    },
                    "required": ["unit_id"]
                }
            },
            {
                "name": "impact_path",
                "description": "Find the impact path between two code units",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "from": { "type": "integer", "description": "Source unit ID" },
                        "to": { "type": "integer", "description": "Target unit ID" }
                    },
                    "required": ["from", "to"]
                }
            },
            // ── Invention 2: Enhanced Code Prophecy ──────────────
            {
                "name": "prophecy",
                "description": "Predict the future of a code unit based on history, complexity, and dependencies",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "unit_id": { "type": "integer", "description": "Code unit ID to predict" }
                    },
                    "required": ["unit_id"]
                }
            },
            {
                "name": "prophecy_if",
                "description": "What-if scenario: predict impact of a hypothetical change",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "unit_id": { "type": "integer", "description": "Code unit ID" },
                        "change_type": { "type": "string", "enum": ["signature", "behavior", "deletion", "rename", "move"], "default": "behavior" }
                    },
                    "required": ["unit_id"]
                }
            },
            // ── Invention 3: Regression Oracle ───────────────────
            {
                "name": "regression_predict",
                "description": "Predict which tests are most likely affected by a change",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "unit_id": { "type": "integer", "description": "Changed code unit ID" },
                        "max_depth": { "type": "integer", "minimum": 1, "default": 5 }
                    },
                    "required": ["unit_id"]
                }
            },
            {
                "name": "regression_minimal",
                "description": "Get the minimal test set needed for a change",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "unit_id": { "type": "integer", "description": "Changed code unit ID" },
                        "threshold": { "type": "number", "description": "Minimum probability threshold (0.0-1.0)", "default": 0.5 }
                    },
                    "required": ["unit_id"]
                }
            },
            // ── Invention 4: Citation Engine ─────────────────────
            {
                "name": "codebase_ground_claim",
                "description": "Ground a claim with full citations including file locations and code snippets",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "claim": { "type": "string", "description": "The claim to verify and cite" }
                    },
                    "required": ["claim"]
                }
            },
            {
                "name": "codebase_cite",
                "description": "Get a citation for a specific code unit",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "unit_id": { "type": "integer", "description": "Code unit ID to cite" }
                    },
                    "required": ["unit_id"]
                }
            },
            // ── Invention 5: Hallucination Detector ──────────────
            {
                "name": "hallucination_check",
                "description": "Check AI-generated output for hallucinations about code",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "output": { "type": "string", "description": "AI-generated text to check" }
                    },
                    "required": ["output"]
                }
            },
            // ── Invention 6: Truth Maintenance ───────────────────
            {
                "name": "truth_register",
                "description": "Register a truth claim for ongoing maintenance",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "claim": { "type": "string", "description": "The truth claim to maintain" }
                    },
                    "required": ["claim"]
                }
            },
            {
                "name": "truth_check",
                "description": "Check if a registered truth is still valid",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "claim": { "type": "string", "description": "The truth claim to check" }
                    },
                    "required": ["claim"]
                }
            },
            // ── Invention 7: Concept Navigation ──────────────────
            {
                "name": "concept_find",
                "description": "Find code implementing a concept (e.g., authentication, payment)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "concept": { "type": "string", "description": "Concept to find (e.g., 'authentication', 'payment')" }
                    },
                    "required": ["concept"]
                }
            },
            {
                "name": "concept_map",
                "description": "Map all detected concepts in the codebase",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" }
                    }
                }
            },
            {
                "name": "concept_explain",
                "description": "Explain how a concept is implemented with details",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "concept": { "type": "string", "description": "Concept to explain" }
                    },
                    "required": ["concept"]
                }
            },
            // ── Invention 8: Architecture Inference ──────────────
            {
                "name": "architecture_infer",
                "description": "Infer the architecture pattern of the codebase",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" }
                    }
                }
            },
            {
                "name": "architecture_validate",
                "description": "Validate the codebase against its inferred architecture",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" }
                    }
                }
            },
            // ── Invention 9: Semantic Search ─────────────────────
            {
                "name": "search_semantic",
                "description": "Natural-language semantic search across the codebase",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "query": { "type": "string", "description": "Natural-language search query" },
                        "top_k": { "type": "integer", "minimum": 1, "default": 10 }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "search_similar",
                "description": "Find code units similar to a given unit",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "unit_id": { "type": "integer", "description": "Unit ID to find similar units for" },
                        "top_k": { "type": "integer", "minimum": 1, "default": 10 }
                    },
                    "required": ["unit_id"]
                }
            },
            {
                "name": "search_explain",
                "description": "Explain why a unit matched a search query",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "unit_id": { "type": "integer", "description": "Unit ID" },
                        "query": { "type": "string", "description": "The search query" }
                    },
                    "required": ["unit_id", "query"]
                }
            },
            // ── Invention 10: Multi-Codebase Compare ─────────────
            {
                "name": "compare_codebases",
                "description": "Full structural, conceptual, and pattern comparison between two codebases in a workspace",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "workspace": { "type": "string", "description": "Workspace name or id" }
                    },
                    "required": ["workspace"]
                }
            },
            {
                "name": "compare_concept",
                "description": "Compare how a concept is implemented across two codebases",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "workspace": { "type": "string", "description": "Workspace name or id" },
                        "concept": { "type": "string", "description": "Concept to compare (e.g., 'authentication')" }
                    },
                    "required": ["workspace", "concept"]
                }
            },
            {
                "name": "compare_migrate",
                "description": "Generate a migration plan from source to target codebase",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "workspace": { "type": "string", "description": "Workspace name or id" }
                    },
                    "required": ["workspace"]
                }
            },
            // ── Invention 11: Version Archaeology ────────────────
            {
                "name": "archaeology_node",
                "description": "Investigate the full history and evolution of a code unit",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "unit_id": { "type": "integer", "description": "Code unit ID to investigate" }
                    },
                    "required": ["unit_id"]
                }
            },
            {
                "name": "archaeology_why",
                "description": "Explain why code looks the way it does based on its history",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "unit_id": { "type": "integer", "description": "Code unit ID" }
                    },
                    "required": ["unit_id"]
                }
            },
            {
                "name": "archaeology_when",
                "description": "Get the timeline of changes for a code unit",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "unit_id": { "type": "integer", "description": "Code unit ID" }
                    },
                    "required": ["unit_id"]
                }
            },
            // ── Invention 12: Pattern Extraction ─────────────────
            {
                "name": "pattern_extract",
                "description": "Extract all detected patterns from the codebase",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" }
                    }
                }
            },
            {
                "name": "pattern_check",
                "description": "Check a code unit against detected patterns for violations",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "unit_id": { "type": "integer", "description": "Code unit ID to check" }
                    },
                    "required": ["unit_id"]
                }
            },
            {
                "name": "pattern_suggest",
                "description": "Suggest patterns for new code based on file location",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "graph": { "type": "string", "description": "Graph name" },
                        "file_path": { "type": "string", "description": "File path for pattern suggestions" }
                    },
                    "required": ["file_path"]
                }
            },
            // -- Invention 13: Code Resurrection --------------------
            { "name": "resurrect_search", "description": "Search for traces of deleted code", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" }, "query": { "type": "string", "description": "Search query for deleted code traces" }, "max_results": { "type": "integer", "minimum": 1, "default": 10 } }, "required": ["query"] } },
            { "name": "resurrect_attempt", "description": "Attempt to reconstruct deleted code from traces", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" }, "query": { "type": "string", "description": "Description of the code to resurrect" } }, "required": ["query"] } },
            { "name": "resurrect_verify", "description": "Verify a resurrection attempt is accurate", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" }, "original_name": { "type": "string", "description": "Original name of the deleted code" }, "reconstructed": { "type": "string", "description": "Reconstructed code to verify" } }, "required": ["original_name", "reconstructed"] } },
            { "name": "resurrect_history", "description": "Get resurrection history for the codebase", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" } } } },
            // -- Invention 14: Code Genetics -----------------------
            { "name": "genetics_dna", "description": "Extract the DNA (core patterns) of a code unit", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" }, "unit_id": { "type": "integer", "description": "Code unit ID" } }, "required": ["unit_id"] } },
            { "name": "genetics_lineage", "description": "Trace the lineage of a code unit through evolution", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" }, "unit_id": { "type": "integer", "description": "Code unit ID" }, "max_depth": { "type": "integer", "minimum": 1, "default": 10 } }, "required": ["unit_id"] } },
            { "name": "genetics_mutations", "description": "Detect mutations (unexpected changes) in code patterns", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" }, "unit_id": { "type": "integer", "description": "Code unit ID" } }, "required": ["unit_id"] } },
            { "name": "genetics_diseases", "description": "Diagnose inherited code diseases (anti-patterns passed through lineage)", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" }, "unit_id": { "type": "integer", "description": "Code unit ID" } }, "required": ["unit_id"] } },
            // -- Invention 15: Code Telepathy ----------------------
            { "name": "telepathy_connect", "description": "Establish telepathic connection between codebases", "inputSchema": { "type": "object", "properties": { "workspace": { "type": "string", "description": "Workspace name or id" }, "source_graph": { "type": "string", "description": "Source graph name" }, "target_graph": { "type": "string", "description": "Target graph name" } }, "required": ["workspace"] } },
            { "name": "telepathy_broadcast", "description": "Broadcast a code insight to connected codebases", "inputSchema": { "type": "object", "properties": { "workspace": { "type": "string", "description": "Workspace name or id" }, "insight": { "type": "string", "description": "The code insight to broadcast" }, "source_graph": { "type": "string", "description": "Source graph name" } }, "required": ["workspace", "insight"] } },
            { "name": "telepathy_listen", "description": "Listen for insights from connected codebases", "inputSchema": { "type": "object", "properties": { "workspace": { "type": "string", "description": "Workspace name or id" }, "target_graph": { "type": "string", "description": "Target graph name to listen from" } }, "required": ["workspace"] } },
            { "name": "telepathy_consensus", "description": "Find consensus patterns across connected codebases", "inputSchema": { "type": "object", "properties": { "workspace": { "type": "string", "description": "Workspace name or id" }, "concept": { "type": "string", "description": "Concept to find consensus on" } }, "required": ["workspace", "concept"] } },
            // -- Invention 16: Code Soul --------------------------
            { "name": "soul_extract", "description": "Extract the soul (essential purpose and values) of code", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" }, "unit_id": { "type": "integer", "description": "Code unit ID" } }, "required": ["unit_id"] } },
            { "name": "soul_compare", "description": "Compare souls across code reincarnations", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" }, "unit_id_a": { "type": "integer", "description": "First code unit ID" }, "unit_id_b": { "type": "integer", "description": "Second code unit ID" } }, "required": ["unit_id_a", "unit_id_b"] } },
            { "name": "soul_preserve", "description": "Preserve a code soul during rewrite", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" }, "unit_id": { "type": "integer", "description": "Code unit ID" }, "new_language": { "type": "string", "description": "Target language for rewrite" } }, "required": ["unit_id"] } },
            { "name": "soul_reincarnate", "description": "Guide a soul to a new code manifestation", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" }, "soul_id": { "type": "string", "description": "Soul identifier" }, "target_context": { "type": "string", "description": "Target context for reincarnation" } }, "required": ["soul_id", "target_context"] } },
            { "name": "soul_karma", "description": "Analyze the karma (positive/negative impact history) of code", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" }, "unit_id": { "type": "integer", "description": "Code unit ID" } }, "required": ["unit_id"] } },
            // -- Invention 17: Code Omniscience --------------------
            { "name": "omniscience_search", "description": "Search across global code knowledge", "inputSchema": { "type": "object", "properties": { "query": { "type": "string", "description": "Search query" }, "languages": { "type": "array", "items": { "type": "string" }, "description": "Filter by languages" }, "max_results": { "type": "integer", "minimum": 1, "default": 10 } }, "required": ["query"] } },
            { "name": "omniscience_best", "description": "Find the best implementation of a concept globally", "inputSchema": { "type": "object", "properties": { "capability": { "type": "string", "description": "Capability to find best implementation for" }, "criteria": { "type": "array", "items": { "type": "string" }, "description": "Evaluation criteria" } }, "required": ["capability"] } },
            { "name": "omniscience_census", "description": "Global code census for a concept", "inputSchema": { "type": "object", "properties": { "concept": { "type": "string", "description": "Concept to census" }, "languages": { "type": "array", "items": { "type": "string" }, "description": "Filter by languages" } }, "required": ["concept"] } },
            { "name": "omniscience_vuln", "description": "Scan for known vulnerability patterns", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" }, "pattern": { "type": "string", "description": "Vulnerability pattern to scan for" }, "cve": { "type": "string", "description": "CVE identifier to check" } } } },
            { "name": "omniscience_trend", "description": "Find emerging or declining code patterns", "inputSchema": { "type": "object", "properties": { "domain": { "type": "string", "description": "Domain to analyze trends in" }, "threshold": { "type": "number", "default": 0.5 } }, "required": ["domain"] } },
            { "name": "omniscience_compare", "description": "Compare your code to global best practices", "inputSchema": { "type": "object", "properties": { "graph": { "type": "string", "description": "Graph name" }, "unit_id": { "type": "integer", "description": "Code unit ID to compare" } }, "required": ["unit_id"] } },
            { "name": "omniscience_api_usage", "description": "Find all usages of an API globally", "inputSchema": { "type": "object", "properties": { "api": { "type": "string", "description": "API name to search for" }, "method": { "type": "string", "description": "Specific method within the API" } }, "required": ["api"] } },
            { "name": "omniscience_solve", "description": "Find code that solves a specific problem", "inputSchema": { "type": "object", "properties": { "problem": { "type": "string", "description": "Problem description to solve" }, "languages": { "type": "array", "items": { "type": "string" }, "description": "Preferred languages" }, "max_results": { "type": "integer", "minimum": 1, "default": 5 } }, "required": ["problem"] } }
        ]);
        if let Value::Array(ref mut arr) = tools_array {
            inject_token_conservation_params(arr);
        }
        JsonRpcResponse::success(
            id,
            json!({
                "tools": tools_array
            }),
        )
    }

    /// Handle "tools/call".
    fn handle_tools_call(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
        // Lazy auto-load: if no graphs are loaded, try the deferred path.
        if self.graphs.is_empty() {
            self.try_lazy_load();
        }

        let requested_tool_name = match params.get("name").and_then(|v| v.as_str()) {
            Some(name) => name.to_string(),
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing 'name' field in tools/call params"),
                );
            }
        };

        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(Value::Object(serde_json::Map::new()));

        let (tool_name, arguments) =
            match normalize_compact_tool_call(&requested_tool_name, arguments) {
                Ok(mapped) => mapped,
                Err(message) => {
                    return JsonRpcResponse::error(id, JsonRpcError::invalid_params(message));
                }
            };

        let result = match tool_name.as_str() {
            "symbol_lookup" => self.tool_symbol_lookup(id.clone(), &arguments),
            "impact_analysis" => self.tool_impact_analysis(id.clone(), &arguments),
            "graph_stats" => self.tool_graph_stats(id.clone(), &arguments),
            "list_units" => self.tool_list_units(id.clone(), &arguments),
            "analysis_log" => return self.tool_analysis_log(id, &arguments),
            "session_start" => return self.tool_session_start(id, &arguments),
            "session_end" => return self.tool_session_end(id, &arguments),
            "codebase_session_resume" => self.tool_codebase_session_resume(id.clone(), &arguments),
            // Grounding tools
            "codebase_ground" => self.tool_codebase_ground(id.clone(), &arguments),
            "codebase_evidence" => self.tool_codebase_evidence(id.clone(), &arguments),
            "codebase_suggest" => self.tool_codebase_suggest(id.clone(), &arguments),
            // Workspace tools
            "workspace_create" => return self.tool_workspace_create(id, &arguments),
            "workspace_add" => return self.tool_workspace_add(id, &arguments),
            "workspace_list" => self.tool_workspace_list(id.clone(), &arguments),
            "workspace_query" => self.tool_workspace_query(id.clone(), &arguments),
            "workspace_compare" => self.tool_workspace_compare(id.clone(), &arguments),
            "workspace_xref" => self.tool_workspace_xref(id.clone(), &arguments),
            // Translation tools
            "translation_record" => return self.tool_translation_record(id, &arguments),
            "translation_progress" => self.tool_translation_progress(id.clone(), &arguments),
            "translation_remaining" => self.tool_translation_remaining(id.clone(), &arguments),
            // ── Invention tools ──────────────────────────────────────
            // 1. Enhanced Impact Analysis
            "impact_analyze" => self.tool_impact_analyze(id.clone(), &arguments),
            "impact_path" => self.tool_impact_path(id.clone(), &arguments),
            // 2. Enhanced Code Prophecy
            "prophecy" => self.tool_prophecy(id.clone(), &arguments),
            "prophecy_if" => self.tool_prophecy_if(id.clone(), &arguments),
            // 3. Regression Oracle
            "regression_predict" => self.tool_regression_predict(id.clone(), &arguments),
            "regression_minimal" => self.tool_regression_minimal(id.clone(), &arguments),
            // 4. Citation Engine
            "codebase_ground_claim" => self.tool_codebase_ground_claim(id.clone(), &arguments),
            "codebase_cite" => self.tool_codebase_cite(id.clone(), &arguments),
            // 5. Hallucination Detector
            "hallucination_check" => self.tool_hallucination_check(id.clone(), &arguments),
            // 6. Truth Maintenance
            "truth_register" => self.tool_truth_register(id.clone(), &arguments),
            "truth_check" => self.tool_truth_check(id.clone(), &arguments),
            // 7. Concept Navigation
            "concept_find" => self.tool_concept_find(id.clone(), &arguments),
            "concept_map" => self.tool_concept_map(id.clone(), &arguments),
            "concept_explain" => self.tool_concept_explain(id.clone(), &arguments),
            // 8. Architecture Inference
            "architecture_infer" => self.tool_architecture_infer(id.clone(), &arguments),
            "architecture_validate" => self.tool_architecture_validate(id.clone(), &arguments),
            // 9. Semantic Search
            "search_semantic" => self.tool_search_semantic(id.clone(), &arguments),
            "search_similar" => self.tool_search_similar(id.clone(), &arguments),
            "search_explain" => self.tool_search_explain(id.clone(), &arguments),
            // 10. Multi-Codebase Compare
            "compare_codebases" => self.tool_compare_codebases(id.clone(), &arguments),
            "compare_concept" => self.tool_compare_concept(id.clone(), &arguments),
            "compare_migrate" => self.tool_compare_migrate(id.clone(), &arguments),
            // 11. Version Archaeology
            "archaeology_node" => self.tool_archaeology_node(id.clone(), &arguments),
            "archaeology_why" => self.tool_archaeology_why(id.clone(), &arguments),
            "archaeology_when" => self.tool_archaeology_when(id.clone(), &arguments),
            // 12. Pattern Extraction
            "pattern_extract" => self.tool_pattern_extract(id.clone(), &arguments),
            "pattern_check" => self.tool_pattern_check(id.clone(), &arguments),
            "pattern_suggest" => self.tool_pattern_suggest(id.clone(), &arguments),
            // 13. Code Resurrection
            "resurrect_search" => self.tool_resurrect_search(id.clone(), &arguments),
            "resurrect_attempt" => self.tool_resurrect_attempt(id.clone(), &arguments),
            "resurrect_verify" => self.tool_resurrect_verify(id.clone(), &arguments),
            "resurrect_history" => self.tool_resurrect_history(id.clone(), &arguments),
            // 14. Code Genetics
            "genetics_dna" => self.tool_genetics_dna(id.clone(), &arguments),
            "genetics_lineage" => self.tool_genetics_lineage(id.clone(), &arguments),
            "genetics_mutations" => self.tool_genetics_mutations(id.clone(), &arguments),
            "genetics_diseases" => self.tool_genetics_diseases(id.clone(), &arguments),
            // 15. Code Telepathy
            "telepathy_connect" => self.tool_telepathy_connect(id.clone(), &arguments),
            "telepathy_broadcast" => self.tool_telepathy_broadcast(id.clone(), &arguments),
            "telepathy_listen" => self.tool_telepathy_listen(id.clone(), &arguments),
            "telepathy_consensus" => self.tool_telepathy_consensus(id.clone(), &arguments),
            // 16. Code Soul
            "soul_extract" => self.tool_soul_extract(id.clone(), &arguments),
            "soul_compare" => self.tool_soul_compare(id.clone(), &arguments),
            "soul_preserve" => self.tool_soul_preserve(id.clone(), &arguments),
            "soul_reincarnate" => self.tool_soul_reincarnate(id.clone(), &arguments),
            "soul_karma" => self.tool_soul_karma(id.clone(), &arguments),
            // 17. Code Omniscience
            "omniscience_search" => self.tool_omniscience_search(id.clone(), &arguments),
            "omniscience_best" => self.tool_omniscience_best(id.clone(), &arguments),
            "omniscience_census" => self.tool_omniscience_census(id.clone(), &arguments),
            "omniscience_vuln" => self.tool_omniscience_vuln(id.clone(), &arguments),
            "omniscience_trend" => self.tool_omniscience_trend(id.clone(), &arguments),
            "omniscience_compare" => self.tool_omniscience_compare(id.clone(), &arguments),
            "omniscience_api_usage" => self.tool_omniscience_api_usage(id.clone(), &arguments),
            "omniscience_solve" => self.tool_omniscience_solve(id.clone(), &arguments),
            _ => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::tool_not_found(format!("Tool not found: {}", tool_name)),
                );
            }
        };

        // Per MCP spec: tool execution errors use isError: true, not JSON-RPC errors.
        // Protocol errors (tool not found, parse error) stay as JSON-RPC errors.
        let result = result.into_tool_error_if_needed();

        // Auto-log the tool call (skip analysis_log to avoid recursion).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let capture_mode = read_env_string_any(&["ACB_AUTO_CAPTURE_MODE", "AUTO_CAPTURE_MODE"])
            .unwrap_or_else(|| "summary".to_string());
        if !capture_mode.eq_ignore_ascii_case("off") {
            let redact =
                read_env_bool_any(&["ACB_AUTO_CAPTURE_REDACT", "AUTO_CAPTURE_REDACT"], true);
            let max_chars = read_env_usize_any(
                &["ACB_AUTO_CAPTURE_MAX_CHARS", "AUTO_CAPTURE_MAX_CHARS"],
                768,
            )
            .max(64);
            let summary = if redact {
                "<redacted>".to_string()
            } else {
                truncate_json_summary(&arguments, max_chars)
            };
            let graph_name = arguments
                .get("graph")
                .and_then(|v| v.as_str())
                .map(String::from);
            self.operation_log.push(OperationRecord {
                tool_name,
                summary,
                timestamp: now,
                graph_name,
            });
        }

        result
    }

    /// Handle "resources/list".
    fn handle_resources_list(&self, id: Value) -> JsonRpcResponse {
        let mut resources = Vec::new();

        for name in self.graphs.keys() {
            resources.push(json!({
                "uri": format!("acb://graphs/{}/stats", name),
                "name": format!("{} statistics", name),
                "description": format!("Statistics for the {} code graph", name),
                "mimeType": "application/json"
            }));
            resources.push(json!({
                "uri": format!("acb://graphs/{}/units", name),
                "name": format!("{} units", name),
                "description": format!("All code units in the {} graph", name),
                "mimeType": "application/json"
            }));
        }

        JsonRpcResponse::success(id, json!({ "resources": resources }))
    }

    /// Handle "resources/read".
    fn handle_resources_read(&self, id: Value, params: &Value) -> JsonRpcResponse {
        let uri = match params.get("uri").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing 'uri' field"),
                );
            }
        };

        // Parse URI: acb://graphs/{name}/stats or acb://graphs/{name}/units
        if let Some(rest) = uri.strip_prefix("acb://graphs/") {
            let parts: Vec<&str> = rest.splitn(2, '/').collect();
            if parts.len() == 2 {
                let graph_name = parts[0];
                let resource = parts[1];

                if let Some(graph) = self.graphs.get(graph_name) {
                    return match resource {
                        "stats" => {
                            let stats = graph.stats();
                            JsonRpcResponse::success(
                                id,
                                json!({
                                    "contents": [{
                                        "uri": uri,
                                        "mimeType": "application/json",
                                        "text": serde_json::to_string_pretty(&json!({
                                            "unit_count": stats.unit_count,
                                            "edge_count": stats.edge_count,
                                            "dimension": stats.dimension,
                                        })).unwrap_or_default()
                                    }]
                                }),
                            )
                        }
                        "units" => {
                            let units: Vec<Value> = graph
                                .units()
                                .iter()
                                .map(|u| {
                                    json!({
                                        "id": u.id,
                                        "name": u.name,
                                        "type": u.unit_type.label(),
                                        "file": u.file_path.display().to_string(),
                                    })
                                })
                                .collect();
                            JsonRpcResponse::success(
                                id,
                                json!({
                                    "contents": [{
                                        "uri": uri,
                                        "mimeType": "application/json",
                                        "text": serde_json::to_string_pretty(&units).unwrap_or_default()
                                    }]
                                }),
                            )
                        }
                        _ => JsonRpcResponse::error(
                            id,
                            JsonRpcError::invalid_params(format!(
                                "Unknown resource type: {}",
                                resource
                            )),
                        ),
                    };
                } else {
                    return JsonRpcResponse::error(
                        id,
                        JsonRpcError::invalid_params(format!("Graph not found: {}", graph_name)),
                    );
                }
            }
        }

        JsonRpcResponse::error(
            id,
            JsonRpcError::invalid_params(format!("Invalid resource URI: {}", uri)),
        )
    }

    /// Handle "prompts/list".
    fn handle_prompts_list(&self, id: Value) -> JsonRpcResponse {
        JsonRpcResponse::success(
            id,
            json!({
                "prompts": [
                    {
                        "name": "analyse_unit",
                        "description": "Analyse a code unit including its dependencies, stability, and test coverage",
                        "arguments": [
                            {
                                "name": "graph",
                                "description": "Graph name",
                                "required": false
                            },
                            {
                                "name": "unit_name",
                                "description": "Name of the code unit to analyse",
                                "required": true
                            }
                        ]
                    },
                    {
                        "name": "explain_coupling",
                        "description": "Explain coupling between two code units",
                        "arguments": [
                            {
                                "name": "graph",
                                "description": "Graph name",
                                "required": false
                            },
                            {
                                "name": "unit_a",
                                "description": "First unit name",
                                "required": true
                            },
                            {
                                "name": "unit_b",
                                "description": "Second unit name",
                                "required": true
                            }
                        ]
                    }
                ]
            }),
        )
    }

    // ========================================================================
    // Tool implementations
    // ========================================================================

    /// Resolve a graph name from arguments, defaulting to the first loaded graph.
    fn resolve_graph<'a>(
        &'a self,
        args: &'a Value,
    ) -> Result<(&'a str, &'a CodeGraph), JsonRpcError> {
        let graph_name = args.get("graph").and_then(|v| v.as_str()).unwrap_or("");

        if graph_name.is_empty() {
            // Use the first graph if available.
            if let Some((name, graph)) = self.graphs.iter().next() {
                return Ok((name.as_str(), graph));
            }
            return Err(JsonRpcError::invalid_params(
                "No graphs loaded. Start the MCP server with --graph <path.acb>, \
                 or set AGENTRA_WORKSPACE_ROOT to a repository for auto-compilation.",
            ));
        }

        self.graphs
            .get(graph_name)
            .map(|g| (graph_name, g))
            .ok_or_else(|| JsonRpcError::invalid_params(format!("Graph not found: {}", graph_name)))
    }

    /// Tool: symbol_lookup.
    fn tool_symbol_lookup(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let name = match args.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing 'name' argument"),
                );
            }
        };

        let mode_raw = args
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("prefix");
        let mode = match mode_raw {
            "exact" => MatchMode::Exact,
            "prefix" => MatchMode::Prefix,
            "contains" => MatchMode::Contains,
            "fuzzy" => MatchMode::Fuzzy,
            _ => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params(format!(
                        "Invalid 'mode': {mode_raw}. Expected one of: exact, prefix, contains, fuzzy"
                    )),
                );
            }
        };

        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

        let params = SymbolLookupParams {
            name,
            mode,
            limit,
            ..SymbolLookupParams::default()
        };

        match self.engine.symbol_lookup(graph, params) {
            Ok(units) => {
                let results: Vec<Value> = units
                    .iter()
                    .map(|u| {
                        json!({
                            "id": u.id,
                            "name": u.name,
                            "qualified_name": u.qualified_name,
                            "type": u.unit_type.label(),
                            "file": u.file_path.display().to_string(),
                            "language": u.language.name(),
                            "complexity": u.complexity,
                        })
                    })
                    .collect();
                JsonRpcResponse::success(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&results).unwrap_or_default()
                        }]
                    }),
                )
            }
            Err(e) => JsonRpcResponse::error(id, JsonRpcError::internal_error(e.to_string())),
        }
    }

    /// Tool: impact_analysis.
    fn tool_impact_analysis(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let unit_id = match args.get("unit_id").and_then(|v| v.as_u64()) {
            Some(uid) => uid,
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing 'unit_id' argument"),
                );
            }
        };

        let max_depth = match args.get("max_depth") {
            None => 3,
            Some(v) => {
                let depth = match v.as_i64() {
                    Some(d) => d,
                    None => {
                        return JsonRpcResponse::error(
                            id,
                            JsonRpcError::invalid_params("'max_depth' must be an integer >= 0"),
                        );
                    }
                };
                if depth < 0 {
                    return JsonRpcResponse::error(
                        id,
                        JsonRpcError::invalid_params("'max_depth' must be >= 0"),
                    );
                }
                depth as u32
            }
        };
        let edge_types = vec![
            EdgeType::Calls,
            EdgeType::Imports,
            EdgeType::Inherits,
            EdgeType::Implements,
            EdgeType::UsesType,
            EdgeType::FfiBinds,
            EdgeType::References,
            EdgeType::Returns,
            EdgeType::ParamType,
            EdgeType::Overrides,
            EdgeType::Contains,
        ];

        let params = ImpactParams {
            unit_id,
            max_depth,
            edge_types,
        };

        match self.engine.impact_analysis(graph, params) {
            Ok(result) => {
                let impacted: Vec<Value> = result
                    .impacted
                    .iter()
                    .map(|i| {
                        json!({
                            "unit_id": i.unit_id,
                            "depth": i.depth,
                            "risk_score": i.risk_score,
                            "has_tests": i.has_tests,
                        })
                    })
                    .collect();
                JsonRpcResponse::success(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&json!({
                                "root_id": result.root_id,
                                "overall_risk": result.overall_risk,
                                "impacted_count": result.impacted.len(),
                                "impacted": impacted,
                                "recommendations": result.recommendations,
                            })).unwrap_or_default()
                        }]
                    }),
                )
            }
            Err(e) => JsonRpcResponse::error(id, JsonRpcError::internal_error(e.to_string())),
        }
    }

    /// Tool: graph_stats.
    fn tool_graph_stats(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (name, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let stats = graph.stats();
        JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&json!({
                        "graph": name,
                        "unit_count": stats.unit_count,
                        "edge_count": stats.edge_count,
                        "dimension": stats.dimension,
                    })).unwrap_or_default()
                }]
            }),
        )
    }

    /// Tool: list_units.
    fn tool_list_units(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
        let unit_type_filter = match args.get("unit_type").and_then(|v| v.as_str()) {
            Some(raw) => match Self::parse_unit_type(raw) {
                Some(parsed) => Some(parsed),
                None => {
                    return JsonRpcResponse::error(
                        id,
                        JsonRpcError::invalid_params(format!(
                            "Unknown unit_type '{}'. Expected one of: module, symbol, type, function, parameter, import, test, doc, config, pattern, trait, impl, macro.",
                            raw
                        )),
                    );
                }
            },
            None => None,
        };

        let units: Vec<Value> = graph
            .units()
            .iter()
            .filter(|u| {
                if let Some(expected) = unit_type_filter {
                    u.unit_type == expected
                } else {
                    true
                }
            })
            .take(limit)
            .map(|u| {
                json!({
                    "id": u.id,
                    "name": u.name,
                    "type": u.unit_type.label(),
                    "file": u.file_path.display().to_string(),
                })
            })
            .collect();

        JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&units).unwrap_or_default()
                }]
            }),
        )
    }

    /// Tool: analysis_log — record the intent/context behind a code analysis.
    fn tool_analysis_log(&mut self, id: Value, args: &Value) -> JsonRpcResponse {
        let intent = match args.get("intent").and_then(|v| v.as_str()) {
            Some(i) if !i.trim().is_empty() => i,
            _ => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("'intent' is required and must not be empty"),
                );
            }
        };

        let finding = args.get("finding").and_then(|v| v.as_str());
        let graph_name = args.get("graph").and_then(|v| v.as_str());
        let topic = args.get("topic").and_then(|v| v.as_str());

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut summary_parts = vec![format!("intent: {intent}")];
        if let Some(f) = finding {
            summary_parts.push(format!("finding: {f}"));
        }
        if let Some(t) = topic {
            summary_parts.push(format!("topic: {t}"));
        }

        let record = OperationRecord {
            tool_name: "analysis_log".to_string(),
            summary: summary_parts.join(" | "),
            timestamp: now,
            graph_name: graph_name.map(String::from),
        };

        let index = self.operation_log.len();
        self.operation_log.push(record);

        JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&json!({
                        "log_index": index,
                        "message": "Analysis context logged"
                    })).unwrap_or_default()
                }]
            }),
        )
    }

    /// Tool: session_start — start or reset the current interaction session.
    fn tool_session_start(&mut self, id: Value, args: &Value) -> JsonRpcResponse {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let session_id = args
            .get("session_id")
            .and_then(|v| v.as_u64())
            .unwrap_or(now);
        let metadata = args.get("metadata").cloned().unwrap_or_else(|| json!({}));

        self.session_start_time = Some(now);
        self.operation_log.clear();

        JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&json!({
                        "session_id": session_id,
                        "started_at": now,
                        "metadata": metadata,
                    })).unwrap_or_default()
                }]
            }),
        )
    }

    /// Tool: session_end — end the current interaction session.
    fn tool_session_end(&mut self, id: Value, args: &Value) -> JsonRpcResponse {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let session_id = args
            .get("session_id")
            .and_then(|v| v.as_u64())
            .unwrap_or(now);
        let summary = args.get("summary").and_then(|v| v.as_str());
        let started_at = self.session_start_time.take();
        let duration_seconds = started_at.map(|start| now.saturating_sub(start));

        JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&json!({
                        "session_id": session_id,
                        "ended_at": now,
                        "started_at": started_at,
                        "duration_seconds": duration_seconds,
                        "operation_count": self.operation_log.len(),
                        "summary": summary,
                    })).unwrap_or_default()
                }]
            }),
        )
    }

    /// Tool: codebase_session_resume — load recent session context.
    fn tool_codebase_session_resume(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
        let recent: Vec<Value> = self
            .operation_log
            .iter()
            .rev()
            .take(limit.max(1))
            .map(|record| {
                json!({
                    "tool_name": record.tool_name,
                    "summary": record.summary,
                    "timestamp": record.timestamp,
                    "graph_name": record.graph_name
                })
            })
            .collect();

        JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&json!({
                        "session_start_time": self.session_start_time,
                        "operation_count": self.operation_log.len(),
                        "recent_tool_calls": recent
                    })).unwrap_or_default()
                }]
            }),
        )
    }

    /// Access the operation log.
    pub fn operation_log(&self) -> &[OperationRecord] {
        &self.operation_log
    }

    /// Access the workspace manager.
    pub fn workspace_manager(&self) -> &WorkspaceManager {
        &self.workspace_manager
    }

    /// Access the workspace manager mutably.
    pub fn workspace_manager_mut(&mut self) -> &mut WorkspaceManager {
        &mut self.workspace_manager
    }

    // ========================================================================
    // Grounding tool implementations
    // ========================================================================

    /// Tool: codebase_ground — verify a claim about code has graph evidence.
    fn tool_codebase_ground(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let claim = match args.get("claim").and_then(|v| v.as_str()) {
            Some(c) if !c.trim().is_empty() => c,
            _ => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing or empty 'claim' argument"),
                );
            }
        };

        let strict = args
            .get("strict")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let engine = GroundingEngine::new(graph);
        let result = engine.ground_claim(claim);

        // In strict mode, Partial is treated as Ungrounded.
        let result = if strict {
            match result {
                GroundingResult::Partial {
                    unsupported,
                    suggestions,
                    ..
                } => GroundingResult::Ungrounded {
                    claim: claim.to_string(),
                    suggestions: {
                        let mut s = unsupported;
                        s.extend(suggestions);
                        s
                    },
                },
                other => other,
            }
        } else {
            result
        };

        let output = match &result {
            GroundingResult::Verified {
                evidence,
                confidence,
            } => json!({
                "status": "verified",
                "confidence": confidence,
                "evidence": evidence.iter().map(|e| json!({
                    "node_id": e.node_id,
                    "node_type": e.node_type,
                    "name": e.name,
                    "file_path": e.file_path,
                    "line_number": e.line_number,
                    "snippet": e.snippet,
                })).collect::<Vec<_>>(),
            }),
            GroundingResult::Partial {
                supported,
                unsupported,
                suggestions,
            } => json!({
                "status": "partial",
                "supported": supported,
                "unsupported": unsupported,
                "suggestions": suggestions,
            }),
            GroundingResult::Ungrounded {
                claim, suggestions, ..
            } => json!({
                "status": "ungrounded",
                "claim": claim,
                "suggestions": suggestions,
            }),
        };

        JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&output).unwrap_or_default()
                }]
            }),
        )
    }

    /// Tool: codebase_evidence — get graph evidence for a symbol name.
    fn tool_codebase_evidence(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let name = match args.get("name").and_then(|v| v.as_str()) {
            Some(n) if !n.trim().is_empty() => n,
            _ => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing or empty 'name' argument"),
                );
            }
        };

        let type_filters: Vec<String> = args
            .get("types")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                    .collect()
            })
            .unwrap_or_default();

        let engine = GroundingEngine::new(graph);
        let mut evidence = engine.find_evidence(name);

        // Apply type filters if provided.
        if !type_filters.is_empty() {
            evidence.retain(|e| type_filters.contains(&e.node_type.to_lowercase()));
        }

        let output: Vec<Value> = evidence
            .iter()
            .map(|e| {
                json!({
                    "node_id": e.node_id,
                    "node_type": e.node_type,
                    "name": e.name,
                    "file_path": e.file_path,
                    "line_number": e.line_number,
                    "snippet": e.snippet,
                })
            })
            .collect();

        JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&output).unwrap_or_default()
                }]
            }),
        )
    }

    /// Tool: codebase_suggest — find symbols similar to a name.
    fn tool_codebase_suggest(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let name = match args.get("name").and_then(|v| v.as_str()) {
            Some(n) if !n.trim().is_empty() => n,
            _ => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing or empty 'name' argument"),
                );
            }
        };

        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

        let engine = GroundingEngine::new(graph);
        let suggestions = engine.suggest_similar(name, limit);

        JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&json!({
                        "query": name,
                        "suggestions": suggestions,
                    })).unwrap_or_default()
                }]
            }),
        )
    }

    // ========================================================================
    // Workspace tool implementations
    // ========================================================================

    /// Resolve a workspace ID from arguments. Accepts workspace ID directly
    /// or tries to match by name.
    fn resolve_workspace_id(&self, args: &Value) -> Result<String, JsonRpcError> {
        let raw = args.get("workspace").and_then(|v| v.as_str()).unwrap_or("");
        if raw.is_empty() {
            // Try active workspace.
            return self
                .workspace_manager
                .get_active()
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    JsonRpcError::invalid_params("No workspace specified and none active")
                });
        }

        // If it looks like a workspace ID (starts with "ws-"), use directly.
        if raw.starts_with("ws-") {
            // Validate it exists.
            self.workspace_manager
                .list(raw)
                .map(|_| raw.to_string())
                .map_err(JsonRpcError::invalid_params)
        } else {
            // Try to find by name — iterate all workspaces. We need to expose
            // this through the manager. For now, just treat it as an ID.
            self.workspace_manager
                .list(raw)
                .map(|_| raw.to_string())
                .map_err(JsonRpcError::invalid_params)
        }
    }

    /// Tool: workspace_create.
    fn tool_workspace_create(&mut self, id: Value, args: &Value) -> JsonRpcResponse {
        let name = match args.get("name").and_then(|v| v.as_str()) {
            Some(n) if !n.trim().is_empty() => n,
            _ => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing or empty 'name' argument"),
                );
            }
        };

        let ws_id = self.workspace_manager.create(name);

        JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&json!({
                        "workspace_id": ws_id,
                        "name": name,
                        "message": "Workspace created"
                    })).unwrap_or_default()
                }]
            }),
        )
    }

    /// Tool: workspace_add — add a loaded graph as a context.
    fn tool_workspace_add(&mut self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let graph_name = match args.get("graph").and_then(|v| v.as_str()) {
            Some(n) if !n.trim().is_empty() => n.to_string(),
            _ => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing or empty 'graph' argument"),
                );
            }
        };

        // Clone the graph to add to workspace (graphs remain available in the server).
        let graph = match self.graphs.get(&graph_name) {
            Some(g) => g.clone(),
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params(format!("Graph not found: {}", graph_name)),
                );
            }
        };

        let role_str = args
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("source");
        let role = match ContextRole::parse_str(role_str) {
            Some(r) => r,
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params(format!(
                        "Invalid role '{}'. Expected: source, target, reference, comparison",
                        role_str
                    )),
                );
            }
        };

        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(&graph_name)
            .to_string();
        let language = args
            .get("language")
            .and_then(|v| v.as_str())
            .map(String::from);

        match self
            .workspace_manager
            .add_context(&ws_id, &path, role, language, graph)
        {
            Ok(ctx_id) => JsonRpcResponse::success(
                id,
                json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&json!({
                            "context_id": ctx_id,
                            "workspace_id": ws_id,
                            "graph": graph_name,
                            "message": "Context added to workspace"
                        })).unwrap_or_default()
                    }]
                }),
            ),
            Err(e) => JsonRpcResponse::error(id, JsonRpcError::invalid_params(e)),
        }
    }

    /// Tool: workspace_list.
    fn tool_workspace_list(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        match self.workspace_manager.list(&ws_id) {
            Ok(workspace) => {
                let contexts: Vec<Value> = workspace
                    .contexts
                    .iter()
                    .map(|c| {
                        json!({
                            "id": c.id,
                            "role": c.role.label(),
                            "path": c.path,
                            "language": c.language,
                            "unit_count": c.graph.units().len(),
                        })
                    })
                    .collect();

                JsonRpcResponse::success(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&json!({
                                "workspace_id": ws_id,
                                "name": workspace.name,
                                "context_count": workspace.contexts.len(),
                                "contexts": contexts,
                            })).unwrap_or_default()
                        }]
                    }),
                )
            }
            Err(e) => JsonRpcResponse::error(id, JsonRpcError::invalid_params(e)),
        }
    }

    /// Tool: workspace_query.
    fn tool_workspace_query(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) if !q.trim().is_empty() => q,
            _ => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing or empty 'query' argument"),
                );
            }
        };

        let role_filters: Vec<String> = args
            .get("roles")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                    .collect()
            })
            .unwrap_or_default();

        match self.workspace_manager.query_all(&ws_id, query) {
            Ok(results) => {
                let mut filtered = results;
                if !role_filters.is_empty() {
                    filtered.retain(|r| role_filters.contains(&r.context_role.label().to_string()));
                }

                let output: Vec<Value> = filtered
                    .iter()
                    .map(|r| {
                        json!({
                            "context_id": r.context_id,
                            "role": r.context_role.label(),
                            "matches": r.matches.iter().map(|m| json!({
                                "unit_id": m.unit_id,
                                "name": m.name,
                                "qualified_name": m.qualified_name,
                                "type": m.unit_type,
                                "file": m.file_path,
                            })).collect::<Vec<_>>(),
                        })
                    })
                    .collect();

                JsonRpcResponse::success(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&output).unwrap_or_default()
                        }]
                    }),
                )
            }
            Err(e) => JsonRpcResponse::error(id, JsonRpcError::invalid_params(e)),
        }
    }

    /// Tool: workspace_compare.
    fn tool_workspace_compare(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let symbol = match args.get("symbol").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() => s,
            _ => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing or empty 'symbol' argument"),
                );
            }
        };

        match self.workspace_manager.compare(&ws_id, symbol) {
            Ok(cmp) => {
                let contexts: Vec<Value> = cmp
                    .contexts
                    .iter()
                    .map(|c| {
                        json!({
                            "context_id": c.context_id,
                            "role": c.role.label(),
                            "found": c.found,
                            "unit_type": c.unit_type,
                            "signature": c.signature,
                            "file_path": c.file_path,
                        })
                    })
                    .collect();

                JsonRpcResponse::success(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&json!({
                                "symbol": cmp.symbol,
                                "semantic_match": cmp.semantic_match,
                                "structural_diff": cmp.structural_diff,
                                "contexts": contexts,
                            })).unwrap_or_default()
                        }]
                    }),
                )
            }
            Err(e) => JsonRpcResponse::error(id, JsonRpcError::invalid_params(e)),
        }
    }

    /// Tool: workspace_xref.
    fn tool_workspace_xref(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let symbol = match args.get("symbol").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() => s,
            _ => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing or empty 'symbol' argument"),
                );
            }
        };

        match self.workspace_manager.cross_reference(&ws_id, symbol) {
            Ok(xref) => {
                let found: Vec<Value> = xref
                    .found_in
                    .iter()
                    .map(|(ctx_id, role)| json!({"context_id": ctx_id, "role": role.label()}))
                    .collect();
                let missing: Vec<Value> = xref
                    .missing_from
                    .iter()
                    .map(|(ctx_id, role)| json!({"context_id": ctx_id, "role": role.label()}))
                    .collect();

                JsonRpcResponse::success(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&json!({
                                "symbol": xref.symbol,
                                "found_in": found,
                                "missing_from": missing,
                            })).unwrap_or_default()
                        }]
                    }),
                )
            }
            Err(e) => JsonRpcResponse::error(id, JsonRpcError::invalid_params(e)),
        }
    }

    // ========================================================================
    // Translation tool implementations
    // ========================================================================

    /// Tool: translation_record.
    fn tool_translation_record(&mut self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let source_symbol = match args.get("source_symbol").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() => s,
            _ => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing or empty 'source_symbol' argument"),
                );
            }
        };

        let target_symbol = args.get("target_symbol").and_then(|v| v.as_str());

        let status_str = args
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("not_started");
        let status = match TranslationStatus::parse_str(status_str) {
            Some(s) => s,
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params(format!(
                        "Invalid status '{}'. Expected: not_started, in_progress, ported, verified, skipped",
                        status_str
                    )),
                );
            }
        };

        let notes = args.get("notes").and_then(|v| v.as_str()).map(String::from);

        // Get or create translation map for this workspace.
        // Use workspace's first source and first target context IDs.
        let tmap = self
            .translation_maps
            .entry(ws_id.clone())
            .or_insert_with(|| {
                // Find source and target context IDs from the workspace.
                let (src, tgt) = if let Ok(ws) = self.workspace_manager.list(&ws_id) {
                    let src = ws
                        .contexts
                        .iter()
                        .find(|c| c.role == ContextRole::Source)
                        .map(|c| c.id.clone())
                        .unwrap_or_default();
                    let tgt = ws
                        .contexts
                        .iter()
                        .find(|c| c.role == ContextRole::Target)
                        .map(|c| c.id.clone())
                        .unwrap_or_default();
                    (src, tgt)
                } else {
                    (String::new(), String::new())
                };
                TranslationMap::new(src, tgt)
            });

        tmap.record(source_symbol, target_symbol, status, notes);

        JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&json!({
                        "source_symbol": source_symbol,
                        "target_symbol": target_symbol,
                        "status": status_str,
                        "message": "Translation mapping recorded"
                    })).unwrap_or_default()
                }]
            }),
        )
    }

    /// Tool: translation_progress.
    fn tool_translation_progress(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let progress = match self.translation_maps.get(&ws_id) {
            Some(tmap) => tmap.progress(),
            None => {
                // No translation map yet — return zeros.
                crate::workspace::TranslationProgress {
                    total: 0,
                    not_started: 0,
                    in_progress: 0,
                    ported: 0,
                    verified: 0,
                    skipped: 0,
                    percent_complete: 0.0,
                }
            }
        };

        JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&json!({
                        "workspace": ws_id,
                        "total": progress.total,
                        "not_started": progress.not_started,
                        "in_progress": progress.in_progress,
                        "ported": progress.ported,
                        "verified": progress.verified,
                        "skipped": progress.skipped,
                        "percent_complete": progress.percent_complete,
                    })).unwrap_or_default()
                }]
            }),
        )
    }

    /// Tool: translation_remaining.
    fn tool_translation_remaining(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let module_filter = args
            .get("module")
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase());

        let remaining = match self.translation_maps.get(&ws_id) {
            Some(tmap) => {
                let mut items = tmap.remaining();
                if let Some(ref module) = module_filter {
                    items.retain(|m| m.source_symbol.to_lowercase().contains(module.as_str()));
                }
                items
                    .iter()
                    .map(|m| {
                        json!({
                            "source_symbol": m.source_symbol,
                            "status": m.status.label(),
                            "notes": m.notes,
                        })
                    })
                    .collect::<Vec<_>>()
            }
            None => Vec::new(),
        };

        JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&json!({
                        "workspace": ws_id,
                        "remaining_count": remaining.len(),
                        "remaining": remaining,
                    })).unwrap_or_default()
                }]
            }),
        )
    }

    // ========================================================================
    // Invention tool handlers
    // ========================================================================

    // ── 1. Enhanced Impact Analysis ─────────────────────────────────────

    fn tool_impact_analyze(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(5) as u32;
        let change_type_str = args
            .get("change_type")
            .and_then(|v| v.as_str())
            .unwrap_or("behavior");
        let change_type = match change_type_str {
            "signature" => crate::engine::impact::ChangeType::Signature,
            "deletion" => crate::engine::impact::ChangeType::Deletion,
            "rename" => crate::engine::impact::ChangeType::Rename,
            "move" => crate::engine::impact::ChangeType::Move,
            _ => crate::engine::impact::ChangeType::Behavior,
        };

        let analyzer = crate::engine::impact::ImpactAnalyzer::new(graph);
        let change = crate::engine::impact::ProposedChange {
            target: unit_id,
            change_type,
            description: format!("Proposed {} change to unit {}", change_type_str, unit_id),
        };
        let result = analyzer.analyze(change, max_depth);
        let viz = analyzer.visualize(&result);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&viz).unwrap_or_default() }] }),
        )
    }

    fn tool_impact_path(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let from = args.get("from").and_then(|v| v.as_u64()).unwrap_or(0);
        let to = args.get("to").and_then(|v| v.as_u64()).unwrap_or(0);

        let analyzer = crate::engine::impact::ImpactAnalyzer::new(graph);
        let path = analyzer.impact_path(from, to);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "from": from, "to": to, "path": path })).unwrap_or_default() }] }),
        )
    }

    // ── 2. Enhanced Code Prophecy ───────────────────────────────────────

    fn tool_prophecy(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);

        let engine = crate::temporal::prophecy_v2::EnhancedProphecyEngine::new(graph);
        let subject = crate::temporal::prophecy_v2::ProphecySubject::Node(unit_id);
        let horizon = crate::temporal::prophecy_v2::ProphecyHorizon::MediumTerm;
        let result = engine.prophecy(subject, horizon);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }] }),
        )
    }

    fn tool_prophecy_if(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let change_type_str = args
            .get("change_type")
            .and_then(|v| v.as_str())
            .unwrap_or("behavior");

        let engine = crate::temporal::prophecy_v2::EnhancedProphecyEngine::new(graph);
        let subject = crate::temporal::prophecy_v2::ProphecySubject::Node(unit_id);
        let horizon = crate::temporal::prophecy_v2::ProphecyHorizon::MediumTerm;
        let scenario = format!("{} change to unit {}", change_type_str, unit_id);
        let result = engine.prophecy_if(subject, &scenario, horizon);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }] }),
        )
    }

    // ── 3. Regression Oracle ────────────────────────────────────────────

    fn tool_regression_predict(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(5) as u32;

        let predictor = crate::engine::regression::RegressionPredictor::new(graph);
        let oracle = predictor.predict(unit_id, max_depth);

        let results: Vec<Value> = oracle
            .likely_failures
            .iter()
            .map(|p| {
                json!({
                    "test_id": p.test.unit_id,
                    "test_function": p.test.function,
                    "test_file": p.test.file,
                    "failure_probability": p.failure_probability,
                    "reason": p.reason,
                })
            })
            .collect();

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "changed_unit": unit_id, "likely_failures": results, "safe_to_skip": oracle.safe_to_skip.len() })).unwrap_or_default() }] }),
        )
    }

    fn tool_regression_minimal(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let _threshold = args
            .get("threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5);

        let predictor = crate::engine::regression::RegressionPredictor::new(graph);
        let minimal = predictor.minimal_test_set(unit_id);

        let results: Vec<Value> = minimal
            .iter()
            .map(|t| {
                json!({
                    "test_id": t.unit_id,
                    "test_function": t.function,
                    "test_file": t.file,
                })
            })
            .collect();

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "changed_unit": unit_id, "minimal_tests": results })).unwrap_or_default() }] }),
        )
    }

    // ── 4. Citation Engine ──────────────────────────────────────────────

    fn tool_codebase_ground_claim(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let claim = args.get("claim").and_then(|v| v.as_str()).unwrap_or("");

        let engine = crate::grounding::citation::CitationEngine::new(graph);
        let result = engine.ground_claim(claim);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }] }),
        )
    }

    fn tool_codebase_cite(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);

        let engine = crate::grounding::citation::CitationEngine::new(graph);
        let citation = engine.cite_node(unit_id);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "unit_id": unit_id, "citation": citation })).unwrap_or_default() }] }),
        )
    }

    // ── 5. Hallucination Detector ───────────────────────────────────────

    fn tool_hallucination_check(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let output = args.get("output").and_then(|v| v.as_str()).unwrap_or("");

        let detector = crate::grounding::hallucination::HallucinationDetector::new(graph);
        let result = detector.check_output(output);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }] }),
        )
    }

    // ── 6. Truth Maintenance ────────────────────────────────────────────

    fn tool_truth_register(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let claim = args.get("claim").and_then(|v| v.as_str()).unwrap_or("");

        let mut maintainer = crate::grounding::truth::TruthMaintainer::new(graph);
        let truth = maintainer.register_truth(claim);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&truth).unwrap_or_default() }] }),
        )
    }

    fn tool_truth_check(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let claim = args.get("claim").and_then(|v| v.as_str()).unwrap_or("");

        let maintainer = crate::grounding::truth::TruthMaintainer::new(graph);
        let result = maintainer.check_truth(claim);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "claim": claim, "status": format!("{:?}", result) })).unwrap_or_default() }] }),
        )
    }

    // ── 7. Concept Navigation ───────────────────────────────────────────

    fn tool_concept_find(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let concept = args.get("concept").and_then(|v| v.as_str()).unwrap_or("");

        let navigator = crate::semantic::concept_nav::ConceptNavigator::new(graph);
        let query = crate::semantic::concept_nav::ConceptQuery {
            description: concept.to_string(),
            constraints: Vec::new(),
        };
        let result = navigator.find_concept(query);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }] }),
        )
    }

    fn tool_concept_map(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let navigator = crate::semantic::concept_nav::ConceptNavigator::new(graph);
        let concepts = navigator.map_all_concepts();

        let results: Vec<Value> = concepts
            .iter()
            .map(|c| {
                json!({
                    "name": c.name,
                    "description": c.description,
                    "implementation_count": c.implementations.len(),
                })
            })
            .collect();

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "concepts": results })).unwrap_or_default() }] }),
        )
    }

    fn tool_concept_explain(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let concept = args.get("concept").and_then(|v| v.as_str()).unwrap_or("");

        let navigator = crate::semantic::concept_nav::ConceptNavigator::new(graph);
        let result = navigator.explain_concept(concept);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }] }),
        )
    }

    // ── 8. Architecture Inference ───────────────────────────────────────

    fn tool_architecture_infer(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let inferrer = crate::semantic::architecture::ArchitectureInferrer::new(graph);
        let architecture = inferrer.infer();
        let diagram = inferrer.diagram(&architecture);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "architecture": architecture, "diagram": diagram })).unwrap_or_default() }] }),
        )
    }

    fn tool_architecture_validate(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let inferrer = crate::semantic::architecture::ArchitectureInferrer::new(graph);
        let architecture = inferrer.infer();
        let anomalies = inferrer.validate(architecture.pattern);

        let results: Vec<Value> = anomalies
            .iter()
            .map(|a| {
                json!({
                    "description": a.description,
                    "severity": format!("{:?}", a.severity),
                    "expected": a.expected,
                    "actual": a.actual,
                })
            })
            .collect();

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "anomalies": results, "count": results.len() })).unwrap_or_default() }] }),
        )
    }

    // ── 9. Semantic Search ──────────────────────────────────────────────

    fn tool_search_semantic(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

        let engine = crate::index::semantic_search::SemanticSearchEngine::new(graph);
        let result = engine.search(query, top_k);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }] }),
        )
    }

    fn tool_search_similar(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

        let engine = crate::index::semantic_search::SemanticSearchEngine::new(graph);
        let results = engine.find_similar(unit_id, top_k);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "unit_id": unit_id, "similar": results })).unwrap_or_default() }] }),
        )
    }

    fn tool_search_explain(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");

        let engine = crate::index::semantic_search::SemanticSearchEngine::new(graph);
        let explanation = engine.explain_match(unit_id, query);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "unit_id": unit_id, "query": query, "explanation": explanation })).unwrap_or_default() }] }),
        )
    }

    // ── 10. Multi-Codebase Compare ──────────────────────────────────────

    fn tool_compare_codebases(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(id) => id,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let workspace = match self.workspace_manager.list(&ws_id) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, JsonRpcError::invalid_params(e)),
        };
        if workspace.contexts.len() < 2 {
            return JsonRpcResponse::error(
                id,
                JsonRpcError::invalid_params("Need at least 2 contexts in workspace to compare"),
            );
        }

        let comparer = crate::workspace::compare::CodebaseComparer::new(
            &workspace.contexts[0].graph,
            &workspace.contexts[0].id,
            &workspace.contexts[1].graph,
            &workspace.contexts[1].id,
        );
        let result = comparer.compare();

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }] }),
        )
    }

    fn tool_compare_concept(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(id) => id,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let concept = args.get("concept").and_then(|v| v.as_str()).unwrap_or("");
        let workspace = match self.workspace_manager.list(&ws_id) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, JsonRpcError::invalid_params(e)),
        };
        if workspace.contexts.len() < 2 {
            return JsonRpcResponse::error(
                id,
                JsonRpcError::invalid_params("Need at least 2 contexts"),
            );
        }

        let comparer = crate::workspace::compare::CodebaseComparer::new(
            &workspace.contexts[0].graph,
            &workspace.contexts[0].id,
            &workspace.contexts[1].graph,
            &workspace.contexts[1].id,
        );
        let result = comparer.compare_concept(concept);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }] }),
        )
    }

    fn tool_compare_migrate(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(id) => id,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let workspace = match self.workspace_manager.list(&ws_id) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, JsonRpcError::invalid_params(e)),
        };
        if workspace.contexts.len() < 2 {
            return JsonRpcResponse::error(
                id,
                JsonRpcError::invalid_params("Need at least 2 contexts"),
            );
        }

        let comparer = crate::workspace::compare::CodebaseComparer::new(
            &workspace.contexts[0].graph,
            &workspace.contexts[0].id,
            &workspace.contexts[1].graph,
            &workspace.contexts[1].id,
        );
        let plan = comparer.migration_plan();

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&plan).unwrap_or_default() }] }),
        )
    }

    // ── 11. Version Archaeology ─────────────────────────────────────────

    fn tool_archaeology_node(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);

        let history = crate::temporal::history::ChangeHistory::new();
        let archaeologist = crate::temporal::archaeology::CodeArchaeologist::new(graph, history);
        let result = archaeologist.investigate(unit_id);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "unit_id": unit_id, "result": result })).unwrap_or_default() }] }),
        )
    }

    fn tool_archaeology_why(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);

        let history = crate::temporal::history::ChangeHistory::new();
        let archaeologist = crate::temporal::archaeology::CodeArchaeologist::new(graph, history);
        let result = archaeologist.investigate(unit_id);
        let explanation = result
            .map(|r| r.why_explanation)
            .unwrap_or_else(|| "Unit not found".to_string());

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "unit_id": unit_id, "explanation": explanation })).unwrap_or_default() }] }),
        )
    }

    fn tool_archaeology_when(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);

        let history = crate::temporal::history::ChangeHistory::new();
        let archaeologist = crate::temporal::archaeology::CodeArchaeologist::new(graph, history);
        let timeline = archaeologist.when_changed(unit_id);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "unit_id": unit_id, "timeline": timeline })).unwrap_or_default() }] }),
        )
    }

    // ── 12. Pattern Extraction ──────────────────────────────────────────

    fn tool_pattern_extract(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };

        let extractor = crate::semantic::pattern_extract::PatternExtractor::new(graph);
        let patterns = extractor.extract_patterns();

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&patterns).unwrap_or_default() }] }),
        )
    }

    fn tool_pattern_check(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);

        let extractor = crate::semantic::pattern_extract::PatternExtractor::new(graph);
        let violations = extractor.check_patterns(unit_id);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "unit_id": unit_id, "violations": violations })).unwrap_or_default() }] }),
        )
    }

    fn tool_pattern_suggest(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let file_path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");

        let extractor = crate::semantic::pattern_extract::PatternExtractor::new(graph);
        let suggestions = extractor.suggest_patterns(file_path);

        JsonRpcResponse::success(
            id,
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({ "file_path": file_path, "suggestions": suggestions })).unwrap_or_default() }] }),
        )
    }

    // -- 13. Code Resurrection --------------------------------------------------

    fn tool_resurrect_search(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;
        let query_lower = query.to_lowercase();
        let mut traces: Vec<Value> = Vec::new();
        for unit in graph.units() {
            let name_lower = unit.name.to_lowercase();
            let doc_lower = unit.doc_summary.as_deref().unwrap_or("").to_lowercase();
            if name_lower.contains(&query_lower) || doc_lower.contains(&query_lower) {
                let is_deprecated = doc_lower.contains("deprecated")
                    || doc_lower.contains("removed")
                    || name_lower.contains("deprecated")
                    || name_lower.starts_with("old_");
                traces.push(json!({"unit_id": unit.id, "name": unit.name, "type": unit.unit_type.label(), "file": unit.file_path.display().to_string(), "is_deprecated": is_deprecated, "doc": unit.doc_summary, "trace_type": if is_deprecated { "deprecated" } else { "reference" }}));
                if traces.len() >= max_results {
                    break;
                }
            }
        }
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"query": query, "traces_found": traces.len(), "traces": traces})).unwrap_or_default()}]}),
        )
    }

    fn tool_resurrect_attempt(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let query_lower = query.to_lowercase();
        let mut evidence: Vec<Value> = Vec::new();
        for unit in graph.units() {
            let name_lower = unit.name.to_lowercase();
            let doc_lower = unit.doc_summary.as_deref().unwrap_or("").to_lowercase();
            let sig_lower = unit.signature.as_deref().unwrap_or("").to_lowercase();
            if (unit.unit_type == CodeUnitType::Test
                && (name_lower.contains(&query_lower) || doc_lower.contains(&query_lower)))
                || (unit.unit_type == CodeUnitType::Doc && doc_lower.contains(&query_lower))
                || sig_lower.contains(&query_lower)
            {
                evidence.push(json!({"source": unit.unit_type.label(), "unit_id": unit.id, "name": unit.name, "signature": unit.signature, "doc": unit.doc_summary, "file": unit.file_path.display().to_string()}));
            }
        }
        let status = if evidence.is_empty() {
            "insufficient_evidence"
        } else {
            "partial_reconstruction"
        };
        let confidence = if evidence.len() > 5 {
            "high"
        } else if evidence.len() > 2 {
            "medium"
        } else {
            "low"
        };
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"query": query, "status": status, "confidence": confidence, "evidence_count": evidence.len(), "evidence": evidence})).unwrap_or_default()}]}),
        )
    }

    fn tool_resurrect_verify(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let original_name = args
            .get("original_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let reconstructed = args
            .get("reconstructed")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let name_lower = original_name.to_lowercase();
        let mut refs = 0u64;
        let mut has_tests = false;
        for unit in graph.units() {
            if unit.name.to_lowercase().contains(&name_lower) {
                refs += 1;
                if unit.unit_type == CodeUnitType::Test {
                    has_tests = true;
                }
            }
        }
        let status = if refs > 0 {
            "plausible"
        } else {
            "unverifiable"
        };
        let confidence = if has_tests && refs > 2 {
            "high"
        } else if refs > 0 {
            "medium"
        } else {
            "low"
        };
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"original_name": original_name, "reconstructed_length": reconstructed.len(), "references": refs, "has_tests": has_tests, "status": status, "confidence": confidence})).unwrap_or_default()}]}),
        )
    }

    fn tool_resurrect_history(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let mut versions: Vec<Value> = Vec::new();
        for edge in graph.edges() {
            if edge.edge_type == EdgeType::VersionOf {
                let src = graph
                    .get_unit(edge.source_id)
                    .map(|u| u.name.as_str())
                    .unwrap_or("?");
                let tgt = graph
                    .get_unit(edge.target_id)
                    .map(|u| u.name.as_str())
                    .unwrap_or("?");
                versions.push(json!({"newer_id": edge.source_id, "newer": src, "older_id": edge.target_id, "older": tgt}));
            }
        }
        let mut deprecated: Vec<Value> = Vec::new();
        for unit in graph.units() {
            let doc = unit.doc_summary.as_deref().unwrap_or("").to_lowercase();
            if doc.contains("deprecated") || unit.name.to_lowercase().contains("deprecated") {
                deprecated.push(
                    json!({"unit_id": unit.id, "name": unit.name, "type": unit.unit_type.label()}),
                );
            }
        }
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"versions": versions, "deprecated": deprecated})).unwrap_or_default()}]}),
        )
    }

    // -- 14. Code Genetics ------------------------------------------------------

    fn tool_genetics_dna(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let unit = match graph.get_unit(unit_id) {
            Some(u) => u,
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params(format!("Unit {} not found", unit_id)),
                )
            }
        };
        let outgoing = graph.edges_from(unit_id);
        let incoming = graph.edges_to(unit_id);
        let mut out_types: Vec<&str> = outgoing.iter().map(|e| e.edge_type.label()).collect();
        out_types.sort();
        out_types.dedup();
        let mut in_types: Vec<&str> = incoming.iter().map(|e| e.edge_type.label()).collect();
        in_types.sort();
        in_types.dedup();
        let naming = if unit.name.contains('_') {
            "snake_case"
        } else if unit
            .name
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
        {
            "PascalCase"
        } else {
            "camelCase"
        };
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"unit_id": unit_id, "name": unit.name, "type": unit.unit_type.label(), "naming": naming, "complexity": unit.complexity, "is_async": unit.is_async, "visibility": format!("{:?}", unit.visibility), "out_edge_types": out_types, "in_edge_types": in_types, "out_count": outgoing.len(), "in_count": incoming.len(), "has_tests": incoming.iter().any(|e| e.edge_type == EdgeType::Tests), "has_docs": incoming.iter().any(|e| e.edge_type == EdgeType::Documents), "stability": unit.stability_score, "signature": unit.signature})).unwrap_or_default()}]}),
        )
    }

    fn tool_genetics_lineage(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
        let mut lineage: Vec<Value> = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut frontier = vec![(unit_id, 0usize)];
        while let Some((current, depth)) = frontier.pop() {
            if depth > max_depth || !visited.insert(current) {
                continue;
            }
            if let Some(unit) = graph.get_unit(current) {
                lineage.push(json!({"unit_id": current, "name": unit.name, "type": unit.unit_type.label(), "depth": depth, "file": unit.file_path.display().to_string()}));
                for edge in graph.edges_to(current) {
                    if matches!(
                        edge.edge_type,
                        EdgeType::Contains
                            | EdgeType::Inherits
                            | EdgeType::VersionOf
                            | EdgeType::Implements
                    ) {
                        frontier.push((edge.source_id, depth + 1));
                    }
                }
            }
        }
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"unit_id": unit_id, "lineage_depth": lineage.len(), "lineage": lineage})).unwrap_or_default()}]}),
        )
    }

    fn tool_genetics_mutations(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let unit = match graph.get_unit(unit_id) {
            Some(u) => u,
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params(format!("Unit {} not found", unit_id)),
                )
            }
        };
        let mut mutations: Vec<Value> = Vec::new();
        if unit.complexity > 20 {
            mutations.push(json!({"type": "complexity_mutation", "description": format!("Complexity {} is unusually high", unit.complexity), "severity": "medium"}));
        }
        if unit.stability_score < 0.3 && unit.change_count > 5 {
            mutations.push(json!({"type": "stability_mutation", "description": format!("Low stability ({:.2}) with {} changes", unit.stability_score, unit.change_count), "severity": "high"}));
        }
        let breaks = graph
            .edges_from(unit_id)
            .into_iter()
            .filter(|e| e.edge_type == EdgeType::BreaksWith)
            .count();
        if breaks > 0 {
            mutations.push(json!({"type": "breaking_mutation", "description": format!("{} breaking relationships", breaks), "severity": "high"}));
        }
        // Check naming mutation vs siblings
        let parents: Vec<_> = graph
            .edges_to(unit_id)
            .into_iter()
            .filter(|e| e.edge_type == EdgeType::Contains)
            .collect();
        for pe in &parents {
            let sibs: Vec<_> = graph
                .edges_from(pe.source_id)
                .into_iter()
                .filter(|e| e.edge_type == EdgeType::Contains && e.target_id != unit_id)
                .collect();
            let unit_snake = unit.name.contains('_');
            for se in &sibs {
                if let Some(sib) = graph.get_unit(se.target_id) {
                    if sib.unit_type == unit.unit_type && sib.name.contains('_') != unit_snake {
                        mutations.push(json!({"type": "naming_mutation", "description": format!("'{}' differs from sibling '{}'", unit.name, sib.name), "severity": "low"}));
                        break;
                    }
                }
            }
        }
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"unit_id": unit_id, "name": unit.name, "mutations": mutations})).unwrap_or_default()}]}),
        )
    }

    fn tool_genetics_diseases(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let unit = match graph.get_unit(unit_id) {
            Some(u) => u,
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params(format!("Unit {} not found", unit_id)),
                )
            }
        };
        let mut diseases: Vec<Value> = Vec::new();
        let out = graph.edges_from(unit_id);
        let inc = graph.edges_to(unit_id);
        if out.len() > 20 {
            diseases.push(json!({"disease": "god_object", "severity": "high", "detail": format!("{} outgoing edges", out.len())}));
        }
        let targets: std::collections::HashSet<u64> = out.iter().map(|e| e.target_id).collect();
        let sources: std::collections::HashSet<u64> = inc.iter().map(|e| e.source_id).collect();
        let circular: Vec<u64> = targets.intersection(&sources).copied().collect();
        if !circular.is_empty() {
            diseases.push(
                json!({"disease": "circular_dependency", "severity": "high", "with": circular}),
            );
        }
        let calls_out = out
            .iter()
            .filter(|e| e.edge_type == EdgeType::Calls)
            .count();
        let calls_in = inc
            .iter()
            .filter(|e| e.edge_type == EdgeType::Calls)
            .count();
        if calls_out > 10 && calls_out > calls_in * 3 {
            diseases.push(json!({"disease": "feature_envy", "severity": "medium", "calls_out": calls_out, "calls_in": calls_in}));
        }
        if !inc.iter().any(|e| e.edge_type == EdgeType::Tests)
            && unit.unit_type == CodeUnitType::Function
        {
            diseases.push(json!({"disease": "untested", "severity": "medium"}));
        }
        let non_contains = inc
            .iter()
            .filter(|e| e.edge_type != EdgeType::Contains)
            .count();
        if non_contains == 0 && !inc.is_empty() {
            diseases.push(json!({"disease": "orphan_code", "severity": "medium"}));
        }
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"unit_id": unit_id, "name": unit.name, "diseases": diseases})).unwrap_or_default()}]}),
        )
    }

    // -- 15. Code Telepathy -----------------------------------------------------

    fn tool_telepathy_connect(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let workspace = match self.workspace_manager.list(&ws_id) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, JsonRpcError::invalid_params(e)),
        };
        let connections: Vec<Value> = workspace.contexts.iter().map(|c| json!({"context_id": c.id, "role": c.role.label(), "path": c.path, "units": c.graph.units().len()})).collect();
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"workspace": ws_id, "status": "connected", "connections": connections})).unwrap_or_default()}]}),
        )
    }

    fn tool_telepathy_broadcast(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let insight = args.get("insight").and_then(|v| v.as_str()).unwrap_or("");
        let source_graph = args
            .get("source_graph")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let results = match self.workspace_manager.query_all(&ws_id, insight) {
            Ok(r) => r,
            Err(e) => return JsonRpcResponse::error(id, JsonRpcError::invalid_params(e)),
        };
        let receivers: Vec<Value> = results.iter().map(|r| json!({"context_id": r.context_id, "role": r.context_role.label(), "matches": r.matches.len()})).collect();
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"workspace": ws_id, "insight": insight, "source": source_graph, "receivers": receivers})).unwrap_or_default()}]}),
        )
    }

    fn tool_telepathy_listen(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let workspace = match self.workspace_manager.list(&ws_id) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, JsonRpcError::invalid_params(e)),
        };
        let mut insights: Vec<Value> = Vec::new();
        for ctx in &workspace.contexts {
            let high_cx: Vec<&str> = ctx
                .graph
                .units()
                .iter()
                .filter(|u| u.complexity > 15)
                .take(5)
                .map(|u| u.name.as_str())
                .collect();
            if !high_cx.is_empty() {
                insights.push(json!({"context": ctx.id, "role": ctx.role.label(), "type": "high_complexity", "units": high_cx}));
            }
            let tests = ctx
                .graph
                .units()
                .iter()
                .filter(|u| u.unit_type == CodeUnitType::Test)
                .count();
            let funcs = ctx
                .graph
                .units()
                .iter()
                .filter(|u| u.unit_type == CodeUnitType::Function)
                .count();
            if funcs > 0 {
                insights.push(json!({"context": ctx.id, "role": ctx.role.label(), "type": "test_ratio", "tests": tests, "functions": funcs, "ratio": tests as f64 / funcs as f64}));
            }
        }
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"workspace": ws_id, "insights": insights})).unwrap_or_default()}]}),
        )
    }

    fn tool_telepathy_consensus(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let ws_id = match self.resolve_workspace_id(args) {
            Ok(ws) => ws,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let concept = args.get("concept").and_then(|v| v.as_str()).unwrap_or("");
        let results = match self.workspace_manager.query_all(&ws_id, concept) {
            Ok(r) => r,
            Err(e) => return JsonRpcResponse::error(id, JsonRpcError::invalid_params(e)),
        };
        let total = results.len();
        let with_matches = results.iter().filter(|r| !r.matches.is_empty()).count();
        let level = if with_matches == total && total > 0 {
            "universal"
        } else if with_matches as f64 / total.max(1) as f64 > 0.5 {
            "majority"
        } else if with_matches > 0 {
            "minority"
        } else {
            "none"
        };
        let details: Vec<Value> = results.iter().map(|r| json!({"context_id": r.context_id, "role": r.context_role.label(), "has_concept": !r.matches.is_empty(), "count": r.matches.len()})).collect();
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"concept": concept, "consensus": level, "total": total, "with_concept": with_matches, "details": details})).unwrap_or_default()}]}),
        )
    }

    // -- 16. Code Soul ----------------------------------------------------------

    fn tool_soul_extract(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let unit = match graph.get_unit(unit_id) {
            Some(u) => u,
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params(format!("Unit {} not found", unit_id)),
                )
            }
        };
        let out = graph.edges_from(unit_id);
        let inc = graph.edges_to(unit_id);
        let purpose = format!(
            "{} {} that {}",
            format!("{:?}", unit.visibility).to_lowercase(),
            unit.unit_type.label(),
            if out.iter().any(|e| e.edge_type == EdgeType::Calls) {
                "orchestrates calls"
            } else if unit.unit_type == CodeUnitType::Test {
                "verifies behavior"
            } else if unit.unit_type == CodeUnitType::Doc {
                "documents knowledge"
            } else {
                "provides functionality"
            }
        );
        let mut values: Vec<&str> = Vec::new();
        if inc.iter().any(|e| e.edge_type == EdgeType::Tests) {
            values.push("correctness");
        }
        if inc.iter().any(|e| e.edge_type == EdgeType::Documents) {
            values.push("documentation");
        }
        if unit.stability_score > 0.8 {
            values.push("stability");
        }
        if unit.complexity < 5 {
            values.push("simplicity");
        }
        if unit.is_async {
            values.push("concurrency");
        }
        let deps: Vec<String> = out
            .iter()
            .filter_map(|e| {
                graph
                    .get_unit(e.target_id)
                    .map(|u| format!("{}:{}", e.edge_type.label(), u.name.clone()))
            })
            .take(10)
            .collect();
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"unit_id": unit_id, "name": unit.name, "soul_id": format!("soul-{}-{}", unit_id, unit.name), "purpose": purpose, "values": values, "dependencies": deps, "signature": unit.signature, "complexity": unit.complexity, "stability": unit.stability_score, "language": unit.language.name()})).unwrap_or_default()}]}),
        )
    }

    fn tool_soul_compare(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let id_a = args.get("unit_id_a").and_then(|v| v.as_u64()).unwrap_or(0);
        let id_b = args.get("unit_id_b").and_then(|v| v.as_u64()).unwrap_or(0);
        let ua = match graph.get_unit(id_a) {
            Some(u) => u,
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params(format!("Unit {} not found", id_a)),
                )
            }
        };
        let ub = match graph.get_unit(id_b) {
            Some(u) => u,
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params(format!("Unit {} not found", id_b)),
                )
            }
        };
        let ea: std::collections::HashSet<String> = graph
            .edges_from(id_a)
            .iter()
            .map(|e| e.edge_type.label().to_string())
            .collect();
        let eb: std::collections::HashSet<String> = graph
            .edges_from(id_b)
            .iter()
            .map(|e| e.edge_type.label().to_string())
            .collect();
        let shared: Vec<&String> = ea.intersection(&eb).collect();
        let type_match = ua.unit_type == ub.unit_type;
        let cdiff = (ua.complexity as i64 - ub.complexity as i64).unsigned_abs();
        let mut sim = 0.0f64;
        if type_match {
            sim += 0.3;
        }
        if ua.is_async == ub.is_async {
            sim += 0.1;
        }
        if cdiff < 5 {
            sim += 0.2;
        }
        sim += 0.4 * (shared.len() as f64 / ea.len().max(eb.len()).max(1) as f64);
        let verdict = if sim > 0.8 {
            "same_soul"
        } else if sim > 0.5 {
            "related_souls"
        } else {
            "different_souls"
        };
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"unit_a": {"id": id_a, "name": ua.name}, "unit_b": {"id": id_b, "name": ub.name}, "similarity": sim, "type_match": type_match, "complexity_diff": cdiff, "shared_edges": shared, "verdict": verdict})).unwrap_or_default()}]}),
        )
    }

    fn tool_soul_preserve(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let new_lang = args
            .get("new_language")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let unit = match graph.get_unit(unit_id) {
            Some(u) => u,
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params(format!("Unit {} not found", unit_id)),
                )
            }
        };
        let out = graph.edges_from(unit_id);
        let inc = graph.edges_to(unit_id);
        let risk = if out.len() > 10 {
            "high"
        } else if out.len() > 5 {
            "medium"
        } else {
            "low"
        };
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"unit_id": unit_id, "name": unit.name, "original_language": unit.language.name(), "target_language": new_lang, "soul_id": format!("soul-{}-{}", unit_id, unit.name), "purpose": unit.doc_summary, "signature": unit.signature, "deps": out.len(), "dependents": inc.len(), "is_async": unit.is_async, "tests": inc.iter().filter(|e| e.edge_type == EdgeType::Tests).count(), "risk": risk})).unwrap_or_default()}]}),
        )
    }

    fn tool_soul_reincarnate(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_gn, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let soul_id = args.get("soul_id").and_then(|v| v.as_str()).unwrap_or("");
        let target_ctx = args
            .get("target_context")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let uid: u64 = soul_id
            .strip_prefix("soul-")
            .and_then(|s| s.split('-').next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let guide = if let Some(unit) = graph.get_unit(uid) {
            let deps: Vec<String> = graph
                .edges_from(uid)
                .iter()
                .filter_map(|e| graph.get_unit(e.target_id).map(|u| u.name.clone()))
                .take(10)
                .collect();
            json!({"soul_id": soul_id, "target": target_ctx, "status": "guidance_ready", "name": unit.name, "type": unit.unit_type.label(), "signature": unit.signature, "purpose": unit.doc_summary, "deps": deps})
        } else {
            json!({"soul_id": soul_id, "target": target_ctx, "status": "soul_not_found"})
        };
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&guide).unwrap_or_default()}]}),
        )
    }

    fn tool_soul_karma(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let unit = match graph.get_unit(unit_id) {
            Some(u) => u,
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params(format!("Unit {} not found", unit_id)),
                )
            }
        };
        let inc = graph.edges_to(unit_id);
        let out = graph.edges_from(unit_id);
        let mut pos = 0i64;
        let mut neg = 0i64;
        let mut details: Vec<Value> = Vec::new();
        let tc = inc
            .iter()
            .filter(|e| e.edge_type == EdgeType::Tests)
            .count();
        if tc > 0 {
            pos += tc as i64 * 10;
            details
                .push(json!({"k": "positive", "r": format!("{} tests", tc), "p": tc as i64 * 10}));
        }
        let dc = inc
            .iter()
            .filter(|e| e.edge_type == EdgeType::Documents)
            .count();
        if dc > 0 {
            pos += dc as i64 * 5;
            details.push(json!({"k": "positive", "r": format!("{} docs", dc), "p": dc as i64 * 5}));
        }
        if unit.stability_score > 0.8 {
            pos += 15;
            details.push(json!({"k": "positive", "r": "high stability", "p": 15}));
        }
        if unit.complexity < 10 {
            pos += 10;
            details.push(json!({"k": "positive", "r": "low complexity", "p": 10}));
        }
        let br = out
            .iter()
            .filter(|e| e.edge_type == EdgeType::BreaksWith)
            .count();
        if br > 0 {
            neg += br as i64 * 20;
            details.push(
                json!({"k": "negative", "r": format!("{} breaks", br), "p": -(br as i64 * 20)}),
            );
        }
        if unit.complexity > 20 {
            neg += 15;
            details.push(json!({"k": "negative", "r": "very high complexity", "p": -15}));
        }
        if tc == 0 && unit.unit_type == CodeUnitType::Function {
            neg += 10;
            details.push(json!({"k": "negative", "r": "no tests", "p": -10}));
        }
        if unit.stability_score < 0.3 {
            neg += 10;
            details.push(json!({"k": "negative", "r": "low stability", "p": -10}));
        }
        let total = pos - neg;
        let level = if total > 30 {
            "enlightened"
        } else if total > 10 {
            "good"
        } else if total > -10 {
            "neutral"
        } else {
            "troubled"
        };
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"unit_id": unit_id, "name": unit.name, "karma": total, "positive": pos, "negative": neg, "level": level, "details": details})).unwrap_or_default()}]}),
        )
    }

    // -- 17. Code Omniscience ---------------------------------------------------

    fn tool_omniscience_search(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let languages: Vec<String> = args
            .get("languages")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;
        let ql = query.to_lowercase();
        let mut results: Vec<Value> = Vec::new();
        for (gn, graph) in &self.graphs {
            for unit in graph.units() {
                if results.len() >= max_results {
                    break;
                }
                if !languages.is_empty()
                    && !languages
                        .iter()
                        .any(|l| l.eq_ignore_ascii_case(unit.language.name()))
                {
                    continue;
                }
                if unit.name.to_lowercase().contains(&ql)
                    || unit.qualified_name.to_lowercase().contains(&ql)
                {
                    results.push(json!({"graph": gn, "unit_id": unit.id, "name": unit.name, "qualified_name": unit.qualified_name, "type": unit.unit_type.label(), "language": unit.language.name(), "file": unit.file_path.display().to_string()}));
                }
            }
        }
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"query": query, "count": results.len(), "results": results})).unwrap_or_default()}]}),
        )
    }

    fn tool_omniscience_best(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let cap = args
            .get("capability")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let criteria: Vec<String> = args
            .get("criteria")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let cl = cap.to_lowercase();
        let mut cands: Vec<Value> = Vec::new();
        for (gn, graph) in &self.graphs {
            for unit in graph.units() {
                if unit.name.to_lowercase().contains(&cl)
                    || unit.qualified_name.to_lowercase().contains(&cl)
                {
                    let inc = graph.edges_to(unit.id);
                    let has_t = inc.iter().any(|e| e.edge_type == EdgeType::Tests);
                    let has_d = inc.iter().any(|e| e.edge_type == EdgeType::Documents);
                    let mut s = 0.15f64;
                    if has_t {
                        s += 0.3;
                    }
                    if has_d {
                        s += 0.2;
                    }
                    if unit.stability_score > 0.7 {
                        s += 0.2;
                    }
                    if unit.complexity < 15 {
                        s += 0.15;
                    }
                    cands.push(json!({"graph": gn, "unit_id": unit.id, "name": unit.name, "score": s, "has_tests": has_t, "has_docs": has_d, "stability": unit.stability_score, "complexity": unit.complexity}));
                }
            }
        }
        cands.sort_by(|a, b| {
            b["score"]
                .as_f64()
                .unwrap_or(0.0)
                .partial_cmp(&a["score"].as_f64().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        cands.truncate(5);
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"capability": cap, "criteria": criteria, "best": cands})).unwrap_or_default()}]}),
        )
    }

    fn tool_omniscience_census(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let concept = args.get("concept").and_then(|v| v.as_str()).unwrap_or("");
        let languages: Vec<String> = args
            .get("languages")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let cl = concept.to_lowercase();
        let mut by_lang: HashMap<String, usize> = HashMap::new();
        let mut by_type: HashMap<String, usize> = HashMap::new();
        let mut total = 0usize;
        for graph in self.graphs.values() {
            for unit in graph.units() {
                if !languages.is_empty()
                    && !languages
                        .iter()
                        .any(|l| l.eq_ignore_ascii_case(unit.language.name()))
                {
                    continue;
                }
                if unit.name.to_lowercase().contains(&cl)
                    || unit.qualified_name.to_lowercase().contains(&cl)
                {
                    total += 1;
                    *by_lang.entry(unit.language.name().to_string()).or_insert(0) += 1;
                    *by_type
                        .entry(unit.unit_type.label().to_string())
                        .or_insert(0) += 1;
                }
            }
        }
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"concept": concept, "total": total, "by_language": by_lang, "by_type": by_type, "graphs": self.graphs.len()})).unwrap_or_default()}]}),
        )
    }

    fn tool_omniscience_vuln(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let cve = args.get("cve").and_then(|v| v.as_str()).unwrap_or("");
        let kws: Vec<&str> = if pattern.is_empty() {
            vec![
                "unsafe",
                "eval",
                "exec",
                "sql",
                "inject",
                "deserialize",
                "shell",
            ]
        } else {
            vec![pattern]
        };
        let mut findings: Vec<Value> = Vec::new();
        for unit in graph.units() {
            let nl = unit.name.to_lowercase();
            let sl = unit.signature.as_deref().unwrap_or("").to_lowercase();
            for &kw in &kws {
                if nl.contains(kw) || sl.contains(kw) {
                    let sev = if kw == "unsafe" || kw == "eval" || kw == "exec" {
                        "high"
                    } else {
                        "medium"
                    };
                    findings.push(json!({"unit_id": unit.id, "name": unit.name, "type": unit.unit_type.label(), "file": unit.file_path.display().to_string(), "pattern": kw, "severity": sev}));
                    break;
                }
            }
        }
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"pattern": pattern, "cve": cve, "count": findings.len(), "findings": findings})).unwrap_or_default()}]}),
        )
    }

    fn tool_omniscience_trend(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let domain = args.get("domain").and_then(|v| v.as_str()).unwrap_or("");
        let dl = domain.to_lowercase();
        let mut trends: Vec<Value> = Vec::new();
        for (gn, graph) in &self.graphs {
            let m: Vec<_> = graph
                .units()
                .iter()
                .filter(|u| {
                    u.name.to_lowercase().contains(&dl)
                        || u.qualified_name.to_lowercase().contains(&dl)
                })
                .collect();
            if !m.is_empty() {
                let avg_s: f64 =
                    m.iter().map(|u| u.stability_score as f64).sum::<f64>() / m.len() as f64;
                let avg_c: f64 =
                    m.iter().map(|u| u.change_count as f64).sum::<f64>() / m.len() as f64;
                let avg_x: f64 =
                    m.iter().map(|u| u.complexity as f64).sum::<f64>() / m.len() as f64;
                let dir = if avg_c > 5.0 && avg_s < 0.5 {
                    "declining"
                } else if avg_s > 0.7 && avg_x < 15.0 {
                    "stable"
                } else {
                    "emerging"
                };
                trends.push(json!({"graph": gn, "count": m.len(), "avg_stability": avg_s, "avg_changes": avg_c, "avg_complexity": avg_x, "trend": dir}));
            }
        }
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"domain": domain, "graphs": self.graphs.len(), "trends": trends})).unwrap_or_default()}]}),
        )
    }

    fn tool_omniscience_compare(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let (_, graph) = match self.resolve_graph(args) {
            Ok(g) => g,
            Err(e) => return JsonRpcResponse::error(id, e),
        };
        let unit_id = args.get("unit_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let unit = match graph.get_unit(unit_id) {
            Some(u) => u,
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params(format!("Unit {} not found", unit_id)),
                )
            }
        };
        let inc = graph.edges_to(unit_id);
        let out = graph.edges_from(unit_id);
        let has_t = inc.iter().any(|e| e.edge_type == EdgeType::Tests);
        let has_d = inc.iter().any(|e| e.edge_type == EdgeType::Documents);
        let mut practices: Vec<Value> = Vec::new();
        let mut score = 0u32;
        if has_t {
            score += 20;
            practices.push(json!({"p": "tests", "s": "pass", "pts": 20}));
        } else {
            practices.push(json!({"p": "tests", "s": "fail", "rec": "Add tests"}));
        }
        if has_d {
            score += 15;
            practices.push(json!({"p": "docs", "s": "pass", "pts": 15}));
        } else {
            practices.push(json!({"p": "docs", "s": "fail", "rec": "Add docs"}));
        }
        if unit.complexity < 10 {
            score += 20;
            practices.push(json!({"p": "complexity", "s": "pass", "pts": 20}));
        } else if unit.complexity < 20 {
            score += 10;
            practices.push(json!({"p": "complexity", "s": "warn", "pts": 10}));
        } else {
            practices.push(json!({"p": "complexity", "s": "fail", "rec": "Refactor"}));
        }
        if unit.stability_score > 0.7 {
            score += 15;
            practices.push(json!({"p": "stability", "s": "pass", "pts": 15}));
        } else {
            practices.push(json!({"p": "stability", "s": "fail", "rec": "Stabilize"}));
        }
        if out.len() < 10 {
            score += 15;
            practices.push(json!({"p": "coupling", "s": "pass", "pts": 15}));
        } else {
            practices.push(json!({"p": "coupling", "s": "fail", "rec": "Reduce deps"}));
        }
        if unit.doc_summary.is_some() {
            score += 15;
            practices.push(json!({"p": "doc_summary", "s": "pass", "pts": 15}));
        } else {
            practices.push(json!({"p": "doc_summary", "s": "fail", "rec": "Add summary"}));
        }
        let grade = if score >= 80 {
            "A"
        } else if score >= 60 {
            "B"
        } else if score >= 40 {
            "C"
        } else {
            "D"
        };
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"unit_id": unit_id, "name": unit.name, "score": score, "max": 100, "grade": grade, "practices": practices})).unwrap_or_default()}]}),
        )
    }

    fn tool_omniscience_api_usage(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let api = args.get("api").and_then(|v| v.as_str()).unwrap_or("");
        let method = args.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let al = api.to_lowercase();
        let ml = method.to_lowercase();
        let mut usages: Vec<Value> = Vec::new();
        for (gn, graph) in &self.graphs {
            for unit in graph.units() {
                let nl = unit.name.to_lowercase();
                let ql = unit.qualified_name.to_lowercase();
                let sl = unit.signature.as_deref().unwrap_or("").to_lowercase();
                let ma = nl.contains(&al) || ql.contains(&al) || sl.contains(&al);
                let mm = method.is_empty() || nl.contains(&ml) || sl.contains(&ml);
                if ma && mm {
                    usages.push(json!({"graph": gn, "unit_id": unit.id, "name": unit.name, "qualified_name": unit.qualified_name, "type": unit.unit_type.label(), "file": unit.file_path.display().to_string(), "signature": unit.signature}));
                }
            }
        }
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"api": api, "method": method, "count": usages.len(), "usages": usages})).unwrap_or_default()}]}),
        )
    }

    fn tool_omniscience_solve(&self, id: Value, args: &Value) -> JsonRpcResponse {
        let problem = args.get("problem").and_then(|v| v.as_str()).unwrap_or("");
        let languages: Vec<String> = args
            .get("languages")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let max_r = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;
        let kws: Vec<String> = problem
            .to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .map(String::from)
            .collect();
        let mut sols: Vec<Value> = Vec::new();
        for (gn, graph) in &self.graphs {
            for unit in graph.units() {
                if sols.len() >= max_r {
                    break;
                }
                if !languages.is_empty()
                    && !languages
                        .iter()
                        .any(|l| l.eq_ignore_ascii_case(unit.language.name()))
                {
                    continue;
                }
                let nl = unit.name.to_lowercase();
                let dl = unit.doc_summary.as_deref().unwrap_or("").to_lowercase();
                let mc = kws
                    .iter()
                    .filter(|kw| nl.contains(kw.as_str()) || dl.contains(kw.as_str()))
                    .count();
                if mc > 0 {
                    let rel = mc as f64 / kws.len().max(1) as f64;
                    sols.push(json!({"graph": gn, "unit_id": unit.id, "name": unit.name, "type": unit.unit_type.label(), "language": unit.language.name(), "file": unit.file_path.display().to_string(), "relevance": rel, "doc": unit.doc_summary, "signature": unit.signature}));
                }
            }
        }
        sols.sort_by(|a, b| {
            b["relevance"]
                .as_f64()
                .unwrap_or(0.0)
                .partial_cmp(&a["relevance"].as_f64().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sols.truncate(max_r);
        JsonRpcResponse::success(
            id,
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&json!({"problem": problem, "count": sols.len(), "solutions": sols})).unwrap_or_default()}]}),
        )
    }
}

fn mcp_tool_surface_is_compact() -> bool {
    read_env_string_any(&["ACB_MCP_TOOL_SURFACE", "MCP_TOOL_SURFACE"])
        .map(|value| value.trim().eq_ignore_ascii_case("compact"))
        .unwrap_or(false)
}

fn compact_op_schema(ops: &[&str], description: &str) -> Value {
    json!({
        "type": "object",
        "required": ["operation"],
        "properties": {
            "operation": {
                "type": "string",
                "enum": ops,
                "description": description
            },
            "params": {
                "type": "object",
                "description": "Arguments for the selected operation"
            }
        }
    })
}

fn compact_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "codebase_core",
            "description": "Compact core facade for analysis and impact operations",
            "inputSchema": compact_op_schema(&[
                "analysis_log",
                "symbol_lookup",
                "impact_analysis",
                "impact_analyze",
                "impact_path",
                "graph_stats",
                "list_units"
            ], "Core codebase operation")
        }),
        json!({
            "name": "codebase_grounding",
            "description": "Compact grounding facade",
            "inputSchema": compact_op_schema(&[
                "codebase_ground",
                "codebase_evidence",
                "codebase_suggest",
                "codebase_ground_claim",
                "codebase_cite",
                "hallucination_check",
                "truth_register",
                "truth_check"
            ], "Grounding operation")
        }),
        json!({
            "name": "codebase_workspace",
            "description": "Compact workspace facade",
            "inputSchema": compact_op_schema(&[
                "workspace_create",
                "workspace_add",
                "workspace_list",
                "workspace_query",
                "workspace_compare",
                "workspace_xref",
                "compare_codebases",
                "compare_concept",
                "compare_migrate"
            ], "Workspace operation")
        }),
        json!({
            "name": "codebase_session",
            "description": "Compact session facade",
            "inputSchema": compact_op_schema(&[
                "session_start",
                "session_end",
                "codebase_session_resume"
            ], "Session operation")
        }),
        json!({
            "name": "codebase_conceptual",
            "description": "Compact concept/architecture/search facade",
            "inputSchema": compact_op_schema(&[
                "concept_find",
                "concept_map",
                "concept_explain",
                "architecture_infer",
                "architecture_validate",
                "search_semantic",
                "search_similar",
                "search_explain"
            ], "Conceptual operation")
        }),
        json!({
            "name": "codebase_translation",
            "description": "Compact translation facade",
            "inputSchema": compact_op_schema(&[
                "translation_record",
                "translation_progress",
                "translation_remaining"
            ], "Translation operation")
        }),
        json!({
            "name": "codebase_archaeology",
            "description": "Compact archaeology and resurrection facade",
            "inputSchema": compact_op_schema(&[
                "archaeology_node",
                "archaeology_when",
                "archaeology_why",
                "resurrect_search",
                "resurrect_attempt",
                "resurrect_verify",
                "resurrect_history"
            ], "Archaeology operation")
        }),
        json!({
            "name": "codebase_patterns",
            "description": "Compact pattern and genetics facade",
            "inputSchema": compact_op_schema(&[
                "pattern_extract",
                "pattern_check",
                "pattern_suggest",
                "genetics_dna",
                "genetics_lineage",
                "genetics_mutations",
                "genetics_diseases"
            ], "Pattern operation")
        }),
        json!({
            "name": "codebase_collective",
            "description": "Compact telepathy and soul facade",
            "inputSchema": compact_op_schema(&[
                "telepathy_connect",
                "telepathy_broadcast",
                "telepathy_listen",
                "telepathy_consensus",
                "soul_extract",
                "soul_compare",
                "soul_preserve",
                "soul_reincarnate",
                "soul_karma"
            ], "Collective operation")
        }),
        json!({
            "name": "codebase_intelligence",
            "description": "Compact prophecy, regression, and omniscience facade",
            "inputSchema": compact_op_schema(&[
                "prophecy",
                "prophecy_if",
                "regression_predict",
                "regression_minimal",
                "omniscience_search",
                "omniscience_best",
                "omniscience_census",
                "omniscience_vuln",
                "omniscience_trend",
                "omniscience_compare",
                "omniscience_api_usage",
                "omniscience_solve"
            ], "Intelligence operation")
        }),
    ]
}

fn decode_compact_operation(args: Value) -> Result<(String, Value), String> {
    let obj = args
        .as_object()
        .ok_or_else(|| "arguments must be an object".to_string())?;

    let operation = obj
        .get("operation")
        .and_then(Value::as_str)
        .ok_or_else(|| "'operation' is required".to_string())?
        .to_string();

    if let Some(params) = obj.get("params") {
        return Ok((operation, params.clone()));
    }

    let mut passthrough = obj.clone();
    passthrough.remove("operation");
    Ok((operation, Value::Object(passthrough)))
}

fn resolve_compact_tool(group: &str, operation: &str) -> Option<String> {
    let allowed = match group {
        "codebase_core" => matches!(
            operation,
            "analysis_log"
                | "symbol_lookup"
                | "impact_analysis"
                | "impact_analyze"
                | "impact_path"
                | "graph_stats"
                | "list_units"
        ),
        "codebase_grounding" => matches!(
            operation,
            "codebase_ground"
                | "codebase_evidence"
                | "codebase_suggest"
                | "codebase_ground_claim"
                | "codebase_cite"
                | "hallucination_check"
                | "truth_register"
                | "truth_check"
        ),
        "codebase_workspace" => matches!(
            operation,
            "workspace_create"
                | "workspace_add"
                | "workspace_list"
                | "workspace_query"
                | "workspace_compare"
                | "workspace_xref"
                | "compare_codebases"
                | "compare_concept"
                | "compare_migrate"
        ),
        "codebase_session" => {
            matches!(
                operation,
                "session_start" | "session_end" | "codebase_session_resume"
            )
        }
        "codebase_conceptual" => matches!(
            operation,
            "concept_find"
                | "concept_map"
                | "concept_explain"
                | "architecture_infer"
                | "architecture_validate"
                | "search_semantic"
                | "search_similar"
                | "search_explain"
        ),
        "codebase_translation" => matches!(
            operation,
            "translation_record" | "translation_progress" | "translation_remaining"
        ),
        "codebase_archaeology" => matches!(
            operation,
            "archaeology_node"
                | "archaeology_when"
                | "archaeology_why"
                | "resurrect_search"
                | "resurrect_attempt"
                | "resurrect_verify"
                | "resurrect_history"
        ),
        "codebase_patterns" => matches!(
            operation,
            "pattern_extract"
                | "pattern_check"
                | "pattern_suggest"
                | "genetics_dna"
                | "genetics_lineage"
                | "genetics_mutations"
                | "genetics_diseases"
        ),
        "codebase_collective" => matches!(
            operation,
            "telepathy_connect"
                | "telepathy_broadcast"
                | "telepathy_listen"
                | "telepathy_consensus"
                | "soul_extract"
                | "soul_compare"
                | "soul_preserve"
                | "soul_reincarnate"
                | "soul_karma"
        ),
        "codebase_intelligence" => matches!(
            operation,
            "prophecy"
                | "prophecy_if"
                | "regression_predict"
                | "regression_minimal"
                | "omniscience_search"
                | "omniscience_best"
                | "omniscience_census"
                | "omniscience_vuln"
                | "omniscience_trend"
                | "omniscience_compare"
                | "omniscience_api_usage"
                | "omniscience_solve"
        ),
        _ => return None,
    };

    if allowed {
        Some(operation.to_string())
    } else {
        None
    }
}

fn normalize_compact_tool_call(
    requested_tool_name: &str,
    arguments: Value,
) -> Result<(String, Value), String> {
    if !matches!(
        requested_tool_name,
        "codebase_core"
            | "codebase_grounding"
            | "codebase_workspace"
            | "codebase_session"
            | "codebase_conceptual"
            | "codebase_translation"
            | "codebase_archaeology"
            | "codebase_patterns"
            | "codebase_collective"
            | "codebase_intelligence"
    ) {
        return Ok((requested_tool_name.to_string(), arguments));
    }

    let (operation, params) = decode_compact_operation(arguments)?;
    let resolved = resolve_compact_tool(requested_tool_name, &operation)
        .ok_or_else(|| format!("Unknown {requested_tool_name} operation: {operation}"))?;

    Ok((resolved, params))
}

/// Truncate a JSON value to a short summary string.
fn truncate_json_summary(value: &Value, max_len: usize) -> String {
    let s = value.to_string();
    if s.len() <= max_len {
        s
    } else {
        format!("{}...", &s[..max_len])
    }
}

fn read_env_string_any(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| std::env::var(name).ok())
        .map(|value| value.trim().to_string())
}

fn read_env_bool_any(names: &[&str], default: bool) -> bool {
    read_env_string_any(names)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn read_env_usize_any(names: &[&str], default: usize) -> usize {
    read_env_string_any(names)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}
