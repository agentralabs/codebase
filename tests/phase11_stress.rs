//! Phase 11: Stress tests for context capture — analysis_log, operation tracking,
//! scale, edge cases, and regression.

use std::path::PathBuf;

use serde_json::{json, Value};

use agentic_codebase::graph::CodeGraph;
use agentic_codebase::mcp::server::McpServer;
use agentic_codebase::types::{CodeUnit, CodeUnitType, Language, Span};

// ─────────────────────── helpers ───────────────────────

fn build_test_graph() -> CodeGraph {
    let mut graph = CodeGraph::with_default_dimension();
    let unit = CodeUnit::new(
        CodeUnitType::Function,
        Language::Rust,
        "process".to_string(),
        "app::process".to_string(),
        PathBuf::from("src/app.rs"),
        Span::new(10, 0, 30, 0),
    );
    graph.add_unit(unit);
    graph
}

fn create_test_server() -> McpServer {
    let mut server = McpServer::new();
    server.load_graph("test".to_string(), build_test_graph());
    server
}

fn send_request(server: &mut McpServer, request: &Value) -> Value {
    let raw = serde_json::to_string(request).unwrap();
    let response_str = server.handle_raw(&raw);
    serde_json::from_str(&response_str).unwrap()
}

fn init_server(server: &mut McpServer) {
    send_request(
        server,
        &json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}),
    );
}

// ============================================================================
// 1. analysis_log Tool — Context Capture
// ============================================================================

#[test]
fn test_analysis_log_basic() {
    let mut server = create_test_server();
    init_server(&mut server);

    let resp = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "analysis_log",
                "arguments": {
                    "intent": "Investigating memory leak in module X",
                    "finding": "Found unbounded Vec growth in process()",
                    "graph": "test",
                    "topic": "performance"
                }
            }
        }),
    );

    assert!(resp.get("result").is_some(), "Should succeed: {resp}");
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let parsed: Value = serde_json::from_str(text).unwrap();
    assert!(parsed["log_index"].as_u64().is_some());
    assert_eq!(parsed["message"], "Analysis context logged");
}

#[test]
fn test_analysis_log_intent_only() {
    let mut server = create_test_server();
    init_server(&mut server);

    let resp = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "analysis_log",
                "arguments": {
                    "intent": "Exploring module dependencies"
                }
            }
        }),
    );

    assert!(resp.get("result").is_some());
}

#[test]
fn test_analysis_log_empty_intent_fails() {
    let mut server = create_test_server();
    init_server(&mut server);

    let resp = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "analysis_log",
                "arguments": {
                    "intent": ""
                }
            }
        }),
    );

    // Should be an error
    assert!(
        resp.get("error").is_some(),
        "Empty intent should be rejected: {resp}"
    );
}

#[test]
fn test_analysis_log_missing_intent_fails() {
    let mut server = create_test_server();
    init_server(&mut server);

    let resp = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "analysis_log",
                "arguments": {
                    "finding": "Some finding without intent"
                }
            }
        }),
    );

    assert!(
        resp.get("error").is_some(),
        "Missing intent should be rejected: {resp}"
    );
}

// ============================================================================
// 2. Operation Log — Auto-Capture
// ============================================================================

#[test]
fn test_operation_log_captures_tool_calls() {
    let mut server = create_test_server();
    init_server(&mut server);

    // Call symbol_lookup
    send_request(
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

    // Call graph_stats
    send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": { "graph": "test" }
            }
        }),
    );

    let log = server.operation_log();
    assert!(
        log.len() >= 2,
        "Operation log should have at least 2 entries, got {}",
        log.len()
    );
    assert!(log.iter().any(|r| r.tool_name == "symbol_lookup"));
    assert!(log.iter().any(|r| r.tool_name == "graph_stats"));
}

#[test]
fn test_analysis_log_self_stores_in_operation_log() {
    let mut server = create_test_server();
    init_server(&mut server);

    // Call analysis_log
    send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "analysis_log",
                "arguments": { "intent": "Test intent" }
            }
        }),
    );

    let log = server.operation_log();
    // analysis_log stores its own record (self-logged), but should NOT be
    // double-logged by the auto-capture path.
    let count = log.iter().filter(|r| r.tool_name == "analysis_log").count();
    assert_eq!(
        count, 1,
        "analysis_log should appear exactly once (self-logged, not auto-logged)"
    );
}

