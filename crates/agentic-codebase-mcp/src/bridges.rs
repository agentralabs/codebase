//! Sister integration bridge traits for AgenticCodebase.
//!
//! Each bridge defines the interface for integrating with another Agentra sister.
//! Default implementations are no-ops, allowing gradual adoption.
//! Trait-based design ensures Hydra compatibility — swap implementors without refactoring.
//!
//! Note: Codebase has no core library crate, so bridges live in the MCP crate.

/// Bridge to agentic-memory for persisting code analysis in memory.
pub trait MemoryBridge: Send + Sync {
    /// Store a code analysis result as a memory node
    fn store_analysis(&self, analysis_type: &str, details: &str) -> Result<u64, String> {
        let _ = (analysis_type, details);
        Err("Memory bridge not connected".to_string())
    }

    /// Recall past code analyses from memory
    fn recall_analyses(&self, topic: &str, max_results: usize) -> Vec<String> {
        let _ = (topic, max_results);
        Vec::new()
    }

    /// Link a code unit to a memory decision node
    fn link_unit_to_memory(&self, unit_id: u64, node_id: u64) -> Result<(), String> {
        let _ = (unit_id, node_id);
        Err("Memory bridge not connected".to_string())
    }
}

/// Bridge to agentic-identity for code attribution and signing.
pub trait IdentityBridge: Send + Sync {
    /// Attribute a code change to an agent identity
    fn attribute_change(&self, unit_id: u64, agent_id: &str) -> Result<(), String> {
        let _ = (unit_id, agent_id);
        Err("Identity bridge not connected".to_string())
    }

    /// Verify the author of a code unit
    fn verify_author(&self, unit_id: u64, claimed_author: &str) -> bool {
        let _ = (unit_id, claimed_author);
        true // Default: trust all
    }

    /// Sign a code review action
    fn sign_review(&self, unit_id: u64, verdict: &str) -> Result<String, String> {
        let _ = (unit_id, verdict);
        Err("Identity bridge not connected".to_string())
    }
}

/// Bridge to agentic-time for temporal code context.
pub trait TimeBridge: Send + Sync {
    /// Get when a code unit was last modified
    fn last_modified(&self, unit_id: u64) -> Option<u64> {
        let _ = unit_id;
        None
    }

    /// Schedule a code review at a future time
    fn schedule_review(&self, unit_id: u64, review_at: u64) -> Result<String, String> {
        let _ = (unit_id, review_at);
        Err("Time bridge not connected".to_string())
    }

    /// Create a deadline for a code change
    fn create_code_deadline(&self, label: &str, due_at: u64) -> Result<String, String> {
        let _ = (label, due_at);
        Err("Time bridge not connected".to_string())
    }
}

/// Bridge to agentic-contract for policy-governed code operations.
pub trait ContractBridge: Send + Sync {
    /// Check if a code operation is allowed by policies
    fn check_policy(&self, operation: &str, unit_id: u64) -> Result<bool, String> {
        let _ = (operation, unit_id);
        Ok(true) // Default: allow all
    }

    /// Record a code operation for audit trail
    fn record_operation(&self, operation: &str, unit_id: u64) -> Result<(), String> {
        let _ = (operation, unit_id);
        Err("Contract bridge not connected".to_string())
    }

    /// Get risk assessment for a code change
    fn risk_assessment(&self, unit_id: u64) -> Option<f64> {
        let _ = unit_id;
        None
    }
}

/// Bridge to agentic-vision for code-visual bindings.
pub trait VisionBridge: Send + Sync {
    /// Link a code unit to a visual capture
    fn link_to_capture(&self, unit_id: u64, capture_id: u64, binding_type: &str) -> Result<(), String> {
        let _ = (unit_id, capture_id, binding_type);
        Err("Vision bridge not connected".to_string())
    }

    /// Find visual captures related to a code component
    fn find_visual_for_code(&self, symbol: &str) -> Vec<u64> {
        let _ = symbol;
        Vec::new()
    }
}

