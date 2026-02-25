//! Multi-context workspaces for loading and querying multiple codebases.
//!
//! Enables migration workflows (e.g., C++ to Rust) by loading source and target
//! codebases into a single workspace, then cross-querying and tracking progress.
//!
//! # Architecture
//!
//! A **Workspace** holds one or more **CodebaseContext** entries, each wrapping a
//! [`CodeGraph`] and annotated with a [`ContextRole`] (Source, Target, Reference,
//! or Comparison). The [`WorkspaceManager`] owns all workspaces, tracks the
//! currently active one, and provides cross-context query and comparison APIs.
//!
//! The [`TranslationMap`] sits on top of a workspace and tracks the porting
//! status of individual symbols from a source context to a target context.
//!
//! # Example
//!
//! ```rust,no_run
//! use agentic_codebase::workspace::{WorkspaceManager, ContextRole, TranslationMap, TranslationStatus};
//! use agentic_codebase::graph::CodeGraph;
//!
//! let mut mgr = WorkspaceManager::new();
//! let ws_id = mgr.create("cpp-to-rust");
//!
//! let cpp_graph = CodeGraph::with_default_dimension();
//! let rs_graph = CodeGraph::with_default_dimension();
//!
//! let src = mgr.add_context(&ws_id, "/src/cpp", ContextRole::Source, Some("C++".into()), cpp_graph).unwrap();
//! let tgt = mgr.add_context(&ws_id, "/src/rust", ContextRole::Target, Some("Rust".into()), rs_graph).unwrap();
//!
//! let mut tmap = TranslationMap::new(src, tgt);
//! tmap.record("process_payment", Some("process_payment"), TranslationStatus::Ported, None);
//! let progress = tmap.progress();
//! ```

mod manager;
mod translation;

pub use manager::{
    CodebaseContext, Comparison, ContextComparison, ContextRole, CrossContextResult,
    CrossReference, SymbolMatch, Workspace, WorkspaceManager,
};
pub use translation::{TranslationMap, TranslationMapping, TranslationProgress, TranslationStatus};
