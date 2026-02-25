//! MCP server implementation.
//!
//! Synchronous JSON-RPC 2.0 server that exposes code graph operations
//! through the Model Context Protocol. All operations are in-process
//! with no async runtime required.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::engine::query::{ImpactParams, MatchMode, SymbolLookupParams};
use crate::engine::QueryEngine;
use crate::graph::CodeGraph;
use crate::grounding::{Grounded, GroundingEngine, GroundingResult};
use crate::types::{CodeUnitType, EdgeType};
use crate::workspace::{
    ContextRole, TranslationMap, TranslationStatus, WorkspaceManager,
};

use super::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

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
        }
    }

    /// Load a code graph into the server under the given name.
    pub fn load_graph(&mut self, name: String, graph: CodeGraph) {
        self.graphs.insert(name, graph);
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
        JsonRpcResponse::success(
            id,
            json!({
                "tools": [
                    {
                        "name": "symbol_lookup",
                        "description": "Look up symbols by name in the code graph.",
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
                        "description": "Analyse the impact of changing a code unit.",
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
                        "description": "Get summary statistics about a loaded code graph.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "graph": { "type": "string", "description": "Graph name" }
                            }
                        }
                    },
                    {
                        "name": "list_units",
                        "description": "List code units in a graph, optionally filtered by type.",
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
                        "description": "Log the intent and context behind a code analysis. Call this to record WHY you are performing a lookup or analysis.",
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
                    // ── Grounding tools ──────────────────────────────────
                    {
                        "name": "codebase_ground",
                        "description": "Verify a claim about code has graph evidence. Use before asserting code exists.",
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
                        "description": "Get graph evidence for a symbol name.",
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
                        "description": "Find symbols similar to a name (for corrections).",
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
                        "description": "Create a workspace to load multiple codebases.",
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
                        "description": "Add a codebase to an existing workspace.",
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
                        "description": "List all contexts in a workspace.",
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
                        "description": "Search across all codebases in workspace.",
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
                        "description": "Compare a symbol between source and target.",
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
                        "description": "Find where symbol exists/doesn't exist across contexts.",
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
                        "description": "Record source→target symbol mapping.",
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
                        "description": "Get migration progress statistics.",
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
                        "description": "List symbols not yet ported.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "workspace": { "type": "string", "description": "Workspace name or id" },
                                "module": { "type": "string", "description": "Filter by module (optional)" }
                            },
                            "required": ["workspace"]
                        }
                    }
                ]
            }),
        )
    }

    /// Handle "tools/call".
    fn handle_tools_call(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
        let tool_name = match params.get("name").and_then(|v| v.as_str()) {
            Some(name) => name,
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

        let result = match tool_name {
            "symbol_lookup" => self.tool_symbol_lookup(id.clone(), &arguments),
            "impact_analysis" => self.tool_impact_analysis(id.clone(), &arguments),
            "graph_stats" => self.tool_graph_stats(id.clone(), &arguments),
            "list_units" => self.tool_list_units(id.clone(), &arguments),
            "analysis_log" => return self.tool_analysis_log(id, &arguments),
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
            _ => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::method_not_found(format!("Unknown tool: {}", tool_name)),
                );
            }
        };

        // Auto-log the tool call (skip analysis_log to avoid recursion).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let summary = truncate_json_summary(&arguments, 200);
        let graph_name = arguments
            .get("graph")
            .and_then(|v| v.as_str())
            .map(String::from);
        self.operation_log.push(OperationRecord {
            tool_name: tool_name.to_string(),
            summary,
            timestamp: now,
            graph_name,
        });

        result
    }

    /// Handle "resources/list".
    fn handle_resources_list(&self, id: Value) -> JsonRpcResponse {
        let mut resources = Vec::new();

        for name in self.graphs.keys() {
            resources.push(json!({
                "uri": format!("acb://graphs/{}/stats", name),
                "name": format!("{} statistics", name),
                "description": format!("Statistics for the {} code graph.", name),
                "mimeType": "application/json"
            }));
            resources.push(json!({
                "uri": format!("acb://graphs/{}/units", name),
                "name": format!("{} units", name),
                "description": format!("All code units in the {} graph.", name),
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
                        "description": "Analyse a code unit including its dependencies, stability, and test coverage.",
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
                        "description": "Explain coupling between two code units.",
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
            return Err(JsonRpcError::invalid_params("No graphs loaded"));
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

        let strict = args.get("strict").and_then(|v| v.as_bool()).unwrap_or(false);

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
                .ok_or_else(|| JsonRpcError::invalid_params("No workspace specified and none active"));
        }

        // If it looks like a workspace ID (starts with "ws-"), use directly.
        if raw.starts_with("ws-") {
            // Validate it exists.
            self.workspace_manager
                .list(raw)
                .map(|_| raw.to_string())
                .map_err(|e| JsonRpcError::invalid_params(e))
        } else {
            // Try to find by name — iterate all workspaces. We need to expose
            // this through the manager. For now, just treat it as an ID.
            self.workspace_manager
                .list(raw)
                .map(|_| raw.to_string())
                .map_err(|e| JsonRpcError::invalid_params(e))
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
        let role = match ContextRole::from_str(role_str) {
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
        let status = match TranslationStatus::from_str(status_str) {
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

        let notes = args
            .get("notes")
            .and_then(|v| v.as_str())
            .map(String::from);

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

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}
