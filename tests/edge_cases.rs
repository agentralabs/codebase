//! Edge case tests for MCP server and protocol handling.
//!
//! Covers malformed input, boundary values, unicode, concurrency,
//! state management, and graceful degradation — matching ecosystem
//! testing conventions from AgenticMemory and AgenticVision.

use std::path::PathBuf;

use serde_json::{json, Value};

use agentic_codebase::graph::CodeGraph;
use agentic_codebase::mcp::protocol::parse_request;
use agentic_codebase::mcp::server::McpServer;
use agentic_codebase::types::{CodeUnit, CodeUnitType, Edge, EdgeType, Language, Span};

// ============================================================================
// Helpers
// ============================================================================

fn build_edge_test_graph() -> CodeGraph {
    let mut graph = CodeGraph::with_default_dimension();

    let unit_a = CodeUnit::new(
        CodeUnitType::Function,
        Language::Python,
        "process_data".to_string(),
        "app.process_data".to_string(),
        PathBuf::from("src/app.py"),
        Span::new(1, 0, 20, 0),
    );
    let id_a = graph.add_unit(unit_a);

    let unit_b = CodeUnit::new(
        CodeUnitType::Function,
        Language::Python,
        "validate".to_string(),
        "app.validate".to_string(),
        PathBuf::from("src/app.py"),
        Span::new(25, 0, 40, 0),
    );
    let id_b = graph.add_unit(unit_b);

    let unit_c = CodeUnit::new(
        CodeUnitType::Type,
        Language::Rust,
        "Config".to_string(),
        "config::Config".to_string(),
        PathBuf::from("src/config.rs"),
        Span::new(1, 0, 30, 0),
    );
    let id_c = graph.add_unit(unit_c);

    graph
        .add_edge(Edge::new(id_a, id_b, EdgeType::Calls))
        .unwrap();
    graph
        .add_edge(Edge::new(id_a, id_c, EdgeType::UsesType))
        .unwrap();
    graph
}

fn create_server() -> McpServer {
    let mut server = McpServer::new();
    server.load_graph("test".to_string(), build_edge_test_graph());
    server
}

fn send(server: &mut McpServer, request: &Value) -> Value {
    let raw = serde_json::to_string(request).unwrap();
    let response_str = server.handle_raw(&raw);
    serde_json::from_str(&response_str).unwrap()
}

// ============================================================================
// JSON-RPC protocol edge cases
// ============================================================================

#[test]
fn edge_empty_string_input() {
    let mut server = create_server();
    let response_str = server.handle_raw("");
    let response: Value = serde_json::from_str(&response_str).unwrap();
    assert!(response.get("error").is_some());
    assert_eq!(response["error"]["code"], -32700); // Parse error
}

#[test]
fn edge_whitespace_only_input() {
    let mut server = create_server();
    let response_str = server.handle_raw("   \n\t  ");
    let response: Value = serde_json::from_str(&response_str).unwrap();
    assert!(response.get("error").is_some());
    assert_eq!(response["error"]["code"], -32700);
}

#[test]
fn edge_null_json_input() {
    let mut server = create_server();
    let response_str = server.handle_raw("null");
    let response: Value = serde_json::from_str(&response_str).unwrap();
    assert!(response.get("error").is_some());
}

#[test]
fn edge_array_json_input() {
    let mut server = create_server();
    let response_str = server.handle_raw("[1, 2, 3]");
    let response: Value = serde_json::from_str(&response_str).unwrap();
    assert!(response.get("error").is_some());
}

#[test]
fn edge_numeric_json_input() {
    let mut server = create_server();
    let response_str = server.handle_raw("42");
    let response: Value = serde_json::from_str(&response_str).unwrap();
    assert!(response.get("error").is_some());
}

#[test]
fn edge_truncated_json() {
    let mut server = create_server();
    let response_str = server.handle_raw(r#"{"jsonrpc":"2.0","id":1,"method":"#);
    let response: Value = serde_json::from_str(&response_str).unwrap();
    assert!(response.get("error").is_some());
    assert_eq!(response["error"]["code"], -32700);
}

#[test]
fn edge_wrong_jsonrpc_version() {
    let raw = r#"{"jsonrpc":"1.0","id":1,"method":"initialize","params":{}}"#;
    let err = parse_request(raw).unwrap_err();
    assert_eq!(err.error.as_ref().unwrap().code, -32600);
}

#[test]
fn edge_missing_jsonrpc_field() {
    let raw = r#"{"id":1,"method":"initialize","params":{}}"#;
    let err = parse_request(raw).unwrap_err();
    assert!(err.error.is_some());
}

#[test]
fn edge_notification_without_id_is_accepted() {
    let raw = r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#;
    let request = parse_request(raw).unwrap();
    assert!(request.id.is_none());
    assert_eq!(request.method, "notifications/initialized");
}

#[test]
fn edge_empty_method_name() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"","params":{}}),
    );
    assert!(response.get("error").is_some());
}

