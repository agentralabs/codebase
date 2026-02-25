//! AgenticCodebase — Semantic code compiler for AI agents.
//!
//! Transforms source code into a navigable graph of concepts, relationships,
//! and patterns. Stores the entire semantic structure of a codebase in a
//! single memory-mappable `.acb` file.
//!
//! # Architecture
//!
//! - **types** — All data types. No logic. No I/O.
//! - **parse** — Language-specific parsing via tree-sitter.
//! - **semantic** — Cross-file resolution, FFI tracing, pattern detection.
//! - **format** — Binary `.acb` file reader/writer.
//! - **graph** — In-memory graph operations.
//! - **engine** — Compilation pipeline and query executor.
//! - **index** — Fast lookup indexes.
//! - **temporal** — Change history, stability, coupling, prophecy.
//! - **collective** — Collective intelligence and pattern sync.
//! - **grounding** — Anti-hallucination verification of code claims.
//! - **workspace** — Multi-context workspaces for cross-codebase queries.
//! - **ffi** — C-compatible FFI bindings.
//! - **config** — Configuration loading and path resolution.
//! - **cli** — Command-line interface.
//! - **mcp** — Model Context Protocol server interface.

pub mod cli;
pub mod collective;
pub mod config;
pub mod engine;
pub mod ffi;
pub mod format;
pub mod graph;
pub mod grounding;
pub mod index;
pub mod mcp;
pub mod parse;
pub mod semantic;
pub mod temporal;
pub mod types;
pub mod workspace;

// Re-export key types at crate root for convenience
pub use format::{AcbReader, AcbWriter};
pub use graph::{CodeGraph, GraphBuilder};
pub use grounding::{Evidence, Grounded, GroundingEngine, GroundingResult};
pub use types::{
    AcbError, AcbResult, CodeUnit, CodeUnitBuilder, CodeUnitType, Edge, EdgeType, FileHeader,
    Language, Span, Visibility,
};
