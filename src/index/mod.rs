//! Index structures for fast lookup.
//!
//! Each index is built from a [`CodeGraph`] and provides O(1) or near-O(1)
//! lookups on a specific dimension (symbol name, type, path, language, or
//! embedding similarity). Indexes are independent and can be rebuilt
//! incrementally.

pub mod embedding_index;
pub mod language_index;
pub mod path_index;
pub mod semantic_search;
pub mod symbol_index;
pub mod type_index;

pub use embedding_index::{EmbeddingIndex, EmbeddingMatch};
pub use language_index::LanguageIndex;
pub use path_index::PathIndex;
pub use semantic_search::{
    QueryIntent, SemanticMatch, SemanticQuery, SemanticSearchEngine, SemanticSearchResult,
};
pub use symbol_index::SymbolIndex;
pub use type_index::TypeIndex;
