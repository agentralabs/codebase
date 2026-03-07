use serde::{Deserialize, Serialize};

/// The level of detail requested for a query extraction.
///
/// Ordered from cheapest (fewest tokens) to most expensive.
/// Default is `IdsOnly` to be maximally token-conservative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExtractionIntent {
    /// Only check if the item exists. Cheapest possible query.
    Exists,
    /// Return only identifiers. Default and very cheap.
    IdsOnly,
    /// Return a compact summary (key fields only).
    Summary,
    /// Return specific named fields.
    Fields,
    /// Return the full object. Most expensive.
    Full,
}

impl ExtractionIntent {
    /// Estimated relative token cost for this intent level.
    /// Returns a multiplier relative to `IdsOnly` (which is 1).
    pub fn estimated_tokens(&self) -> u64 {
        match self {
            ExtractionIntent::Exists => 1,
            ExtractionIntent::IdsOnly => 2,
            ExtractionIntent::Summary => 10,
            ExtractionIntent::Fields => 25,
            ExtractionIntent::Full => 100,
        }
    }

    /// Whether this intent requests the full payload.
    pub fn is_full(&self) -> bool {
        matches!(self, ExtractionIntent::Full)
    }

    /// Whether this is a minimal (token-conservative) intent.
    pub fn is_minimal(&self) -> bool {
        matches!(self, ExtractionIntent::Exists | ExtractionIntent::IdsOnly)
    }
}

impl Default for ExtractionIntent {
    fn default() -> Self {
        ExtractionIntent::IdsOnly
    }
}

/// Trait for types that can have an extraction intent applied to scope their output.
pub trait Scopeable {
    /// The scoped output type.
    type Output;

    /// Apply the given intent to produce a scoped result.
    fn apply_intent(&self, intent: ExtractionIntent) -> ScopedResult<Self::Output>;
}

/// The result of applying an extraction intent to scope a query response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScopedResult<T> {
    /// The item exists (response to `Exists` intent).
    Exists(bool),
    /// Only identifiers are returned.
    IdsOnly(Vec<String>),
    /// A compact summary.
    Summary(String),
    /// Selected fields.
    Fields(T),
    /// The full object.
    Full(T),
}

impl<T> ScopedResult<T> {
    /// Estimated token cost of this result.
    pub fn estimated_tokens(&self) -> u64 {
        match self {
            ScopedResult::Exists(_) => 1,
            ScopedResult::IdsOnly(ids) => ids.len() as u64 * 2,
            ScopedResult::Summary(_) => 10,
            ScopedResult::Fields(_) => 25,
            ScopedResult::Full(_) => 100,
        }
    }
}

/// Apply an extraction intent to a full-size data vector, returning the
/// scoped result. This is the primary entry point for token conservation
/// at the query layer.
pub fn apply_intent<T: Clone + Serialize>(
    data: &[T],
    intent: ExtractionIntent,
    id_extractor: impl Fn(&T) -> String,
    summary_extractor: impl Fn(&[T]) -> String,
) -> ScopedResult<Vec<T>> {
    match intent {
        ExtractionIntent::Exists => ScopedResult::Exists(!data.is_empty()),
        ExtractionIntent::IdsOnly => {
            ScopedResult::IdsOnly(data.iter().map(&id_extractor).collect())
        }
        ExtractionIntent::Summary => ScopedResult::Summary(summary_extractor(data)),
        ExtractionIntent::Fields => ScopedResult::Fields(data.to_vec()),
        ExtractionIntent::Full => ScopedResult::Full(data.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_ids_only() {
        assert_eq!(ExtractionIntent::default(), ExtractionIntent::IdsOnly);
    }

    #[test]
    fn test_estimated_tokens_ordering() {
        assert!(
            ExtractionIntent::Exists.estimated_tokens()
                < ExtractionIntent::IdsOnly.estimated_tokens()
        );
        assert!(
            ExtractionIntent::IdsOnly.estimated_tokens()
                < ExtractionIntent::Summary.estimated_tokens()
        );
        assert!(
            ExtractionIntent::Summary.estimated_tokens()
                < ExtractionIntent::Fields.estimated_tokens()
        );
        assert!(
            ExtractionIntent::Fields.estimated_tokens() < ExtractionIntent::Full.estimated_tokens()
        );
    }

    #[test]
    fn test_is_full() {
        assert!(!ExtractionIntent::IdsOnly.is_full());
        assert!(ExtractionIntent::Full.is_full());
    }

    #[test]
    fn test_is_minimal() {
        assert!(ExtractionIntent::Exists.is_minimal());
        assert!(ExtractionIntent::IdsOnly.is_minimal());
        assert!(!ExtractionIntent::Summary.is_minimal());
        assert!(!ExtractionIntent::Full.is_minimal());
    }

    #[test]
    fn test_apply_intent_exists() {
        let data = vec!["a", "b", "c"];
        let result = apply_intent(
            &data,
            ExtractionIntent::Exists,
            |s| s.to_string(),
            |_| "summary".to_string(),
        );
        match result {
            ScopedResult::Exists(true) => {}
            _ => panic!("expected Exists(true)"),
        }
    }

    #[test]
    fn test_apply_intent_ids_only() {
        let data = vec!["alpha", "beta"];
        let result = apply_intent(
            &data,
            ExtractionIntent::IdsOnly,
            |s| s.to_string(),
            |_| String::new(),
        );
        match result {
            ScopedResult::IdsOnly(ids) => {
                assert_eq!(ids, vec!["alpha", "beta"]);
            }
            _ => panic!("expected IdsOnly"),
        }
    }

    #[test]
    fn test_apply_intent_full() {
        let data = vec![1, 2, 3];
        let result = apply_intent(
            &data,
            ExtractionIntent::Full,
            |n| n.to_string(),
            |_| String::new(),
        );
        match result {
            ScopedResult::Full(v) => assert_eq!(v, vec![1, 2, 3]),
            _ => panic!("expected Full"),
        }
    }

    #[test]
    fn test_ids_only_much_cheaper_than_full() {
        let ids_only = ExtractionIntent::IdsOnly.estimated_tokens();
        let full = ExtractionIntent::Full.estimated_tokens();
        assert!(full >= ids_only * 10, "Full should be at least 10x IdsOnly");
    }

    #[test]
    fn test_scoped_result_estimated_tokens() {
        let exists: ScopedResult<()> = ScopedResult::Exists(true);
        assert_eq!(exists.estimated_tokens(), 1);

        let ids: ScopedResult<()> = ScopedResult::IdsOnly(vec!["a".into(), "b".into()]);
        assert_eq!(ids.estimated_tokens(), 4);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let intent = ExtractionIntent::Summary;
        let json = serde_json::to_string(&intent).unwrap();
        let back: ExtractionIntent = serde_json::from_str(&json).unwrap();
        assert_eq!(intent, back);
    }
}
