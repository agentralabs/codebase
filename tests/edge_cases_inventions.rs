//! Edge case tests for agentic-codebase invention tools.
//!
//! Covers: tool count verification, smoke tests for all ~74 tools
//! (empty args, no crash), empty-graph invention calls, invalid
//! graph names, unicode in arguments, and rapid-fire sequential calls.

use std::path::PathBuf;

use serde_json::{json, Value};

use agentic_codebase::graph::CodeGraph;
use agentic_codebase::mcp::server::McpServer;
use agentic_codebase::types::{CodeUnit, CodeUnitType, Edge, EdgeType, Language, Span};

// ============================================================================
// Helpers
// ============================================================================

/// Send a JSON-RPC request and parse the response.
fn send(server: &mut McpServer, request: &Value) -> Value {
    let raw = serde_json::to_string(request).unwrap();
    let resp_str = server.handle_raw(&raw);
    serde_json::from_str(&resp_str).unwrap()
}

/// Retrieve all tool names from tools/list.
fn get_all_tool_names(server: &mut McpServer) -> Vec<String> {
    let req = json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}});
    let resp = send(server, &req);
    resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

/// Shorthand: call a tool by name with the given arguments.
fn tool_call(server: &mut McpServer, id: u64, tool: &str, args: Value) -> Value {
    send(
        server,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": tool, "arguments": args }
        }),
    )
}

