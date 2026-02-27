//! Semantic Search Enhancement — Invention 9.
//!
//! Natural-language code search that understands intent, not just keywords.
//! Wraps the existing `EmbeddingIndex` with query understanding and intent
//! classification to provide more meaningful search results.

use serde::{Deserialize, Serialize};

use crate::graph::CodeGraph;
use crate::index::embedding_index::{EmbeddingIndex, EmbeddingMatch};
use crate::types::CodeUnitType;

// ── Types ────────────────────────────────────────────────────────────────────

/// Intent behind a semantic query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryIntent {
    /// Looking for a function/method definition.
    FindFunction,
    /// Looking for a type/struct/class definition.
    FindType,
    /// Looking for usages / call sites.
    FindUsage,
    /// Looking for implementations of a concept.
    FindImplementation,
    /// Looking for tests.
    FindTest,
    /// General text search.
    General,
}

impl QueryIntent {
    /// Classify intent from a natural-language query.
    pub fn classify(query: &str) -> Self {
        let q = query.to_lowercase();
        if q.contains("test") || q.contains("spec") || q.starts_with("how is") {
            return Self::FindTest;
        }
        if q.contains("function")
            || q.contains("method")
            || q.contains("fn ")
            || q.starts_with("def ")
        {
            return Self::FindFunction;
        }
        if q.contains("type")
            || q.contains("struct")
            || q.contains("class")
            || q.contains("enum")
            || q.contains("interface")
        {
            return Self::FindType;
        }
        if q.contains("usage")
            || q.contains("call")
            || q.contains("who uses")
            || q.contains("where is")
        {
            return Self::FindUsage;
        }
        if q.contains("implement") || q.contains("how does") || q.contains("logic for") {
            return Self::FindImplementation;
        }
        Self::General
    }

    /// Label for display.
    pub fn label(&self) -> &str {
        match self {
            Self::FindFunction => "find_function",
            Self::FindType => "find_type",
            Self::FindUsage => "find_usage",
            Self::FindImplementation => "find_implementation",
            Self::FindTest => "find_test",
            Self::General => "general",
        }
    }
}

/// Scope restriction for a search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchScope {
    /// Search the entire codebase.
    All,
    /// Restrict to a specific module path prefix.
    Module(String),
    /// Restrict to a specific file.
    File(String),
    /// Restrict to a specific code unit type.
    UnitType(CodeUnitType),
}

/// A semantic search query with parsed intent and scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticQuery {
    /// Original query string.
    pub raw: String,
    /// Classified intent.
    pub intent: QueryIntent,
    /// Extracted keywords (lowercase).
    pub keywords: Vec<String>,
    /// Scope restriction.
    pub scope: SearchScope,
}

/// A single match from semantic search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticMatch {
    /// Unit ID.
    pub unit_id: u64,
    /// Unit name.
    pub name: String,
    /// Qualified name.
    pub qualified_name: String,
    /// Type label.
    pub unit_type: String,
    /// File path.
    pub file_path: String,
    /// Combined relevance score (0.0–1.0).
    pub relevance: f64,
    /// Why this matched.
    pub explanation: String,
}

/// Full result of a semantic search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchResult {
    /// The parsed query.
    pub query: SemanticQuery,
    /// Ranked matches.
    pub matches: Vec<SemanticMatch>,
    /// Total candidates scanned.
    pub candidates_scanned: usize,
}

// ── SemanticSearchEngine ─────────────────────────────────────────────────────

/// Enhanced semantic search engine wrapping `EmbeddingIndex`.
pub struct SemanticSearchEngine<'g> {
    graph: &'g CodeGraph,
    embedding_index: EmbeddingIndex,
}

impl<'g> SemanticSearchEngine<'g> {
    pub fn new(graph: &'g CodeGraph) -> Self {
        let embedding_index = EmbeddingIndex::build(graph);
        Self {
            graph,
            embedding_index,
        }
    }

    /// Parse a natural-language query into a structured `SemanticQuery`.
    pub fn parse_query(&self, raw: &str) -> SemanticQuery {
        let intent = QueryIntent::classify(raw);
        let keywords = extract_keywords(raw);
        let scope = self.infer_scope(raw);

        SemanticQuery {
            raw: raw.to_string(),
            intent,
            keywords,
            scope,
        }
    }

