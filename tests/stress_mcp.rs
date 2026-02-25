//! MCP tool stress tests — tests all 12 new MCP tools via JSON-RPC.

use std::path::PathBuf;

use serde_json::{json, Value};

use agentic_codebase::graph::CodeGraph;
use agentic_codebase::mcp::server::McpServer;
use agentic_codebase::types::{CodeUnit, CodeUnitType, Language, Span};

// ─────────────────────── helpers ───────────────────────

fn build_test_graph() -> CodeGraph {
    let mut graph = CodeGraph::with_default_dimension();
    let symbols = vec![
        ("process_payment", CodeUnitType::Function, "payments.stripe.process_payment", "src/payments/stripe.py"),
        ("CodeGraph", CodeUnitType::Type, "crate::graph::CodeGraph", "src/graph/code_graph.rs"),
        ("validate_input", CodeUnitType::Function, "crate::validate_input", "src/validation.rs"),
        ("MAX_RETRIES", CodeUnitType::Config, "crate::MAX_RETRIES", "src/config.rs"),
        ("UserProfile", CodeUnitType::Type, "crate::models::UserProfile", "src/models.rs"),
    ];
    for (name, utype, qname, fpath) in symbols {
        let mut unit = CodeUnit::new(
            utype,
            Language::Rust,
            name.to_string(),
            qname.to_string(),
            PathBuf::from(fpath),
            Span::new(1, 0, 50, 0),
        );
        unit.signature = Some(format!("fn {}()", name));
        graph.add_unit(unit);
    }
    graph
}

fn create_server() -> McpServer {
    let mut server = McpServer::new();
    server.load_graph("test".to_string(), build_test_graph());
    init(&mut server);
    server
}

fn init(server: &mut McpServer) {
    call(server, &json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}));
}

fn call(server: &mut McpServer, request: &Value) -> Value {
    let raw = serde_json::to_string(request).unwrap();
    let response_str = server.handle_raw(&raw);
    serde_json::from_str(&response_str).unwrap()
}

fn tool_call(server: &mut McpServer, tool: &str, args: Value) -> Value {
    call(
        server,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": tool,
                "arguments": args
            }
        }),
    )
}

