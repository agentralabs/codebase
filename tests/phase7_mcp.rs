//! Phase 7: MCP server tests.
//!
//! Tests for the synchronous JSON-RPC 2.0 server that implements the
//! Model Context Protocol for code graph access.

use std::path::PathBuf;

use serde_json::{json, Value};

use agentic_codebase::graph::CodeGraph;
use agentic_codebase::mcp::protocol::parse_request;
use agentic_codebase::mcp::server::McpServer;
use agentic_codebase::types::{CodeUnit, CodeUnitType, Edge, EdgeType, Language, Span};

/// Helper: build a test graph with several units and edges.
fn build_mcp_test_graph() -> CodeGraph {
    let mut graph = CodeGraph::with_default_dimension();

    let unit_a = CodeUnit::new(
        CodeUnitType::Function,
        Language::Rust,
        "process_data".to_string(),
        "app::process_data".to_string(),
        PathBuf::from("src/app.rs"),
        Span::new(10, 0, 30, 0),
    );
    let id_a = graph.add_unit(unit_a);

    let unit_b = CodeUnit::new(
        CodeUnitType::Function,
        Language::Rust,
        "validate_input".to_string(),
        "app::validate_input".to_string(),
        PathBuf::from("src/app.rs"),
        Span::new(35, 0, 50, 0),
    );
    let id_b = graph.add_unit(unit_b);

    let unit_c = CodeUnit::new(
        CodeUnitType::Type,
        Language::Rust,
        "AppConfig".to_string(),
        "config::AppConfig".to_string(),
        PathBuf::from("src/config.rs"),
        Span::new(1, 0, 20, 0),
    );
    let id_c = graph.add_unit(unit_c);

    let unit_d = CodeUnit::new(
        CodeUnitType::Test,
        Language::Rust,
        "test_process".to_string(),
        "tests::test_process".to_string(),
        PathBuf::from("tests/test_app.rs"),
        Span::new(1, 0, 15, 0),
    );
    let id_d = graph.add_unit(unit_d);

    // process_data calls validate_input
    graph
        .add_edge(Edge::new(id_a, id_b, EdgeType::Calls))
        .unwrap();
    // process_data uses AppConfig
    graph
        .add_edge(Edge::new(id_a, id_c, EdgeType::UsesType))
        .unwrap();
    // test_process tests process_data
    graph
        .add_edge(Edge::new(id_d, id_a, EdgeType::Tests))
        .unwrap();

    graph
}

/// Helper: create a McpServer with a test graph loaded.
fn create_test_server() -> McpServer {
    let mut server = McpServer::new();
    server.load_graph("test".to_string(), build_mcp_test_graph());
    server
}

/// Helper: send a raw JSON-RPC request to the server and parse the response.
fn send_request(server: &mut McpServer, request: &Value) -> Value {
    let raw = serde_json::to_string(request).unwrap();
    let response_str = server.handle_raw(&raw);
    serde_json::from_str(&response_str).unwrap()
}

// ============================================================================
// Protocol tests
// ============================================================================

#[test]
fn test_parse_valid_request() {
    let raw = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    let request = parse_request(raw).unwrap();
    assert_eq!(request.method, "initialize");
    assert_eq!(request.id, Some(json!(1)));
}

#[test]
fn test_parse_invalid_json() {
    let raw = r#"not valid json"#;
    let err = parse_request(raw).unwrap_err();
    assert!(err.error.is_some());
    assert_eq!(err.error.as_ref().unwrap().code, -32700);
}

#[test]
fn test_parse_wrong_version() {
    let raw = r#"{"jsonrpc":"1.0","id":1,"method":"test","params":{}}"#;
    let err = parse_request(raw).unwrap_err();
    assert!(err.error.is_some());
    assert_eq!(err.error.as_ref().unwrap().code, -32600);
}

#[test]
fn test_parse_missing_method() {
    let raw = r#"{"jsonrpc":"2.0","id":1}"#;
    let err = parse_request(raw).unwrap_err();
    assert!(err.error.is_some());
}

// ============================================================================
// Initialize / shutdown tests
// ============================================================================