    /// Perform a semantic search.
    pub fn search(&self, raw_query: &str, top_k: usize) -> SemanticSearchResult {
        let query = self.parse_query(raw_query);
        let candidates_scanned = self.graph.unit_count();

        // Keyword-based scoring across all units
        let mut scored: Vec<SemanticMatch> = Vec::new();

        for unit in self.graph.units() {
            // Apply scope filtering
            match &query.scope {
                SearchScope::All => {}
                SearchScope::Module(prefix) => {
                    if !unit.qualified_name.starts_with(prefix.as_str()) {
                        continue;
                    }
                }
                SearchScope::File(path) => {
                    if unit.file_path.display().to_string() != *path {
                        continue;
                    }
                }
                SearchScope::UnitType(ut) => {
                    if unit.unit_type != *ut {
                        continue;
                    }
                }
            }

            // Apply intent filtering
            let intent_bonus = match query.intent {
                QueryIntent::FindFunction => {
                    if unit.unit_type == CodeUnitType::Function {
                        0.15
                    } else {
                        0.0
                    }
                }
                QueryIntent::FindType => {
                    if unit.unit_type == CodeUnitType::Type {
                        0.15
                    } else {
                        0.0
                    }
                }
                QueryIntent::FindTest => {
                    if unit.unit_type == CodeUnitType::Test {
                        0.15
                    } else {
                        0.0
                    }
                }
                _ => 0.0,
            };

            // Keyword scoring
            let name_lower = unit.name.to_lowercase();
            let qname_lower = unit.qualified_name.to_lowercase();

            let mut keyword_score: f64 = 0.0;
            let mut matched_keywords = Vec::new();

            for kw in &query.keywords {
                if name_lower == *kw {
                    keyword_score += 0.5;
                    matched_keywords.push(format!("exact name match '{}'", kw));
                } else if name_lower.contains(kw.as_str()) {
                    keyword_score += 0.3;
                    matched_keywords.push(format!("name contains '{}'", kw));
                } else if qname_lower.contains(kw.as_str()) {
                    keyword_score += 0.15;
                    matched_keywords.push(format!("qualified name contains '{}'", kw));
                }
            }

            let total_score = (keyword_score + intent_bonus).min(1.0_f64);

            if total_score > 0.1 {
                let explanation = if matched_keywords.is_empty() {
                    format!("Intent match: {}", query.intent.label())
                } else {
                    matched_keywords.join("; ")
                };

                scored.push(SemanticMatch {
                    unit_id: unit.id,
                    name: unit.name.clone(),
                    qualified_name: unit.qualified_name.clone(),
                    unit_type: unit.unit_type.label().to_string(),
                    file_path: unit.file_path.display().to_string(),
                    relevance: total_score,
                    explanation,
                });
            }
        }

        // Sort by relevance descending
        scored.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);

