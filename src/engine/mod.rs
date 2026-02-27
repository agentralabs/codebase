//! High-level compilation pipeline and query engine.
//!
//! Orchestrates parsing, semantic analysis, and graph building.
//! Orchestrates query execution across indexes.

pub mod compile;
pub mod impact;
pub mod incremental;
pub mod query;
pub mod regression;

pub use compile::{CompileOptions, CompilePipeline, CompileResult, CompileStats};
pub use impact::{
    BlastRadius, ChangeType, EnhancedImpactResult, ImpactAnalyzer, ImpactType, ImpactedNode,
    Mitigation, ProposedChange, RiskLevel,
};
pub use incremental::{ChangeSet, IncrementalCompiler, IncrementalResult};
pub use query::QueryEngine;
pub use regression::{RegressionPredictor, TestId, TestPrediction};