fn tool_text(resp: &Value) -> String {
    resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

// ============================================================================
// Grounding MCP tools
// ============================================================================

#[test]
fn test_mcp_codebase_ground_verified() {
    let mut server = create_server();
    let resp = tool_call(
        &mut server,
        "codebase_ground",
        json!({"claim": "The process_payment function exists", "graph": "test"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["status"], "verified");
    assert!(parsed["evidence"].as_array().unwrap().len() > 0);
}

#[test]
fn test_mcp_codebase_ground_ungrounded() {
    let mut server = create_server();
    let resp = tool_call(
        &mut server,
        "codebase_ground",
        json!({"claim": "The deploy_server function deploys", "graph": "test"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["status"], "ungrounded");
}

#[test]
fn test_mcp_codebase_ground_partial() {
    let mut server = create_server();
    let resp = tool_call(
        &mut server,
        "codebase_ground",
        json!({"claim": "process_payment calls send_email after success", "graph": "test"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["status"], "partial");
}

#[test]
fn test_mcp_codebase_ground_strict() {
    let mut server = create_server();
    let resp = tool_call(
        &mut server,
        "codebase_ground",
        json!({"claim": "process_payment calls send_email", "graph": "test", "strict": true}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    // In strict mode, partial → ungrounded.
    assert_eq!(parsed["status"], "ungrounded");
}

#[test]
fn test_mcp_codebase_ground_missing_claim() {
    let mut server = create_server();
    let resp = tool_call(
        &mut server,
        "codebase_ground",
        json!({"graph": "test"}),
    );
    assert!(resp["error"].is_object(), "Should error without claim");
}

#[test]
fn test_mcp_codebase_evidence() {
    let mut server = create_server();
    let resp = tool_call(
        &mut server,
        "codebase_evidence",
        json!({"name": "process_payment", "graph": "test"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let evidence = parsed.as_array().unwrap();
    assert!(!evidence.is_empty());
    assert_eq!(evidence[0]["name"], "process_payment");
}

#[test]
fn test_mcp_codebase_evidence_with_type_filter() {
    let mut server = create_server();
    let resp = tool_call(
        &mut server,
        "codebase_evidence",
        json!({"name": "CodeGraph", "graph": "test", "types": ["type"]}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let evidence = parsed.as_array().unwrap();
    assert!(!evidence.is_empty());
    assert_eq!(evidence[0]["node_type"], "type");
}

#[test]
fn test_mcp_codebase_evidence_no_match() {
    let mut server = create_server();
    let resp = tool_call(
        &mut server,
        "codebase_evidence",
        json!({"name": "nonexistent_function", "graph": "test"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed.as_array().unwrap().is_empty());
}

#[test]
fn test_mcp_codebase_suggest() {
    let mut server = create_server();
    let resp = tool_call(
        &mut server,
        "codebase_suggest",
        json!({"name": "process_paymnt", "graph": "test", "limit": 3}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let suggestions = parsed["suggestions"].as_array().unwrap();
    assert!(
        suggestions.iter().any(|s| s.as_str() == Some("process_payment")),
        "Should suggest process_payment for typo, got {:?}",
        suggestions
    );
}

#[test]
fn test_mcp_codebase_suggest_limit() {
    let mut server = create_server();
    let resp = tool_call(
        &mut server,
        "codebase_suggest",
        json!({"name": "a", "graph": "test", "limit": 2}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let suggestions = parsed["suggestions"].as_array().unwrap();
    assert!(suggestions.len() <= 2);
}

// ============================================================================
// Workspace MCP tools
// ============================================================================

#[test]
fn test_mcp_workspace_create() {
    let mut server = create_server();
    let resp = tool_call(
        &mut server,
        "workspace_create",
        json!({"name": "test-migration"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed["workspace_id"].as_str().unwrap().starts_with("ws-"));
    assert_eq!(parsed["name"], "test-migration");
}

#[test]
fn test_mcp_workspace_create_missing_name() {
    let mut server = create_server();
    let resp = tool_call(
        &mut server,
        "workspace_create",
        json!({}),
    );
    assert!(resp["error"].is_object());
}

#[test]
fn test_mcp_workspace_add() {
    let mut server = create_server();

    // Create workspace first.
    let create_resp = tool_call(&mut server, "workspace_create", json!({"name": "ws"}));
    let ws_id: String = {
        let text = tool_text(&create_resp);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        parsed["workspace_id"].as_str().unwrap().to_string()
    };

    // Add graph to workspace.
    let resp = tool_call(
        &mut server,
        "workspace_add",
        json!({"workspace": ws_id, "graph": "test", "role": "source", "path": "/src/main"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed["context_id"].as_str().unwrap().starts_with("ctx-"));
}

#[test]
fn test_mcp_workspace_add_invalid_graph() {
    let mut server = create_server();
    let create_resp = tool_call(&mut server, "workspace_create", json!({"name": "ws"}));
    let ws_id: String = {
        let text = tool_text(&create_resp);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        parsed["workspace_id"].as_str().unwrap().to_string()
    };

    let resp = tool_call(
        &mut server,
        "workspace_add",
        json!({"workspace": ws_id, "graph": "nonexistent", "role": "source"}),
    );
    assert!(resp["error"].is_object());
}

#[test]
fn test_mcp_workspace_add_invalid_role() {
    let mut server = create_server();
    let create_resp = tool_call(&mut server, "workspace_create", json!({"name": "ws"}));
    let ws_id: String = {
        let text = tool_text(&create_resp);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        parsed["workspace_id"].as_str().unwrap().to_string()
    };

    let resp = tool_call(
        &mut server,
        "workspace_add",
        json!({"workspace": ws_id, "graph": "test", "role": "invalid_role"}),
    );
    assert!(resp["error"].is_object());
}

#[test]
fn test_mcp_workspace_list() {
    let mut server = create_server();

    // Create and populate workspace.
    let create_resp = tool_call(&mut server, "workspace_create", json!({"name": "ws"}));
    let ws_id: String = {
        let text = tool_text(&create_resp);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        parsed["workspace_id"].as_str().unwrap().to_string()
    };

    tool_call(&mut server, "workspace_add", json!({"workspace": &ws_id, "graph": "test", "role": "source"}));

    let resp = tool_call(&mut server, "workspace_list", json!({"workspace": &ws_id}));
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["context_count"], 1);
    assert_eq!(parsed["contexts"][0]["role"], "source");
}

#[test]
fn test_mcp_workspace_query() {
    let mut server = create_server();
    let create_resp = tool_call(&mut server, "workspace_create", json!({"name": "ws"}));
    let ws_id: String = {
        let text = tool_text(&create_resp);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        parsed["workspace_id"].as_str().unwrap().to_string()
    };
    tool_call(&mut server, "workspace_add", json!({"workspace": &ws_id, "graph": "test", "role": "source"}));

    let resp = tool_call(
        &mut server,
        "workspace_query",
        json!({"workspace": &ws_id, "query": "process"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let results = parsed.as_array().unwrap();
    assert!(!results.is_empty());
    assert!(results[0]["matches"].as_array().unwrap().len() > 0);
}

#[test]
fn test_mcp_workspace_compare() {
    let mut server = create_server();
    // Load a second graph for comparison.
    let mut g2 = CodeGraph::with_default_dimension();
    let mut unit = CodeUnit::new(
        CodeUnitType::Function,
        Language::Rust,
        "process_payment".to_string(),
        "crate::process_payment".to_string(),
        PathBuf::from("src/payment.rs"),
        Span::new(1, 0, 20, 0),
    );
    unit.signature = Some("fn process_payment(amount: Decimal) -> Result<()>".to_string());
    g2.add_unit(unit);
    server.load_graph("target".to_string(), g2);

    let create_resp = tool_call(&mut server, "workspace_create", json!({"name": "cmp"}));
    let ws_id: String = {
        let text = tool_text(&create_resp);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        parsed["workspace_id"].as_str().unwrap().to_string()
    };
    tool_call(&mut server, "workspace_add", json!({"workspace": &ws_id, "graph": "test", "role": "source"}));
    tool_call(&mut server, "workspace_add", json!({"workspace": &ws_id, "graph": "target", "role": "target"}));

    let resp = tool_call(
        &mut server,
        "workspace_compare",
        json!({"workspace": &ws_id, "symbol": "process_payment"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["symbol"], "process_payment");
    assert_eq!(parsed["contexts"].as_array().unwrap().len(), 2);
}

#[test]
fn test_mcp_workspace_xref() {
    let mut server = create_server();
    let mut g2 = CodeGraph::with_default_dimension();
    g2.add_unit(CodeUnit::new(
        CodeUnitType::Function,
        Language::Rust,
        "other_fn".to_string(),
        "crate::other_fn".to_string(),
        PathBuf::from("src/other.rs"),
        Span::new(1, 0, 10, 0),
    ));
    server.load_graph("target".to_string(), g2);

    let create_resp = tool_call(&mut server, "workspace_create", json!({"name": "xref"}));
    let ws_id: String = {
        let text = tool_text(&create_resp);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        parsed["workspace_id"].as_str().unwrap().to_string()
    };
    tool_call(&mut server, "workspace_add", json!({"workspace": &ws_id, "graph": "test", "role": "source"}));
    tool_call(&mut server, "workspace_add", json!({"workspace": &ws_id, "graph": "target", "role": "target"}));

    let resp = tool_call(
        &mut server,
        "workspace_xref",
        json!({"workspace": &ws_id, "symbol": "process_payment"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["found_in"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["missing_from"].as_array().unwrap().len(), 1);
}

// ============================================================================
// Translation MCP tools
// ============================================================================

#[test]
fn test_mcp_translation_record() {
    let mut server = create_server();
    let create_resp = tool_call(&mut server, "workspace_create", json!({"name": "tr"}));
    let ws_id: String = {
        let text = tool_text(&create_resp);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        parsed["workspace_id"].as_str().unwrap().to_string()
    };

    let resp = tool_call(
        &mut server,
        "translation_record",
        json!({
            "workspace": &ws_id,
            "source_symbol": "parse_config",
            "target_symbol": "parse_config",
            "status": "ported",
            "notes": "Direct port"
        }),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["source_symbol"], "parse_config");
    assert_eq!(parsed["status"], "ported");
}

#[test]
fn test_mcp_translation_record_invalid_status() {
    let mut server = create_server();
    let create_resp = tool_call(&mut server, "workspace_create", json!({"name": "tr"}));
    let ws_id: String = {
        let text = tool_text(&create_resp);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        parsed["workspace_id"].as_str().unwrap().to_string()
    };

    let resp = tool_call(
        &mut server,
        "translation_record",
        json!({"workspace": &ws_id, "source_symbol": "foo", "status": "bogus"}),
    );
    assert!(resp["error"].is_object());
}

#[test]
fn test_mcp_translation_progress() {
    let mut server = create_server();
    let create_resp = tool_call(&mut server, "workspace_create", json!({"name": "tr"}));
    let ws_id: String = {
        let text = tool_text(&create_resp);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        parsed["workspace_id"].as_str().unwrap().to_string()
    };

    // Record some translations.
    tool_call(&mut server, "translation_record", json!({"workspace": &ws_id, "source_symbol": "a", "status": "ported"}));
    tool_call(&mut server, "translation_record", json!({"workspace": &ws_id, "source_symbol": "b", "status": "not_started"}));
    tool_call(&mut server, "translation_record", json!({"workspace": &ws_id, "source_symbol": "c", "status": "verified"}));

    let resp = tool_call(&mut server, "translation_progress", json!({"workspace": &ws_id}));
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["total"], 3);
    assert_eq!(parsed["ported"], 1);
    assert_eq!(parsed["not_started"], 1);
    assert_eq!(parsed["verified"], 1);
}

#[test]
fn test_mcp_translation_progress_empty() {
    let mut server = create_server();
    let create_resp = tool_call(&mut server, "workspace_create", json!({"name": "tr"}));
    let ws_id: String = {
        let text = tool_text(&create_resp);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        parsed["workspace_id"].as_str().unwrap().to_string()
    };

    let resp = tool_call(&mut server, "translation_progress", json!({"workspace": &ws_id}));
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["total"], 0);
}

#[test]
fn test_mcp_translation_remaining() {
    let mut server = create_server();
    let create_resp = tool_call(&mut server, "workspace_create", json!({"name": "tr"}));
    let ws_id: String = {
        let text = tool_text(&create_resp);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        parsed["workspace_id"].as_str().unwrap().to_string()
    };

    tool_call(&mut server, "translation_record", json!({"workspace": &ws_id, "source_symbol": "done_fn", "status": "ported"}));
    tool_call(&mut server, "translation_record", json!({"workspace": &ws_id, "source_symbol": "todo_fn", "status": "not_started"}));
    tool_call(&mut server, "translation_record", json!({"workspace": &ws_id, "source_symbol": "wip_fn", "status": "in_progress"}));

    let resp = tool_call(&mut server, "translation_remaining", json!({"workspace": &ws_id}));
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["remaining_count"], 2);
    let remaining = parsed["remaining"].as_array().unwrap();
    let names: Vec<&str> = remaining.iter().map(|r| r["source_symbol"].as_str().unwrap()).collect();
    assert!(names.contains(&"todo_fn"));
    assert!(names.contains(&"wip_fn"));
}

// ============================================================================
// Unknown tool error
// ============================================================================

#[test]
fn test_mcp_unknown_tool_error() {
    let mut server = create_server();
    let resp = tool_call(&mut server, "nonexistent_tool", json!({}));
    assert!(resp["error"].is_object());
}