        SemanticSearchResult {
            query,
            matches: scored,
            candidates_scanned,
        }
    }

    /// Find units similar to a given unit by embedding similarity.
    pub fn find_similar(&self, unit_id: u64, top_k: usize) -> Vec<SemanticMatch> {
        let unit = match self.graph.get_unit(unit_id) {
            Some(u) => u,
            None => return Vec::new(),
        };

        let embedding_matches: Vec<EmbeddingMatch> =
            self.embedding_index
                .search(&unit.feature_vec, top_k + 1, 0.0);

        embedding_matches
            .into_iter()
            .filter(|m| m.unit_id != unit_id)
            .take(top_k)
            .filter_map(|m| {
                self.graph.get_unit(m.unit_id).map(|u| SemanticMatch {
                    unit_id: u.id,
                    name: u.name.clone(),
                    qualified_name: u.qualified_name.clone(),
                    unit_type: u.unit_type.label().to_string(),
                    file_path: u.file_path.display().to_string(),
                    relevance: m.score as f64,
                    explanation: format!("Embedding similarity: {:.3}", m.score),
                })
            })
            .collect()
    }

    /// Explain why a unit matched a query.
    pub fn explain_match(&self, unit_id: u64, raw_query: &str) -> Option<String> {
        let unit = self.graph.get_unit(unit_id)?;
        let query = self.parse_query(raw_query);

        let mut reasons = Vec::new();

        for kw in &query.keywords {
            let name_lower = unit.name.to_lowercase();
            if name_lower.contains(kw.as_str()) {
                reasons.push(format!("Name contains keyword '{}'", kw));
            }
            let qname_lower = unit.qualified_name.to_lowercase();
            if qname_lower.contains(kw.as_str()) && !name_lower.contains(kw.as_str()) {
                reasons.push(format!("Qualified name contains keyword '{}'", kw));
            }
        }

        match query.intent {
            QueryIntent::FindFunction if unit.unit_type == CodeUnitType::Function => {
                reasons.push("Matches intent: looking for functions".to_string());
            }
            QueryIntent::FindType if unit.unit_type == CodeUnitType::Type => {
                reasons.push("Matches intent: looking for types".to_string());
            }
            QueryIntent::FindTest if unit.unit_type == CodeUnitType::Test => {
                reasons.push("Matches intent: looking for tests".to_string());
            }
            _ => {}
        }

        if reasons.is_empty() {
            Some("No direct match found".to_string())
        } else {
            Some(reasons.join("; "))
        }
    }

    // ── Internal ─────────────────────────────────────────────────────────

    fn infer_scope(&self, query: &str) -> SearchScope {
        let q = query.to_lowercase();
        // Check for explicit file references
        if q.contains(".rs") || q.contains(".py") || q.contains(".ts") || q.contains(".js") {
            // Try to extract a file path
            for word in query.split_whitespace() {
                if word.contains('.') && !word.starts_with('.') {
                    return SearchScope::File(word.to_string());
                }
            }
        }
        // Check for module references
        if q.contains("in module ") || q.contains("in mod ") {
            if let Some(rest) = q
                .split("in module ")
                .nth(1)
                .or_else(|| q.split("in mod ").nth(1))
            {
                let module = rest.split_whitespace().next().unwrap_or("");
                if !module.is_empty() {
                    return SearchScope::Module(module.to_string());
                }
            }
        }
        SearchScope::All
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Extract meaningful keywords from a query string.
fn extract_keywords(query: &str) -> Vec<String> {
    let stop_words = [
        "the",
        "a",
        "an",
        "is",
        "are",
        "was",
        "were",
        "be",
        "been",
        "being",
        "have",
        "has",
        "had",
        "do",
        "does",
        "did",
        "will",
        "would",
        "could",
        "should",
        "may",
        "might",
        "shall",
        "can",
        "need",
        "dare",
        "ought",
        "used",
        "to",
        "of",
        "in",
        "for",
        "on",
        "with",
        "at",
        "by",
        "from",
        "as",
        "into",
        "through",
        "during",
        "before",
        "after",
        "above",
        "below",
        "between",
        "out",
        "off",
        "over",
        "under",
        "again",
        "further",
        "then",
        "once",
        "here",
        "there",
        "when",
        "where",
        "why",
        "how",
        "all",
        "each",
        "every",
        "both",
        "few",
        "more",
        "most",
        "other",
        "some",
        "such",
        "no",
        "nor",
        "not",
        "only",
        "own",
        "same",
        "so",
        "than",
        "too",
        "very",
        "just",
        "because",
        "but",
        "and",
        "or",
        "if",
        "while",
        "that",
        "this",
        "what",
        "which",
        "who",
        "whom",
        "find",
        "search",
        "look",
        "show",
        "get",
        "function",
        "method",
        "type",
        "struct",
        "class",
        "enum",
        "test",
        "usage",
        "implement",
        "call",
    ];
    let stop_set: std::collections::HashSet<&str> = stop_words.iter().copied().collect();

    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 2 && !stop_set.contains(w))
        .map(|w| w.to_string())
        .collect()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CodeUnit, CodeUnitType, Language, Span};
    use std::path::PathBuf;

    fn test_graph() -> CodeGraph {
        let mut graph = CodeGraph::with_default_dimension();
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "process_payment".to_string(),
            "billing::process_payment".to_string(),
            PathBuf::from("src/billing.rs"),
            Span::new(1, 0, 20, 0),
        ));
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Type,
            Language::Rust,
            "PaymentResult".to_string(),
            "billing::PaymentResult".to_string(),
            PathBuf::from("src/billing.rs"),
            Span::new(21, 0, 30, 0),
        ));
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Test,
            Language::Rust,
            "test_payment".to_string(),
            "tests::test_payment".to_string(),
            PathBuf::from("tests/billing_test.rs"),
            Span::new(1, 0, 15, 0),
        ));
        graph
    }

    #[test]
    fn classify_intent() {
        assert_eq!(
            QueryIntent::classify("find function process_payment"),
            QueryIntent::FindFunction
        );
        assert_eq!(
            QueryIntent::classify("show me the struct User"),
            QueryIntent::FindType
        );
        assert_eq!(
            QueryIntent::classify("test for payment"),
            QueryIntent::FindTest
        );
        assert_eq!(
            QueryIntent::classify("payment processing"),
            QueryIntent::General
        );
    }

    #[test]
    fn keyword_search() {
        let graph = test_graph();
        let engine = SemanticSearchEngine::new(&graph);
        let result = engine.search("payment", 10);
        assert!(result.matches.len() >= 2); // process_payment and PaymentResult
    }

    #[test]
    fn intent_boosts_correct_type() {
        let graph = test_graph();
        let engine = SemanticSearchEngine::new(&graph);
        let result = engine.search("function payment", 10);
        // Function intent should boost process_payment over PaymentResult
        if result.matches.len() >= 2 {
            assert_eq!(result.matches[0].unit_type, "function");
        }
    }

    #[test]
    fn explain_match_works() {
        let graph = test_graph();
        let engine = SemanticSearchEngine::new(&graph);
        let explanation = engine.explain_match(0, "payment");
        assert!(explanation.is_some());
        assert!(explanation.unwrap().contains("payment"));
    }
}
