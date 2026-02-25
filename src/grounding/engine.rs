//! Grounding engine — verifies code claims against the [`CodeGraph`].
//!
//! [`CodeGraph`]: crate::graph::CodeGraph

use crate::graph::CodeGraph;

use super::{Evidence, Grounded, GroundingResult};

// ── Common English stop-words to filter from reference extraction ────────────
const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
    "do", "does", "did", "will", "would", "shall", "should", "may", "might", "must", "can",
    "could", "to", "of", "in", "for", "on", "with", "at", "by", "from", "as", "into", "about",
    "between", "through", "during", "before", "after", "above", "below", "up", "down", "out",
    "off", "over", "under", "again", "further", "then", "once", "here", "there", "when", "where",
    "why", "how", "all", "each", "every", "both", "few", "more", "most", "other", "some", "such",
    "no", "nor", "not", "only", "own", "same", "so", "than", "too", "very", "just", "because",
    "but", "and", "or", "if", "while", "that", "this", "these", "those", "it", "its", "my",
    "your", "his", "her", "our", "their", "what", "which", "who", "whom", "we", "you", "he",
    "she", "they", "me", "him", "us", "them", "i",
];

// ── Pattern detection helpers (no regex crate) ──────────────────────────────

/// Returns `true` if the string is a valid `snake_case` identifier.
///
/// Pattern: `[a-z][a-z0-9]*(_[a-z0-9]+)+`
fn is_snake_case(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return false;
    }
    // Must start with lowercase letter
    if !chars[0].is_ascii_lowercase() {
        return false;
    }
    // Must contain at least one underscore
    if !s.contains('_') {
        return false;
    }
    // Every character must be lowercase alphanumeric or underscore
    if !chars
        .iter()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
    {
        return false;
    }
    // No leading/trailing/consecutive underscores
    if s.starts_with('_') || s.ends_with('_') || s.contains("__") {
        return false;
    }
    // Each segment after underscore must start with a lowercase letter or digit
    for segment in s.split('_') {
        if segment.is_empty() {
            return false;
        }
    }
    true
}

/// Returns `true` if the string is a valid `CamelCase` identifier.
///
/// Pattern: `[A-Z][a-z]+([A-Z][a-z]+)+`
fn is_camel_case(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 2 {
        return false;
    }
    // Must start with an uppercase letter
    if !chars[0].is_ascii_uppercase() {
        return false;
    }
    // All characters must be alphabetic or digits
    if !chars.iter().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    // Must have at least two uppercase letters (one at start, one in body)
    // to distinguish CamelCase from a regular capitalized word.
    let upper_count = chars.iter().filter(|c| c.is_ascii_uppercase()).count();
    if upper_count < 2 {
        return false;
    }
    // After the first char there must be at least one lowercase letter
    let has_lower_after_first = chars[1..].iter().any(|c| c.is_ascii_lowercase());
    if !has_lower_after_first {
        return false;
    }
    true
}

/// Returns `true` if the string is a valid `SCREAMING_CASE` identifier.
///
/// Pattern: `[A-Z][A-Z0-9]*(_[A-Z0-9]+)+`
fn is_screaming_case(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return false;
    }
    // Must start with uppercase letter
    if !chars[0].is_ascii_uppercase() {
        return false;
    }
    // Must contain at least one underscore
    if !s.contains('_') {
        return false;
    }
    // Every character must be uppercase alphanumeric or underscore
    if !chars
        .iter()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
    {
        return false;
    }
    // No leading/trailing/consecutive underscores
    if s.starts_with('_') || s.ends_with('_') || s.contains("__") {
        return false;
    }
    for segment in s.split('_') {
        if segment.is_empty() {
            return false;
        }
    }
    true
}

/// Returns `true` if `word` is a common English stop-word.
fn is_stop_word(word: &str) -> bool {
    STOP_WORDS.contains(&word.to_lowercase().as_str())
}

// ── Reference extraction ─────────────────────────────────────────────────────