/// Bridge to agentic-comm for code-aware messaging.
pub trait CommBridge: Send + Sync {
    /// Broadcast a code change notification
    fn broadcast_change(&self, unit_id: u64, change_type: &str, channel_id: u64) -> Result<(), String> {
        let _ = (unit_id, change_type, channel_id);
        Err("Comm bridge not connected".to_string())
    }

    /// Notify of a regression prediction
    fn notify_regression(&self, unit_id: u64, affected_tests: &[String]) -> Result<(), String> {
        let _ = (unit_id, affected_tests);
        Err("Comm bridge not connected".to_string())
    }
}

/// No-op implementation of all bridges for standalone use.
#[derive(Debug, Clone, Default)]
pub struct NoOpBridges;

impl MemoryBridge for NoOpBridges {}
impl IdentityBridge for NoOpBridges {}
impl TimeBridge for NoOpBridges {}
impl ContractBridge for NoOpBridges {}
impl VisionBridge for NoOpBridges {}
impl CommBridge for NoOpBridges {}

/// Configuration for which bridges are active.
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub memory_enabled: bool,
    pub identity_enabled: bool,
    pub time_enabled: bool,
    pub contract_enabled: bool,
    pub vision_enabled: bool,
    pub comm_enabled: bool,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            memory_enabled: false,
            identity_enabled: false,
            time_enabled: false,
            contract_enabled: false,
            vision_enabled: false,
            comm_enabled: false,
        }
    }
}

/// Hydra adapter trait — future orchestrator discovery interface.
pub trait HydraAdapter: Send + Sync {
    fn adapter_id(&self) -> &str;
    fn capabilities(&self) -> Vec<String>;
    fn handle_request(&self, method: &str, params: &str) -> Result<String, String>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_bridges_implements_all_traits() {
        let b = NoOpBridges;
        let _: &dyn MemoryBridge = &b;
        let _: &dyn IdentityBridge = &b;
        let _: &dyn TimeBridge = &b;
        let _: &dyn ContractBridge = &b;
        let _: &dyn VisionBridge = &b;
        let _: &dyn CommBridge = &b;
    }

    #[test]
    fn memory_bridge_defaults() {
        let b = NoOpBridges;
        assert!(b.store_analysis("impact", "details").is_err());
        assert!(b.recall_analyses("topic", 10).is_empty());
        assert!(b.link_unit_to_memory(1, 2).is_err());
    }

    #[test]
    fn identity_bridge_defaults() {
        let b = NoOpBridges;
        assert!(b.attribute_change(1, "agent-1").is_err());
        assert!(b.verify_author(1, "agent-1"));
        assert!(b.sign_review(1, "approved").is_err());
    }

    #[test]
    fn time_bridge_defaults() {
        let b = NoOpBridges;
        assert!(b.last_modified(1).is_none());
        assert!(b.schedule_review(1, 1000).is_err());
        assert!(b.create_code_deadline("label", 1000).is_err());
    }

    #[test]
    fn contract_bridge_defaults() {
        let b = NoOpBridges;
        assert!(b.check_policy("analyze", 1).unwrap());
        assert!(b.record_operation("analyze", 1).is_err());
        assert!(b.risk_assessment(1).is_none());
    }

    #[test]
    fn vision_bridge_defaults() {
        let b = NoOpBridges;
        assert!(b.link_to_capture(1, 2, "rendered_by").is_err());
        assert!(b.find_visual_for_code("Button").is_empty());
    }

    #[test]
    fn comm_bridge_defaults() {
        let b = NoOpBridges;
        assert!(b.broadcast_change(1, "behavior", 1).is_err());
        assert!(b.notify_regression(1, &["test_1".to_string()]).is_err());
    }

    #[test]
    fn bridge_config_defaults_all_false() {
        let cfg = BridgeConfig::default();
        assert!(!cfg.memory_enabled);
        assert!(!cfg.identity_enabled);
        assert!(!cfg.time_enabled);
        assert!(!cfg.contract_enabled);
        assert!(!cfg.vision_enabled);
        assert!(!cfg.comm_enabled);
    }

    #[test]
    fn noop_bridges_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NoOpBridges>();
    }

    #[test]
    fn noop_bridges_default_and_clone() {
        let b = NoOpBridges::default();
        let _b2 = b.clone();
    }
}