#[test]
fn edge_string_id_in_request() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":"string-id-123","method":"tools/list","params":{}}),
    );
    // Should succeed — MCP allows string IDs.
    assert!(response.get("result").is_some());
    assert_eq!(response["id"], "string-id-123");
}

#[test]
fn edge_null_id_in_request() {
    let mut server = create_server();
    let response_str =
        server.handle_raw(r#"{"jsonrpc":"2.0","id":null,"method":"tools/list","params":{}}"#);
    assert!(response_str.is_empty());
}

#[test]
fn edge_notifications_emit_no_response_frame() {
    let mut server = create_server();
    let response_str =
        server.handle_raw(r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#);
    assert!(response_str.is_empty());
    assert!(server.is_initialized());
}

#[test]
fn edge_negative_numeric_id() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":-1,"method":"tools/list","params":{}}),
    );
    assert!(response.get("result").is_some());
    assert_eq!(response["id"], -1);
}

#[test]
fn edge_very_large_id() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":999999999,"method":"tools/list","params":{}}),
    );
    assert!(response.get("result").is_some());
}

// ============================================================================
// Method edge cases
// ============================================================================

#[test]
fn edge_method_with_extra_slashes() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"tools///list","params":{}}),
    );
    assert!(response.get("error").is_some());
    assert_eq!(response["error"]["code"], -32601);
}

#[test]
fn edge_method_case_sensitive() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"TOOLS/LIST","params":{}}),
    );
    // Methods should be case-sensitive per JSON-RPC.
    assert!(response.get("error").is_some());
    assert_eq!(response["error"]["code"], -32601);
}

#[test]
fn edge_initialize_twice() {
    let mut server = create_server();
    let r1 = send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    );
    assert!(r1.get("result").is_some());
    assert!(server.is_initialized());

    // Second initialize should still work (idempotent).
    let r2 = send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{}}),
    );
    assert!(r2.get("result").is_some());
}

#[test]
fn edge_shutdown_without_initialize() {
    let mut server = create_server();
    assert!(!server.is_initialized());

    let response = send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"shutdown","params":{}}),
    );
    // Should handle gracefully.
    assert!(response.get("result").is_some() || response.get("error").is_some());
}

// ============================================================================
// Tool argument edge cases
// ============================================================================

#[test]
fn edge_symbol_lookup_empty_name() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "symbol_lookup",
                "arguments": { "graph": "test", "name": "", "mode": "exact" }
            }
        }),
    );
    // Empty name should return empty results, not an error.
    assert!(response.get("result").is_some());
}

#[test]
fn edge_symbol_lookup_unicode_name() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "symbol_lookup",
                "arguments": { "graph": "test", "name": "日本語クラス", "mode": "contains" }
            }
        }),
    );
    assert!(response.get("result").is_some());
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    let results: Vec<Value> = serde_json::from_str(text).unwrap();
    assert!(results.is_empty());
}

#[test]
fn edge_symbol_lookup_very_long_name() {
    let mut server = create_server();
    let long_name = "a".repeat(10_000);
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "symbol_lookup",
                "arguments": { "graph": "test", "name": long_name, "mode": "exact" }
            }
        }),
    );
    // Should not panic, just return no results.
    assert!(response.get("result").is_some());
}

#[test]
fn edge_symbol_lookup_special_chars() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "symbol_lookup",
                "arguments": { "graph": "test", "name": "<script>alert(1)</script>", "mode": "contains" }
            }
        }),
    );
    assert!(response.get("result").is_some());
}

#[test]
fn edge_symbol_lookup_invalid_mode() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "symbol_lookup",
                "arguments": { "graph": "test", "name": "x", "mode": "invalid_mode" }
            }
        }),
    );
    // Should either error or default to a reasonable mode.
    assert!(response.get("result").is_some() || response.get("error").is_some());
}

#[test]
fn edge_impact_analysis_unit_id_zero() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "impact_analysis",
                "arguments": { "graph": "test", "unit_id": 0, "max_depth": 3 }
            }
        }),
    );
    // Unit 0 exists in our test graph.
    assert!(response.get("result").is_some());
}

