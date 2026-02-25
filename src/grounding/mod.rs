//! Grounding — Anti-hallucination layer for code claims.
//!
//! Verifies that claims about code are backed by graph evidence.
//! An agent cannot assert that code exists without graph confirmation.
//!
//! # Overview
//!
//! The grounding system extracts code references from natural-language claims
//! and checks each one against the [`CodeGraph`]. Claims that reference
//! symbols, functions, types, or files absent from the graph are flagged as
//! ungrounded — potential hallucinations.
//!
//! # Usage
//!
//! ```ignore
//! use agentic_codebase::grounding::{GroundingEngine, Grounded};
//!
//! let engine = GroundingEngine::new(&graph);
//! let result = engine.ground_claim("The function process_payment validates the amount");
//! ```
//!
//! [`CodeGraph`]: crate::graph::CodeGraph

mod engine;

pub use engine::{extract_code_references, GroundingEngine};

/// Result of grounding a claim against the code graph.
#[derive(Debug, Clone)]
pub enum GroundingResult {
    /// Claim fully supported by graph data.
    Verified {
        /// Evidence nodes backing each reference in the claim.
        evidence: Vec<Evidence>,
        /// Confidence score in [0.0, 1.0]. Higher means stronger backing.
        confidence: f32,
    },
    /// Claim partially supported — some references found, others not.
    Partial {
        /// References that matched graph nodes.
        supported: Vec<String>,
        /// References with no graph backing.
        unsupported: Vec<String>,
        /// Possible corrections for the unsupported references.
        suggestions: Vec<String>,
    },
    /// No graph backing — potential hallucination.
    Ungrounded {
        /// The original claim text.
        claim: String,
        /// Similar names from the graph that the claim may have intended.
        suggestions: Vec<String>,
    },
}

/// Evidence backing a code claim.
///
/// Each piece of evidence links to a specific node in the [`CodeGraph`]
/// that supports one or more references in the claim.
///
/// [`CodeGraph`]: crate::graph::CodeGraph
#[derive(Debug, Clone)]
pub struct Evidence {
    /// The code-unit ID in the graph.
    pub node_id: u64,
    /// Human-readable type label (e.g. "function", "type", "module").
    pub node_type: String,
    /// Simple name of the code unit.
    pub name: String,
    /// File path where the code unit is defined.
    pub file_path: String,
    /// Starting line number (if available).
    pub line_number: Option<u32>,
    /// A short code snippet or signature (if available).
    pub snippet: Option<String>,
}

/// Trait for grounding code claims against a knowledge source.
///
/// Implementors hold a reference to some code knowledge base (typically a
/// [`CodeGraph`]) and can verify whether natural-language claims about code
/// are backed by real data.
///
/// [`CodeGraph`]: crate::graph::CodeGraph
pub trait Grounded {
    /// Verify a natural-language claim about code.
    ///
    /// Extracts code references from `claim`, checks each against the
    /// backing graph, and returns a [`GroundingResult`] indicating full,
    /// partial, or no support.
    fn ground_claim(&self, claim: &str) -> GroundingResult;

    /// Find all evidence nodes matching `name`.
    ///
    /// Searches by exact name first, then by qualified-name substring.
    fn find_evidence(&self, name: &str) -> Vec<Evidence>;

    /// Suggest graph names similar to `name` (for typo correction).
    ///
    /// Returns up to `limit` suggestions sorted by edit distance.
    fn suggest_similar(&self, name: &str, limit: usize) -> Vec<String>;
}