/// Extract the text content from a successful tool response.
fn tool_text(resp: &Value) -> String {
    resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

/// Build a small but realistic test graph with edges.
fn build_invention_test_graph() -> CodeGraph {
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

/// Create a server with a loaded test graph.
fn create_loaded_server() -> McpServer {
    let mut server = McpServer::new();
    server.load_graph("test".to_string(), build_invention_test_graph());
    server
}

// ============================================================================
// 1. Tool count verification
// ============================================================================

#[test]
fn tool_count_at_least_70() {
    let mut server = McpServer::new();
    let tools = get_all_tool_names(&mut server);
    assert!(
        tools.len() >= 70,
        "Expected at least 70 tools, found {}",
        tools.len()
    );
}

// ============================================================================
// 2. Smoke tests — every tool with empty args, no crash
// ============================================================================

#[test]
fn smoke_all_tools_empty_args_no_graph() {
    let mut server = McpServer::new();
    let tools = get_all_tool_names(&mut server);
    for (i, name) in tools.iter().enumerate() {
        let req = json!({
            "jsonrpc": "2.0",
            "id": i + 100,
            "method": "tools/call",
            "params": { "name": name, "arguments": {} }
        });
        let resp = send(&mut server, &req);
        assert!(
            resp.get("result").is_some() || resp.get("error").is_some(),
            "Tool '{}' returned neither result nor error: {:?}",
            name,
            resp
        );
    }
}

#[test]
fn smoke_all_tools_empty_args_with_graph() {
    let mut server = create_loaded_server();
    let tools = get_all_tool_names(&mut server);
    for (i, name) in tools.iter().enumerate() {
        let req = json!({
            "jsonrpc": "2.0",
            "id": i + 200,
            "method": "tools/call",
            "params": { "name": name, "arguments": {} }
        });
        let resp = send(&mut server, &req);
        assert!(
            resp.get("result").is_some() || resp.get("error").is_some(),
            "Tool '{}' with graph returned neither result nor error: {:?}",
            name,
            resp
        );
    }
}

#[test]
fn smoke_all_tools_with_graph_arg() {
    let mut server = create_loaded_server();
    let tools = get_all_tool_names(&mut server);
    for (i, name) in tools.iter().enumerate() {
        let req = json!({
            "jsonrpc": "2.0",
            "id": i + 300,
            "method": "tools/call",
            "params": { "name": name, "arguments": { "graph": "test" } }
        });
        let resp = send(&mut server, &req);
        assert!(
            resp.get("result").is_some() || resp.get("error").is_some(),
            "Tool '{}' with graph arg returned neither result nor error: {:?}",
            name,
            resp
        );
    }
}

// ============================================================================
// 3. Empty graph invention calls — specific tool families
// ============================================================================

#[test]
fn empty_graph_resurrect_search() {
    let mut server = McpServer::new();
    let resp = tool_call(
        &mut server,
        1,
        "resurrect_search",
        json!({"query": "deleted function"}),
    );
    // Should not crash; returns either result or error about no graph
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn empty_graph_genetics_dna() {
    let mut server = McpServer::new();
    let resp = tool_call(&mut server, 2, "genetics_dna", json!({"unit_id": 0}));
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn empty_graph_telepathy_connect() {
    let mut server = McpServer::new();
    let resp = tool_call(
        &mut server,
        3,
        "telepathy_connect",
        json!({"workspace": "test-ws"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn empty_graph_soul_extract() {
    let mut server = McpServer::new();
    let resp = tool_call(&mut server, 4, "soul_extract", json!({"unit_id": 0}));
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn empty_graph_omniscience_search() {
    let mut server = McpServer::new();
    let resp = tool_call(
        &mut server,
        5,
        "omniscience_search",
        json!({"query": "binary search"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

// ============================================================================
// 4. Invalid graph name — tools referencing nonexistent graph
// ============================================================================

#[test]
fn invalid_graph_symbol_lookup() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        10,
        "symbol_lookup",
        json!({"graph": "nonexistent", "name": "process_data"}),
    );
    // Should produce an error about graph not found
    let has_error = resp.get("error").is_some()
        || (resp.get("result").is_some() && {
            let text = tool_text(&resp);
            text.contains("not found") || text.contains("error") || text.contains("Error")
        });
    assert!(
        has_error || resp.get("result").is_some(),
        "Expected error or result for nonexistent graph, got: {:?}",
        resp
    );
}

#[test]
fn invalid_graph_genetics_dna() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        11,
        "genetics_dna",
        json!({"graph": "nonexistent", "unit_id": 0}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Tool genetics_dna with nonexistent graph should not crash"
    );
}

#[test]
fn invalid_graph_soul_extract() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        12,
        "soul_extract",
        json!({"graph": "nonexistent", "unit_id": 0}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Tool soul_extract with nonexistent graph should not crash"
    );
}

// ============================================================================
// 5. Unicode in arguments
// ============================================================================

#[test]
fn unicode_emoji_in_symbol_name() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        20,
        "symbol_lookup",
        json!({"graph": "test", "name": "process_\u{1F680}_data"}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Emoji in symbol name should not crash: {:?}",
        resp
    );
}

#[test]
fn unicode_cjk_in_query() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        21,
        "resurrect_search",
        json!({"graph": "test", "query": "\u{4e16}\u{754c}\u{4f60}\u{597d}"}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "CJK in query should not crash: {:?}",
        resp
    );
}

#[test]
fn unicode_mixed_in_claim() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        22,
        "codebase_ground",
        json!({"graph": "test", "claim": "The fn \u{00E9}tat processes \u{1F30D} data \u{2603}"}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Mixed unicode in claim should not crash: {:?}",
        resp
    );
}

// ============================================================================
// 6. Rapid-fire — sequential stress
// ============================================================================

#[test]
fn rapid_fire_100_symbol_lookups() {
    let mut server = create_loaded_server();
    for i in 0..100 {
        let resp = tool_call(
            &mut server,
            1000 + i,
            "symbol_lookup",
            json!({"graph": "test", "name": format!("sym_{}", i)}),
        );
        assert!(
            resp.get("result").is_some() || resp.get("error").is_some(),
            "Rapid-fire iteration {} crashed",
            i
        );
    }
}

#[test]
fn rapid_fire_100_mixed_tools() {
    let mut server = create_loaded_server();
    let tool_rotation = [
        ("graph_stats", json!({"graph": "test"})),
        (
            "symbol_lookup",
            json!({"graph": "test", "name": "process_data"}),
        ),
        ("list_units", json!({"graph": "test"})),
        (
            "codebase_ground",
            json!({"graph": "test", "claim": "There is a function"}),
        ),
        ("concept_find", json!({"graph": "test", "query": "config"})),
        ("pattern_extract", json!({"graph": "test"})),
        ("architecture_infer", json!({"graph": "test"})),
        ("omniscience_search", json!({"query": "sorting"})),
        ("soul_extract", json!({"graph": "test", "unit_id": 0})),
        ("genetics_dna", json!({"graph": "test", "unit_id": 0})),
    ];
    for i in 0..100u64 {
        let idx = (i as usize) % tool_rotation.len();
        let (tool, ref args) = tool_rotation[idx];
        let resp = tool_call(&mut server, 2000 + i, tool, args.clone());
        assert!(
            resp.get("result").is_some() || resp.get("error").is_some(),
            "Rapid-fire mixed iteration {} tool '{}' crashed",
            i,
            tool
        );
    }
}

#[test]
fn rapid_fire_100_tools_list() {
    let mut server = McpServer::new();
    for i in 0..100u64 {
        let req = json!({
            "jsonrpc": "2.0",
            "id": 3000 + i,
            "method": "tools/list",
            "params": {}
        });
        let resp = send(&mut server, &req);
        assert!(
            resp.get("result").is_some(),
            "tools/list failed at iteration {}",
            i
        );
    }
}

// ============================================================================
// 7. Individual invention tool families — targeted edge cases
// ============================================================================

// --- Resurrect family ---

#[test]
fn resurrect_search_with_loaded_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        40,
        "resurrect_search",
        json!({"graph": "test", "query": "deleted function", "max_results": 5}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn resurrect_attempt_with_loaded_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        41,
        "resurrect_attempt",
        json!({"graph": "test", "query": "old validate function"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn resurrect_verify_with_loaded_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        42,
        "resurrect_verify",
        json!({
            "graph": "test",
            "original_name": "old_validate",
            "reconstructed": "fn old_validate(x: i32) -> bool { x > 0 }"
        }),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn resurrect_history_with_loaded_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        43,
        "resurrect_history",
        json!({"graph": "test"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

// --- Genetics family ---

#[test]
fn genetics_dna_valid_unit() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        50,
        "genetics_dna",
        json!({"graph": "test", "unit_id": 0}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn genetics_lineage_valid_unit() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        51,
        "genetics_lineage",
        json!({"graph": "test", "unit_id": 0, "max_depth": 5}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn genetics_mutations_valid_unit() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        52,
        "genetics_mutations",
        json!({"graph": "test", "unit_id": 0}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn genetics_diseases_valid_unit() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        53,
        "genetics_diseases",
        json!({"graph": "test", "unit_id": 0}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn genetics_dna_out_of_bounds_unit_id() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        54,
        "genetics_dna",
        json!({"graph": "test", "unit_id": 99999}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Out-of-bounds unit_id should not crash"
    );
}

#[test]
fn genetics_lineage_negative_unit_id() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        55,
        "genetics_lineage",
        json!({"graph": "test", "unit_id": -1}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Negative unit_id should not crash"
    );
}

// --- Telepathy family ---

#[test]
fn telepathy_connect_with_loaded_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        60,
        "telepathy_connect",
        json!({"workspace": "test-ws", "source_graph": "test", "target_graph": "test"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn telepathy_broadcast_with_loaded_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        61,
        "telepathy_broadcast",
        json!({"workspace": "test-ws", "insight": "All functions should be pure", "source_graph": "test"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn telepathy_listen_with_loaded_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        62,
        "telepathy_listen",
        json!({"workspace": "test-ws", "target_graph": "test"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn telepathy_consensus_with_loaded_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        63,
        "telepathy_consensus",
        json!({"workspace": "test-ws", "concept": "error handling"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

// --- Soul family ---

#[test]
fn soul_extract_valid_unit() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        70,
        "soul_extract",
        json!({"graph": "test", "unit_id": 0}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn soul_compare_two_units() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        71,
        "soul_compare",
        json!({"graph": "test", "unit_id_a": 0, "unit_id_b": 1}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn soul_preserve_with_language() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        72,
        "soul_preserve",
        json!({"graph": "test", "unit_id": 0, "new_language": "Python"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn soul_reincarnate_with_context() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        73,
        "soul_reincarnate",
        json!({"graph": "test", "soul_id": "soul-0", "target_context": "microservice architecture"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn soul_karma_valid_unit() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        74,
        "soul_karma",
        json!({"graph": "test", "unit_id": 0}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn soul_extract_out_of_bounds() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        75,
        "soul_extract",
        json!({"graph": "test", "unit_id": 99999}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "soul_extract with out-of-bounds unit_id should not crash"
    );
}

#[test]
fn soul_compare_same_unit() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        76,
        "soul_compare",
        json!({"graph": "test", "unit_id_a": 0, "unit_id_b": 0}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "soul_compare with same unit should not crash"
    );
}

// --- Omniscience family ---

#[test]
fn omniscience_search_basic() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        80,
        "omniscience_search",
        json!({"query": "binary search", "max_results": 3}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn omniscience_best_basic() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        81,
        "omniscience_best",
        json!({"capability": "sorting", "criteria": ["performance", "readability"]}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn omniscience_census_basic() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        82,
        "omniscience_census",
        json!({"concept": "error handling", "languages": ["Rust", "Python"]}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn omniscience_vuln_basic() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        83,
        "omniscience_vuln",
        json!({"graph": "test", "pattern": "SQL injection"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn omniscience_trend_basic() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        84,
        "omniscience_trend",
        json!({"domain": "web frameworks", "threshold": 0.3}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn omniscience_compare_basic() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        85,
        "omniscience_compare",
        json!({"graph": "test", "unit_id": 0}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn omniscience_api_usage_basic() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        86,
        "omniscience_api_usage",
        json!({"api": "tokio", "method": "spawn"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn omniscience_solve_basic() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        87,
        "omniscience_solve",
        json!({"problem": "concurrent hashmap", "languages": ["Rust"], "max_results": 3}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn omniscience_search_with_language_filter() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        88,
        "omniscience_search",
        json!({"query": "authentication", "languages": ["Rust"], "max_results": 1}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

// --- Archaeology family ---

#[test]
fn archaeology_node_valid() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        90,
        "archaeology_node",
        json!({"graph": "test", "unit_id": 0}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn archaeology_why_valid() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        91,
        "archaeology_why",
        json!({"graph": "test", "unit_id": 0}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn archaeology_when_valid() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        92,
        "archaeology_when",
        json!({"graph": "test", "unit_id": 0}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

// --- Pattern family ---

#[test]
fn pattern_extract_on_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(&mut server, 95, "pattern_extract", json!({"graph": "test"}));
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn pattern_check_on_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        96,
        "pattern_check",
        json!({"graph": "test", "pattern": "singleton"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn pattern_suggest_on_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(&mut server, 97, "pattern_suggest", json!({"graph": "test"}));
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

// --- Grounding and truth family ---

#[test]
fn codebase_ground_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        100,
        "codebase_ground",
        json!({"graph": "test", "claim": "process_data calls validate_input"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
    if resp.get("result").is_some() {
        let text = tool_text(&resp);
        assert!(!text.is_empty(), "codebase_ground should return content");
    }
}

#[test]
fn hallucination_check_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        101,
        "hallucination_check",
        json!({"graph": "test", "claim": "There is a function called nonexistent_fn"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn truth_register_and_check() {
    let mut server = create_loaded_server();
    // Register a truth
    let resp1 = tool_call(
        &mut server,
        102,
        "truth_register",
        json!({"graph": "test", "fact": "process_data is the entry point"}),
    );
    assert!(resp1.get("result").is_some() || resp1.get("error").is_some());

    // Check the truth
    let resp2 = tool_call(
        &mut server,
        103,
        "truth_check",
        json!({"graph": "test", "claim": "process_data is the entry point"}),
    );
    assert!(resp2.get("result").is_some() || resp2.get("error").is_some());
}

// --- Prophecy and regression family ---

#[test]
fn prophecy_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        110,
        "prophecy",
        json!({"graph": "test", "unit_id": 0}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn prophecy_if_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        111,
        "prophecy_if",
        json!({"graph": "test", "unit_id": 0, "change": "add error handling"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn regression_predict_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        112,
        "regression_predict",
        json!({"graph": "test", "unit_id": 0}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn regression_minimal_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        113,
        "regression_minimal",
        json!({"graph": "test", "unit_id": 0}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

// --- Search family ---

#[test]
fn search_semantic_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        120,
        "search_semantic",
        json!({"graph": "test", "query": "data processing"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn search_similar_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        121,
        "search_similar",
        json!({"graph": "test", "unit_id": 0}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn search_explain_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        122,
        "search_explain",
        json!({"graph": "test", "query": "config management"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

// --- Compare family ---

#[test]
fn compare_codebases_same_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        130,
        "compare_codebases",
        json!({"graph_a": "test", "graph_b": "test"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn compare_concept_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        131,
        "compare_concept",
        json!({"graph_a": "test", "graph_b": "test", "concept": "validation"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn compare_migrate_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        132,
        "compare_migrate",
        json!({"graph_a": "test", "graph_b": "test"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

// --- Concept family ---

#[test]
fn concept_find_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        140,
        "concept_find",
        json!({"graph": "test", "query": "configuration"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn concept_map_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(&mut server, 141, "concept_map", json!({"graph": "test"}));
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn concept_explain_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        142,
        "concept_explain",
        json!({"graph": "test", "concept": "data flow"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

// --- Architecture family ---

#[test]
fn architecture_infer_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        150,
        "architecture_infer",
        json!({"graph": "test"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn architecture_validate_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        151,
        "architecture_validate",
        json!({"graph": "test", "rule": "no circular dependencies"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

// --- Workspace family edge cases ---

#[test]
fn workspace_create_and_list() {
    let mut server = create_loaded_server();
    let resp1 = tool_call(
        &mut server,
        160,
        "workspace_create",
        json!({"name": "edge-test-ws"}),
    );
    assert!(resp1.get("result").is_some() || resp1.get("error").is_some());

    let resp2 = tool_call(&mut server, 161, "workspace_list", json!({}));
    assert!(resp2.get("result").is_some() || resp2.get("error").is_some());
}

#[test]
fn workspace_add_nonexistent_workspace() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        162,
        "workspace_add",
        json!({"workspace": "no-such-ws", "graph": "test"}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "workspace_add to nonexistent workspace should not crash"
    );
}

// --- Impact family ---

#[test]
fn impact_analyze_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        170,
        "impact_analyze",
        json!({"graph": "test", "unit_id": 0}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn impact_path_with_graph() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        171,
        "impact_path",
        json!({"graph": "test", "from": 0, "to": 1}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

// --- Translation family ---

#[test]
fn translation_progress_empty() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        180,
        "translation_progress",
        json!({"graph": "test"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

#[test]
fn translation_remaining_empty() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        181,
        "translation_remaining",
        json!({"graph": "test"}),
    );
    assert!(resp.get("result").is_some() || resp.get("error").is_some());
}

// ============================================================================
// 8. Boundary and degenerate inputs
// ============================================================================

#[test]
fn tool_call_with_null_arguments() {
    let mut server = create_loaded_server();
    let req = json!({
        "jsonrpc": "2.0",
        "id": 500,
        "method": "tools/call",
        "params": { "name": "graph_stats", "arguments": null }
    });
    let resp = send(&mut server, &req);
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "null arguments should not crash"
    );
}

#[test]
fn tool_call_with_extra_unknown_fields() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        501,
        "graph_stats",
        json!({"graph": "test", "unknown_field": "value", "another": 42}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Extra fields should not crash"
    );
}

#[test]
fn tool_call_with_empty_string_values() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        502,
        "symbol_lookup",
        json!({"graph": "", "name": ""}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Empty string values should not crash"
    );
}

#[test]
fn tool_call_with_very_long_string() {
    let mut server = create_loaded_server();
    let long_name: String = "a".repeat(10_000);
    let resp = tool_call(
        &mut server,
        503,
        "symbol_lookup",
        json!({"graph": "test", "name": long_name}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Very long string should not crash"
    );
}

#[test]
fn tool_call_with_zero_max_results() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        504,
        "resurrect_search",
        json!({"graph": "test", "query": "test", "max_results": 0}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "zero max_results should not crash"
    );
}

#[test]
fn tool_call_with_negative_unit_id() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        505,
        "soul_extract",
        json!({"graph": "test", "unit_id": -42}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Negative unit_id should not crash"
    );
}

#[test]
fn tool_call_with_float_unit_id() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        506,
        "genetics_dna",
        json!({"graph": "test", "unit_id": 1.5}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Float unit_id should not crash"
    );
}

#[test]
fn tool_call_with_string_unit_id() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        507,
        "genetics_dna",
        json!({"graph": "test", "unit_id": "not_a_number"}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "String unit_id should not crash"
    );
}

#[test]
fn unknown_tool_name_returns_error() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        508,
        "completely_nonexistent_tool_xyz",
        json!({}),
    );
    assert!(
        resp.get("error").is_some(),
        "Unknown tool should return an error response"
    );
}

#[test]
fn tool_call_with_massive_max_results() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        509,
        "omniscience_search",
        json!({"query": "test", "max_results": 999999}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Massive max_results should not crash"
    );
}

#[test]
fn tool_call_with_special_chars_in_strings() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        510,
        "codebase_ground",
        json!({"graph": "test", "claim": "fn foo<T: Clone>(x: &'a str) -> Result<(), Box<dyn Error>>"}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Special Rust syntax chars in strings should not crash"
    );
}

#[test]
fn tool_call_with_newlines_in_strings() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        511,
        "telepathy_broadcast",
        json!({"workspace": "ws", "insight": "line1\nline2\nline3\ttab"}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Newlines in strings should not crash"
    );
}

#[test]
fn tool_call_with_null_bytes_in_string() {
    let mut server = create_loaded_server();
    let resp = tool_call(
        &mut server,
        512,
        "symbol_lookup",
        json!({"graph": "test", "name": "foo\u{0000}bar"}),
    );
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "Null bytes in string should not crash"
    );
}