#[test]
fn edge_impact_analysis_very_large_unit_id() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "impact_analysis",
                "arguments": { "graph": "test", "unit_id": 999999, "max_depth": 3 }
            }
        }),
    );
    assert!(response.get("error").is_some());
}

#[test]
fn edge_impact_analysis_zero_depth() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "impact_analysis",
                "arguments": { "graph": "test", "unit_id": 0, "max_depth": 0 }
            }
        }),
    );
    // Zero depth = only the root, no transitive impact.
    assert!(response.get("result").is_some());
}

#[test]
fn edge_impact_analysis_negative_depth() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "impact_analysis",
                "arguments": { "graph": "test", "unit_id": 0, "max_depth": -1 }
            }
        }),
    );
    // Negative depth should not cause a panic.
    assert!(response.get("result").is_some() || response.get("error").is_some());
}

#[test]
fn edge_list_units_zero_limit() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "list_units",
                "arguments": { "graph": "test", "limit": 0 }
            }
        }),
    );
    assert!(response.get("result").is_some());
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    let units: Vec<Value> = serde_json::from_str(text).unwrap();
    assert!(units.is_empty());
}

#[test]
fn edge_list_units_very_large_limit() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "list_units",
                "arguments": { "graph": "test", "limit": 999999 }
            }
        }),
    );
    assert!(response.get("result").is_some());
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    let units: Vec<Value> = serde_json::from_str(text).unwrap();
    // Should return all units, not panic.
    assert_eq!(units.len(), 3);
}

// ============================================================================
// Graph management edge cases
// ============================================================================

#[test]
fn edge_unload_nonexistent_graph() {
    let mut server = McpServer::new();
    // Should not panic.
    server.unload_graph("nonexistent");
    assert!(server.graph_names().is_empty());
}

#[test]
fn edge_load_graph_overwrite() {
    let mut server = McpServer::new();
    server.load_graph("test".to_string(), CodeGraph::with_default_dimension());
    assert_eq!(server.graph_names().len(), 1);

    // Load with the same name — should overwrite.
    server.load_graph("test".to_string(), build_edge_test_graph());
    assert_eq!(server.graph_names().len(), 1);

    // Verify the new graph is active.
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": { "graph": "test" }
            }
        }),
    );
    assert!(response.get("result").is_some());
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    let stats: Value = serde_json::from_str(text).unwrap();
    assert_eq!(stats["unit_count"], 3);
}

#[test]
fn edge_load_multiple_graphs() {
    let mut server = McpServer::new();
    server.load_graph("alpha".to_string(), build_edge_test_graph());
    server.load_graph("beta".to_string(), CodeGraph::with_default_dimension());
    assert_eq!(server.graph_names().len(), 2);

    // Query specific graph.
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": { "graph": "alpha" }
            }
        }),
    );
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    let stats: Value = serde_json::from_str(text).unwrap();
    assert_eq!(stats["unit_count"], 3);

    // Query the empty graph.
    let response2 = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 2,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": { "graph": "beta" }
            }
        }),
    );
    let text2 = response2["result"]["content"][0]["text"].as_str().unwrap();
    let stats2: Value = serde_json::from_str(text2).unwrap();
    assert_eq!(stats2["unit_count"], 0);
}

#[test]
fn edge_query_empty_graph() {
    let mut server = McpServer::new();
    server.load_graph("empty".to_string(), CodeGraph::with_default_dimension());

    // Symbol lookup on empty graph.
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "symbol_lookup",
                "arguments": { "graph": "empty", "name": "anything", "mode": "contains" }
            }
        }),
    );
    assert!(response.get("result").is_some());
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    let results: Vec<Value> = serde_json::from_str(text).unwrap();
    assert!(results.is_empty());
}

#[test]
fn edge_query_wrong_graph_name() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": { "graph": "nonexistent" }
            }
        }),
    );
    assert!(response.get("error").is_some());
    assert_eq!(response["error"]["code"], -32602);
}

// ============================================================================
// Resource edge cases
// ============================================================================

#[test]
fn edge_resource_malformed_uri_scheme() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "resources/read",
            "params": { "uri": "http://graphs/test/stats" }
        }),
    );
    assert!(response.get("error").is_some());
}

#[test]
fn edge_resource_empty_uri() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "resources/read",
            "params": { "uri": "" }
        }),
    );
    assert!(response.get("error").is_some());
}

#[test]
fn edge_resource_missing_uri() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "resources/read",
            "params": {}
        }),
    );
    assert!(response.get("error").is_some());
}

#[test]
fn edge_resource_partial_path() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "resources/read",
            "params": { "uri": "acb://graphs/test" }
        }),
    );
    // Incomplete path — should fail gracefully.
    assert!(response.get("error").is_some());
}