#[test]
fn test_session_tracking_on_initialize() {
    let mut server = create_test_server();

    // Before initialize, operation log should be empty
    assert!(server.operation_log().is_empty());

    // Initialize
    init_server(&mut server);

    // Operation log should still be empty (no tools called yet)
    assert!(server.operation_log().is_empty());
}

// ============================================================================
// 3. Scale Tests
// ============================================================================

#[test]
fn test_scale_500_tool_calls_logged() {
    let mut server = create_test_server();
    init_server(&mut server);

    let start = std::time::Instant::now();

    for i in 0..500 {
        send_request(
            &mut server,
            &json!({
                "jsonrpc": "2.0",
                "id": i + 1,
                "method": "tools/call",
                "params": {
                    "name": "graph_stats",
                    "arguments": { "graph": "test" }
                }
            }),
        );
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "500 graph_stats calls took {:?} — too slow",
        elapsed
    );

    let log = server.operation_log();
    assert_eq!(log.len(), 500);
}

#[test]
fn test_scale_200_analysis_logs() {
    let mut server = create_test_server();
    init_server(&mut server);

    let start = std::time::Instant::now();

    for i in 0..200 {
        send_request(
            &mut server,
            &json!({
                "jsonrpc": "2.0",
                "id": i + 1,
                "method": "tools/call",
                "params": {
                    "name": "analysis_log",
                    "arguments": {
                        "intent": format!("Analysis intent {i}"),
                        "finding": format!("Finding {i}"),
                        "topic": format!("topic-{}", i % 10)
                    }
                }
            }),
        );
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "200 analysis_log calls took {:?} — too slow",
        elapsed
    );
}

// ============================================================================
// 4. Edge Cases
// ============================================================================

#[test]
fn test_analysis_log_unicode() {
    let mut server = create_test_server();
    init_server(&mut server);

    let resp = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "analysis_log",
                "arguments": {
                    "intent": "分析代码依赖关系",
                    "finding": "依存関係を検出しました",
                    "topic": "국제화"
                }
            }
        }),
    );

    assert!(resp.get("result").is_some());
}

#[test]
fn test_analysis_log_special_chars() {
    let mut server = create_test_server();
    init_server(&mut server);

    let resp = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "analysis_log",
                "arguments": {
                    "intent": "Check \"quotes\" and 'apostrophes' & <angle> brackets",
                    "finding": "Found: \\ backslash, \t tab"
                }
            }
        }),
    );

    assert!(resp.get("result").is_some());
}

#[test]
fn test_analysis_log_very_long_intent() {
    let mut server = create_test_server();
    init_server(&mut server);

    let long_intent = "X".repeat(10_000);
    let resp = send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "analysis_log",
                "arguments": {
                    "intent": long_intent
                }
            }
        }),
    );

    assert!(resp.get("result").is_some());
}

// ============================================================================
// 5. Regression — tool list includes analysis_log
// ============================================================================

#[test]
fn test_tool_list_includes_analysis_log() {
    let mut server = create_test_server();

    let resp = send_request(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
    );

    let tools = resp["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();

    assert!(
        names.contains(&"analysis_log"),
        "Tool list must include analysis_log, found: {:?}",
        names
    );
}

#[test]
fn test_tool_count_is_17() {
    let mut server = create_test_server();

    let resp = send_request(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
    );

    let tools = resp["result"]["tools"].as_array().unwrap();
    assert_eq!(
        tools.len(),
        70,
        "Should have 70 tools (17 original + 28 invention 1-12 + 25 invention 13-17 tools)"
    );
}

// ============================================================================
// 6. Operation Log Summary Quality
// ============================================================================

#[test]
fn test_operation_log_has_timestamps() {
    let mut server = create_test_server();
    init_server(&mut server);

    send_request(
        &mut server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "graph_stats",
                "arguments": { "graph": "test" }
            }
        }),
    );

    let log = server.operation_log();
    assert!(!log.is_empty());
    assert!(log[0].timestamp > 0, "Should have a non-zero timestamp");
}

#[test]
fn test_operation_log_has_graph_names() {
    let mut server = create_test_server();
    init_server(&mut server);

    send_request(
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

    let log = server.operation_log();
    assert!(!log.is_empty());
    // The summary should capture some info about the call
    assert!(!log[0].summary.is_empty(), "Summary should be non-empty");
}