#[test]
fn test_mcp_initialize() {
    let mut server = create_test_server();
    assert!(!server.is_initialized());

    let response = send_request(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    );

    assert!(response.get("result").is_some());
    let result = &response["result"];
    assert!(result.get("protocolVersion").is_some());
    assert!(result.get("capabilities").is_some());
    assert!(result.get("serverInfo").is_some());
    assert_eq!(result["serverInfo"]["name"], "agentic-codebase");
    assert!(server.is_initialized());
}

#[test]
fn test_mcp_shutdown() {
    let mut server = create_test_server();

    // Initialize first.
    send_request(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    );
    assert!(server.is_initialized());

    // Shutdown.
    let response = send_request(
        &mut server,
        &json!({"jsonrpc":"2.0","id":2,"method":"shutdown","params":{}}),
    );

    assert!(response.get("result").is_some());
    assert!(!server.is_initialized());
}

// ============================================================================
// Tools list / call tests
// ============================================================================

#[test]
fn test_mcp_list_tools() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
    );

    let tools = &response["result"]["tools"];
    assert!(tools.is_array());
    let tools_array = tools.as_array().unwrap();
    assert!(tools_array.len() >= 3);

    // Check tool names.
    let tool_names: Vec<&str> = tools_array
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(tool_names.contains(&"symbol_lookup"));
    assert!(tool_names.contains(&"impact_analysis"));
    assert!(tool_names.contains(&"graph_stats"));
}

#[test]
fn test_mcp_tool_symbol_lookup() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "symbol_lookup",
                "arguments": {
                    "graph": "test",
                    "name": "process",
                    "mode": "prefix"
                }
            }
        }),
    );

    assert!(response.get("result").is_some());
    let content = &response["result"]["content"];
    assert!(content.is_array());
    let text = content[0]["text"].as_str().unwrap();
    assert!(text.contains("process_data"));
}

#[test]
fn test_mcp_tool_symbol_lookup_exact() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "symbol_lookup",
                "arguments": {
                    "graph": "test",
                    "name": "validate_input",
                    "mode": "exact"
                }
            }
        }),
    );

    assert!(response.get("result").is_some());
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("validate_input"));
}

#[test]
fn test_mcp_tool_symbol_lookup_no_results() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "symbol_lookup",
                "arguments": {
                    "graph": "test",
                    "name": "nonexistent_symbol",
                    "mode": "exact"
                }
            }
        }),
    );

    assert!(response.get("result").is_some());
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    let results: Vec<Value> = serde_json::from_str(text).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_mcp_tool_impact_analysis() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "impact_analysis",
                "arguments": {
                    "graph": "test",
                    "unit_id": 1,
                    "max_depth": 3
                }
            }
        }),
    );

    assert!(response.get("result").is_some());
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    let impact: Value = serde_json::from_str(text).unwrap();
    assert_eq!(impact["root_id"], 1);
}

#[test]
fn test_mcp_tool_graph_stats() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": {
                    "graph": "test"
                }
            }
        }),
    );

    assert!(response.get("result").is_some());
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    let stats: Value = serde_json::from_str(text).unwrap();
    assert_eq!(stats["unit_count"], 4);
    assert_eq!(stats["edge_count"], 3);
}

#[test]
fn test_mcp_tool_list_units() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "list_units",
                "arguments": {
                    "graph": "test",
                    "limit": 2
                }
            }
        }),
    );

    assert!(response.get("result").is_some());
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    let units: Vec<Value> = serde_json::from_str(text).unwrap();
    assert_eq!(units.len(), 2);
}

#[test]
fn test_mcp_tool_unknown() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "nonexistent_tool",
                "arguments": {}
            }
        }),
    );

    assert!(response.get("error").is_some());
    assert_eq!(response["error"]["code"], -32601);
}

#[test]
fn test_mcp_tool_missing_name() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "arguments": { "graph": "test" }
            }
        }),
    );

    assert!(response.get("error").is_some());
    assert_eq!(response["error"]["code"], -32602);
}

// ============================================================================
// Resources tests
// ============================================================================

#[test]
fn test_mcp_list_resources() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"resources/list","params":{}}),
    );

    let resources = &response["result"]["resources"];
    assert!(resources.is_array());
    let resources_array = resources.as_array().unwrap();
    // Should have stats and units resources for the "test" graph.
    assert!(resources_array.len() >= 2);

    let uris: Vec<&str> = resources_array
        .iter()
        .filter_map(|r| r.get("uri").and_then(|u| u.as_str()))
        .collect();
    assert!(uris.contains(&"acb://graphs/test/stats"));
    assert!(uris.contains(&"acb://graphs/test/units"));
}