#[test]
fn edge_resource_unicode_graph_name() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "resources/read",
            "params": { "uri": "acb://graphs/日本語/stats" }
        }),
    );
    assert!(response.get("error").is_some());
}

// ============================================================================
// Concurrent / stress edge cases
// ============================================================================

#[test]
fn edge_rapid_tool_calls() {
    let mut server = create_server();
    // 100 rapid tool calls should not cause any issues.
    for i in 0..100 {
        let response = send(
            &mut server,
            &json!({
                "jsonrpc": "2.0", "id": i,
                "method": "tools/call",
                "params": {
                    "name": "graph_stats",
                    "arguments": { "graph": "test" }
                }
            }),
        );
        assert!(response.get("result").is_some());
    }
}

#[test]
fn edge_rapid_load_unload_cycle() {
    let mut server = McpServer::new();
    for i in 0..50 {
        let name = format!("graph_{}", i);
        server.load_graph(name.clone(), build_edge_test_graph());
        server.unload_graph(&name);
    }
    assert!(server.graph_names().is_empty());
}

#[test]
fn edge_interleaved_operations() {
    let mut server = McpServer::new();

    // Load a graph, query it, load another, query both, unload first, query second.
    server.load_graph("first".to_string(), build_edge_test_graph());
    let r1 = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": { "graph": "first" }
            }
        }),
    );
    assert!(r1.get("result").is_some());

    server.load_graph("second".to_string(), CodeGraph::with_default_dimension());

    let r2 = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 2,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": { "graph": "second" }
            }
        }),
    );
    assert!(r2.get("result").is_some());

    server.unload_graph("first");

    // First graph should now be gone.
    let r3 = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 3,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": { "graph": "first" }
            }
        }),
    );
    assert!(r3.get("error").is_some());

    // Second should still work.
    let r4 = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 4,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": { "graph": "second" }
            }
        }),
    );
    assert!(r4.get("result").is_some());
}

// ============================================================================
// Full lifecycle edge case
// ============================================================================

#[test]
fn edge_full_lifecycle() {
    let mut server = McpServer::new();
    assert!(!server.is_initialized());

    // Initialize.
    let r = send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    );
    assert!(r.get("result").is_some());
    assert!(server.is_initialized());

    // List tools on empty server.
    let r = send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    );
    assert!(r["result"]["tools"].as_array().unwrap().len() >= 3);

    // Query without graphs loaded.
    let r = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 3,
            "method": "tools/call",
            "params": { "name": "graph_stats", "arguments": {} }
        }),
    );
    assert!(r.get("error").is_some());

    // Load graph.
    server.load_graph("lifecycle".to_string(), build_edge_test_graph());

    // List resources.
    let r = send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":4,"method":"resources/list","params":{}}),
    );
    assert!(r["result"]["resources"].as_array().unwrap().len() >= 2);

    // Query.
    let r = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 5,
            "method": "tools/call",
            "params": {
                "name": "symbol_lookup",
                "arguments": { "graph": "lifecycle", "name": "Config", "mode": "exact" }
            }
        }),
    );
    assert!(r.get("result").is_some());
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Config"));

    // Unload graph.
    server.unload_graph("lifecycle");

    // Query again — should fail.
    let r = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 6,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": { "graph": "lifecycle" }
            }
        }),
    );
    assert!(r.get("error").is_some());

    // Shutdown.
    let r = send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":7,"method":"shutdown","params":{}}),
    );
    assert!(r.get("result").is_some());
    assert!(!server.is_initialized());
}

// ============================================================================
// Null / missing params edge cases
// ============================================================================

#[test]
fn edge_tool_call_null_arguments() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": { "name": "graph_stats", "arguments": null }
        }),
    );
    // Should treat null arguments as empty.
    assert!(response.get("result").is_some() || response.get("error").is_some());
}

#[test]
fn edge_tool_call_missing_arguments() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": { "name": "graph_stats" }
        }),
    );
    // Missing arguments should be handled gracefully.
    assert!(response.get("result").is_some() || response.get("error").is_some());
}

#[test]
fn edge_tool_call_extra_arguments_ignored() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": { "graph": "test", "extra_key": "extra_value", "unused": 42 }
            }
        }),
    );
    // Extra arguments should be ignored.
    assert!(response.get("result").is_some());
}

#[test]
fn edge_params_as_null() {
    let mut server = create_server();
    let response = send(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":null}),
    );
    // Null params should be treated as empty.
    assert!(response.get("result").is_some());
}