/// Extract potential code identifiers from a natural-language claim.
///
/// Looks for:
/// - Backtick-quoted identifiers (e.g. `` `foo_bar` ``)
/// - `snake_case` tokens
/// - `CamelCase` tokens
/// - `SCREAMING_CASE` tokens
///
/// Common English stop-words are filtered out.
pub fn extract_code_references(claim: &str) -> Vec<String> {
    let mut refs: Vec<String> = Vec::new();

    // 1. Extract backtick-quoted identifiers
    let mut in_backtick = false;
    let mut buf = String::new();
    for ch in claim.chars() {
        if ch == '`' {
            if in_backtick {
                let trimmed = buf.trim().to_string();
                if !trimmed.is_empty() && !is_stop_word(&trimmed) {
                    refs.push(trimmed);
                }
                buf.clear();
            }
            in_backtick = !in_backtick;
        } else if in_backtick {
            buf.push(ch);
        }
    }

    // 2. Tokenize remaining text and check patterns
    // Split on anything that isn't alphanumeric or underscore
    let tokens: Vec<&str> = claim
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|t| !t.is_empty())
        .collect();

    for token in &tokens {
        if is_stop_word(token) {
            continue;
        }
        if is_snake_case(token) || is_camel_case(token) || is_screaming_case(token) {
            let s = (*token).to_string();
            if !refs.contains(&s) {
                refs.push(s);
            }
        }
    }

    refs
}

// ── Levenshtein edit distance ────────────────────────────────────────────────