#[test]
fn test_mcp_resource_stats() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/read",
            "params": {
                "uri": "acb://graphs/test/stats"
            }
        }),
    );

    assert!(response.get("result").is_some());
    let contents = &response["result"]["contents"];
    assert!(contents.is_array());
    let text = contents[0]["text"].as_str().unwrap();
    let stats: Value = serde_json::from_str(text).unwrap();
    assert_eq!(stats["unit_count"], 4);
}

#[test]
fn test_mcp_resource_units() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/read",
            "params": {
                "uri": "acb://graphs/test/units"
            }
        }),
    );

    assert!(response.get("result").is_some());
    let text = response["result"]["contents"][0]["text"].as_str().unwrap();
    let units: Vec<Value> = serde_json::from_str(text).unwrap();
    assert_eq!(units.len(), 4);
}

#[test]
fn test_mcp_resource_404() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/read",
            "params": {
                "uri": "acb://graphs/nonexistent/stats"
            }
        }),
    );

    assert!(response.get("error").is_some());
    assert_eq!(response["error"]["code"], -32602);
}

#[test]
fn test_mcp_resource_invalid_uri() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/read",
            "params": {
                "uri": "invalid://unknown"
            }
        }),
    );

    assert!(response.get("error").is_some());
}

// ============================================================================
// Prompts tests
// ============================================================================

#[test]
fn test_mcp_list_prompts() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"prompts/list","params":{}}),
    );

    let prompts = &response["result"]["prompts"];
    assert!(prompts.is_array());
    let prompts_array = prompts.as_array().unwrap();
    assert!(prompts_array.len() >= 2);

    let prompt_names: Vec<&str> = prompts_array
        .iter()
        .filter_map(|p| p.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(prompt_names.contains(&"analyse_unit"));
    assert!(prompt_names.contains(&"explain_coupling"));
}

// ============================================================================
// Error handling tests
// ============================================================================

#[test]
fn test_mcp_invalid_method() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"nonexistent/method","params":{}}),
    );

    assert!(response.get("error").is_some());
    assert_eq!(response["error"]["code"], -32601);
}

#[test]
fn test_mcp_invalid_json() {
    let mut server = create_test_server();
    let response_str = server.handle_raw("not valid json");
    let response: Value = serde_json::from_str(&response_str).unwrap();
    assert!(response.get("error").is_some());
    assert_eq!(response["error"]["code"], -32700);
}

// ============================================================================
// Graph management tests
// ============================================================================

#[test]
fn test_mcp_load_unload_graph() {
    let mut server = McpServer::new();
    assert!(server.graph_names().is_empty());

    server.load_graph("test".to_string(), CodeGraph::with_default_dimension());
    assert_eq!(server.graph_names().len(), 1);

    server.unload_graph("test");
    assert!(server.graph_names().is_empty());
}

#[test]
fn test_mcp_no_graphs_loaded() {
    let mut server = McpServer::new();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": {}
            }
        }),
    );

    assert!(response.get("error").is_some());
    assert_eq!(response["error"]["code"], -32602);
}

#[test]
fn test_mcp_default_graph_resolution() {
    let mut server = McpServer::new();
    server.load_graph("default".to_string(), build_mcp_test_graph());

    // No graph specified in arguments — should use the first one.
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": {}
            }
        }),
    );

    assert!(response.get("result").is_some());
}

#[test]
fn test_mcp_impact_analysis_invalid_unit() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "impact_analysis",
                "arguments": {
                    "graph": "test",
                    "unit_id": 9999
                }
            }
        }),
    );

    assert!(response.get("error").is_some());
    assert_eq!(response["error"]["code"], -32603);
}

#[test]
fn test_mcp_symbol_lookup_missing_name_arg() {
    let mut server = create_test_server();
    let response = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "symbol_lookup",
                "arguments": {
                    "graph": "test"
                }
            }
        }),
    );

    assert!(response.get("error").is_some());
    assert_eq!(response["error"]["code"], -32602);
}