/// Compute the Levenshtein edit distance between two strings.
///
/// Uses the standard iterative dynamic-programming approach with O(min(m,n))
/// space.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    // Use two rows instead of full matrix to save memory.
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1) // deletion
                .min(curr[j - 1] + 1) // insertion
                .min(prev[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

// ── GroundingEngine ──────────────────────────────────────────────────────────

/// Engine that verifies code claims against a [`CodeGraph`].
///
/// Wraps a reference to the code graph and implements the [`Grounded`] trait
/// to provide anti-hallucination checks.
///
/// # Examples
///
/// ```ignore
/// let engine = GroundingEngine::new(&graph);
/// match engine.ground_claim("process_payment validates the Decimal amount") {
///     GroundingResult::Verified { evidence, confidence } => { /* all good */ }
///     GroundingResult::Partial { unsupported, .. } => { /* some unknown refs */ }
///     GroundingResult::Ungrounded { claim, suggestions } => { /* hallucination */ }
/// }
/// ```
///
/// [`CodeGraph`]: crate::graph::CodeGraph
pub struct GroundingEngine<'g> {
    graph: &'g CodeGraph,
}

impl<'g> GroundingEngine<'g> {
    /// Create a new grounding engine backed by the given code graph.
    pub fn new(graph: &'g CodeGraph) -> Self {
        Self { graph }
    }

    /// Build an [`Evidence`] record from a [`CodeUnit`][crate::types::CodeUnit].
    fn evidence_from_unit(unit: &crate::types::CodeUnit) -> Evidence {
        Evidence {
            node_id: unit.id,
            node_type: unit.unit_type.label().to_string(),
            name: unit.name.clone(),
            file_path: unit.file_path.display().to_string(),
            line_number: Some(unit.span.start_line),
            snippet: unit.signature.clone(),
        }
    }
}

impl<'g> Grounded for GroundingEngine<'g> {
    fn ground_claim(&self, claim: &str) -> GroundingResult {
        let refs = extract_code_references(claim);

        // No identifiable code references — treat as ungrounded.
        if refs.is_empty() {
            return GroundingResult::Ungrounded {
                claim: claim.to_string(),
                suggestions: Vec::new(),
            };
        }

        let mut all_evidence: Vec<Evidence> = Vec::new();
        let mut supported: Vec<String> = Vec::new();
        let mut unsupported: Vec<String> = Vec::new();

        for reference in &refs {
            let evidence = self.find_evidence(reference);
            if evidence.is_empty() {
                unsupported.push(reference.clone());
            } else {
                supported.push(reference.clone());
                all_evidence.extend(evidence);
            }
        }

        if unsupported.is_empty() {
            // All references verified.
            let confidence = 1.0_f32; // all matched
            GroundingResult::Verified {
                evidence: all_evidence,
                confidence,
            }
        } else if supported.is_empty() {
            // Nothing matched — potential hallucination.
            let mut suggestions: Vec<String> = Vec::new();
            for u in &unsupported {
                suggestions.extend(self.suggest_similar(u, 3));
            }
            // Deduplicate suggestions
            suggestions.sort();
            suggestions.dedup();
            GroundingResult::Ungrounded {
                claim: claim.to_string(),
                suggestions,
            }
        } else {
            // Partial match.
            let mut suggestions: Vec<String> = Vec::new();
            for u in &unsupported {
                suggestions.extend(self.suggest_similar(u, 3));
            }
            suggestions.sort();
            suggestions.dedup();
            GroundingResult::Partial {
                supported,
                unsupported,
                suggestions,
            }
        }
    }

    fn find_evidence(&self, name: &str) -> Vec<Evidence> {
        let mut results: Vec<Evidence> = Vec::new();

        // 1. Exact match on simple name
        for unit in self.graph.units() {
            if unit.name == name {
                results.push(Self::evidence_from_unit(unit));
            }
        }

        // 2. If no exact match, try qualified_name contains
        if results.is_empty() {
            for unit in self.graph.units() {
                if unit.qualified_name.contains(name) {
                    results.push(Self::evidence_from_unit(unit));
                }
            }
        }

        // 3. If still empty, try case-insensitive exact match on name
        if results.is_empty() {
            let lower = name.to_lowercase();
            for unit in self.graph.units() {
                if unit.name.to_lowercase() == lower {
                    results.push(Self::evidence_from_unit(unit));
                }
            }
        }

        results
    }

    fn suggest_similar(&self, name: &str, limit: usize) -> Vec<String> {
        let lower = name.to_lowercase();
        let threshold = name.len() / 2;

        let mut candidates: Vec<(String, usize)> = Vec::new();

        for unit in self.graph.units() {
            let unit_lower = unit.name.to_lowercase();

            // Prefix match — always include with distance 0
            if unit_lower.starts_with(&lower) || lower.starts_with(&unit_lower) {
                if !candidates.iter().any(|(n, _)| *n == unit.name) {
                    candidates.push((unit.name.clone(), 0));
                }
                continue;
            }

            // Edit distance
            let dist = levenshtein(&lower, &unit_lower);
            if dist <= threshold && dist > 0 {
                if !candidates.iter().any(|(n, _)| *n == unit.name) {
                    candidates.push((unit.name.clone(), dist));
                }
            }
        }

        // Sort by distance (ascending), then alphabetically for ties
        candidates.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

        candidates
            .into_iter()
            .take(limit)
            .map(|(name, _)| name)
            .collect()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CodeUnit, CodeUnitType, Language, Span};
    use std::path::PathBuf;

    /// Build a small test graph for grounding tests.
    fn test_graph() -> CodeGraph {
        let mut graph = CodeGraph::with_default_dimension();

        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Python,
            "process_payment".to_string(),
            "payments.stripe.process_payment".to_string(),
            PathBuf::from("src/payments/stripe.py"),
            Span::new(10, 0, 30, 0),
        ));

        graph.add_unit(CodeUnit::new(
            CodeUnitType::Type,
            Language::Rust,
            "CodeGraph".to_string(),
            "crate::graph::CodeGraph".to_string(),
            PathBuf::from("src/graph/code_graph.rs"),
            Span::new(17, 0, 250, 0),
        ));

        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "add_unit".to_string(),
            "crate::graph::CodeGraph::add_unit".to_string(),
            PathBuf::from("src/graph/code_graph.rs"),
            Span::new(58, 0, 64, 0),
        ));

        graph.add_unit(CodeUnit::new(
            CodeUnitType::Config,
            Language::Rust,
            "MAX_EDGES_PER_UNIT".to_string(),
            "crate::types::MAX_EDGES_PER_UNIT".to_string(),
            PathBuf::from("src/types/mod.rs"),
            Span::new(40, 0, 40, 0),
        ));

        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Python,
            "validate_amount".to_string(),
            "payments.utils.validate_amount".to_string(),
            PathBuf::from("src/payments/utils.py"),
            Span::new(5, 0, 15, 0),
        ));

        graph
    }

    // ── extract_code_references ──────────────────────────────────────────

    #[test]
    fn extract_snake_case_refs() {
        let refs = extract_code_references("The process_payment function validates the amount");
        assert!(refs.contains(&"process_payment".to_string()));
    }

    #[test]
    fn extract_camel_case_refs() {
        let refs = extract_code_references("The CodeGraph struct holds all units");
        assert!(refs.contains(&"CodeGraph".to_string()));
    }

    #[test]
    fn extract_screaming_case_refs() {
        let refs =
            extract_code_references("The constant MAX_EDGES_PER_UNIT limits the edge count");
        assert!(refs.contains(&"MAX_EDGES_PER_UNIT".to_string()));
    }

    #[test]
    fn extract_backtick_refs() {
        let refs = extract_code_references("Call `add_unit` to insert a node");
        assert!(refs.contains(&"add_unit".to_string()));
    }

    #[test]
    fn extract_mixed_refs() {
        let refs = extract_code_references(
            "The `process_payment` function in CodeGraph uses MAX_EDGES_PER_UNIT",
        );
        assert!(refs.contains(&"process_payment".to_string()));
        assert!(refs.contains(&"CodeGraph".to_string()));
        assert!(refs.contains(&"MAX_EDGES_PER_UNIT".to_string()));
    }

    #[test]
    fn extract_filters_stop_words() {
        let refs = extract_code_references("the is a an in on");
        assert!(refs.is_empty());
    }

    #[test]
    fn extract_no_duplicates() {
        let refs = extract_code_references(
            "`process_payment` calls process_payment to handle the process_payment flow",
        );
        let count = refs
            .iter()
            .filter(|r| *r == "process_payment")
            .count();
        assert_eq!(count, 1);
    }

    // ── ground_claim ─────────────────────────────────────────────────────

    #[test]
    fn ground_verified_claim() {
        let graph = test_graph();
        let engine = GroundingEngine::new(&graph);

        let result = engine.ground_claim("The process_payment function exists");
        match result {
            GroundingResult::Verified { evidence, confidence } => {
                assert!(!evidence.is_empty());
                assert!(confidence > 0.0);
                assert_eq!(evidence[0].name, "process_payment");
            }
            other => panic!("Expected Verified, got {:?}", other),
        }
    }

    #[test]
    fn ground_ungrounded_claim() {
        let graph = test_graph();
        let engine = GroundingEngine::new(&graph);

        let result = engine.ground_claim("The send_invoice function sends emails");
        match result {
            GroundingResult::Ungrounded { claim, .. } => {
                assert!(claim.contains("send_invoice"));
            }
            other => panic!("Expected Ungrounded, got {:?}", other),
        }
    }

    #[test]
    fn ground_partial_claim() {
        let graph = test_graph();
        let engine = GroundingEngine::new(&graph);

        let result =
            engine.ground_claim("process_payment calls send_notification after success");
        match result {
            GroundingResult::Partial {
                supported,
                unsupported,
                ..
            } => {
                assert!(supported.contains(&"process_payment".to_string()));
                assert!(unsupported.contains(&"send_notification".to_string()));
            }
            other => panic!("Expected Partial, got {:?}", other),
        }
    }

    #[test]
    fn ground_no_refs_is_ungrounded() {
        let graph = test_graph();
        let engine = GroundingEngine::new(&graph);

        let result = engine.ground_claim("This is a normal English sentence.");
        assert!(matches!(result, GroundingResult::Ungrounded { .. }));
    }

    // ── find_evidence ────────────────────────────────────────────────────

    #[test]
    fn find_evidence_exact_name() {
        let graph = test_graph();
        let engine = GroundingEngine::new(&graph);

        let ev = engine.find_evidence("add_unit");
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].name, "add_unit");
        assert_eq!(ev[0].node_type, "function");
    }

    #[test]
    fn find_evidence_qualified_fallback() {
        let graph = test_graph();
        let engine = GroundingEngine::new(&graph);

        // "stripe" appears in the qualified name of process_payment
        let ev = engine.find_evidence("stripe");
        assert!(!ev.is_empty());
        assert_eq!(ev[0].name, "process_payment");
    }

    #[test]
    fn find_evidence_case_insensitive_fallback() {
        let graph = test_graph();
        let engine = GroundingEngine::new(&graph);

        let ev = engine.find_evidence("codegraph");
        assert!(!ev.is_empty());
        assert_eq!(ev[0].name, "CodeGraph");
    }

    #[test]
    fn find_evidence_nonexistent() {
        let graph = test_graph();
        let engine = GroundingEngine::new(&graph);

        let ev = engine.find_evidence("nonexistent_function");
        assert!(ev.is_empty());
    }

    // ── suggest_similar ──────────────────────────────────────────────────

    #[test]
    fn suggest_similar_typo() {
        let graph = test_graph();
        let engine = GroundingEngine::new(&graph);

        let suggestions = engine.suggest_similar("process_paymnt", 5);
        assert!(
            suggestions.contains(&"process_payment".to_string()),
            "Expected process_payment in {:?}",
            suggestions
        );
    }

    #[test]
    fn suggest_similar_prefix() {
        let graph = test_graph();
        let engine = GroundingEngine::new(&graph);

        let suggestions = engine.suggest_similar("add", 5);
        assert!(
            suggestions.contains(&"add_unit".to_string()),
            "Expected add_unit in {:?}",
            suggestions
        );
    }

    #[test]
    fn suggest_similar_respects_limit() {
        let graph = test_graph();
        let engine = GroundingEngine::new(&graph);

        let suggestions = engine.suggest_similar("a", 2);
        assert!(suggestions.len() <= 2);
    }

    // ── levenshtein ──────────────────────────────────────────────────────

    #[test]
    fn levenshtein_identical() {
        assert_eq!(levenshtein("hello", "hello"), 0);
    }

    #[test]
    fn levenshtein_one_edit() {
        assert_eq!(levenshtein("kitten", "sitten"), 1);
    }

    #[test]
    fn levenshtein_full_diff() {
        assert_eq!(levenshtein("abc", "xyz"), 3);
    }

    #[test]
    fn levenshtein_empty() {
        assert_eq!(levenshtein("", "hello"), 5);
        assert_eq!(levenshtein("hello", ""), 5);
        assert_eq!(levenshtein("", ""), 0);
    }

    // ── pattern detection helpers ────────────────────────────────────────

    #[test]
    fn test_is_snake_case() {
        assert!(is_snake_case("process_payment"));
        assert!(is_snake_case("add_unit"));
        assert!(is_snake_case("a_b"));
        assert!(!is_snake_case("process")); // no underscore
        assert!(!is_snake_case("ProcessPayment")); // CamelCase
        assert!(!is_snake_case("_leading"));
        assert!(!is_snake_case("trailing_"));
        assert!(!is_snake_case("double__under"));
    }

    #[test]
    fn test_is_camel_case() {
        assert!(is_camel_case("CodeGraph"));
        assert!(is_camel_case("GroundingEngine"));
        assert!(is_camel_case("MyType2"));
        assert!(!is_camel_case("codegraph")); // all lower
        assert!(!is_camel_case("CODEGRAPH")); // all upper
        assert!(!is_camel_case("A")); // too short
        assert!(!is_camel_case("Code")); // only one uppercase
    }

    #[test]
    fn test_is_screaming_case() {
        assert!(is_screaming_case("MAX_EDGES_PER_UNIT"));
        assert!(is_screaming_case("API_KEY"));
        assert!(!is_screaming_case("max_edges")); // lowercase
        assert!(!is_screaming_case("NOUNDERSCORES")); // no underscore
        assert!(!is_screaming_case("_LEADING"));
        assert!(!is_screaming_case("TRAILING_"));
    }
}
