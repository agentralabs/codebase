use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// The layer at which a query was resolved, from cheapest to most expensive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Layer {
    /// Resolved from cache (zero token cost).
    Cache,
    /// Resolved from an index lookup.
    Index,
    /// Resolved via scoped extraction (partial data).
    Scoped,
    /// Resolved via delta (only changes).
    Delta,
    /// Full data retrieval (most expensive).
    Full,
}

impl Layer {
    /// Typical token cost multiplier for this layer.
    pub fn cost_multiplier(&self) -> f64 {
        match self {
            Layer::Cache => 0.0,
            Layer::Index => 0.05,
            Layer::Scoped => 0.1,
            Layer::Delta => 0.3,
            Layer::Full => 1.0,
        }
    }

    /// All layers in order from cheapest to most expensive.
    pub fn all() -> &'static [Layer] {
        &[
            Layer::Cache,
            Layer::Index,
            Layer::Scoped,
            Layer::Delta,
            Layer::Full,
        ]
    }
}

/// Thread-safe per-layer token usage tracking.
pub struct TokenMetrics {
    cache_tokens: AtomicU64,
    index_tokens: AtomicU64,
    scoped_tokens: AtomicU64,
    delta_tokens: AtomicU64,
    full_tokens: AtomicU64,
    tokens_saved: AtomicU64,
    total_queries: AtomicU64,
}

impl TokenMetrics {
    /// Create a new zeroed token metrics tracker.
    pub fn new() -> Self {
        Self {
            cache_tokens: AtomicU64::new(0),
            index_tokens: AtomicU64::new(0),
            scoped_tokens: AtomicU64::new(0),
            delta_tokens: AtomicU64::new(0),
            full_tokens: AtomicU64::new(0),
            tokens_saved: AtomicU64::new(0),
            total_queries: AtomicU64::new(0),
        }
    }

    /// Record tokens used at a specific layer.
    pub fn record(&self, layer: Layer, tokens: u64) {
        self.total_queries.fetch_add(1, Ordering::Relaxed);
        match layer {
            Layer::Cache => self.cache_tokens.fetch_add(tokens, Ordering::Relaxed),
            Layer::Index => self.index_tokens.fetch_add(tokens, Ordering::Relaxed),
            Layer::Scoped => self.scoped_tokens.fetch_add(tokens, Ordering::Relaxed),
            Layer::Delta => self.delta_tokens.fetch_add(tokens, Ordering::Relaxed),
            Layer::Full => self.full_tokens.fetch_add(tokens, Ordering::Relaxed),
        };
    }

    /// Record tokens that were saved by not doing a full retrieval.
    pub fn record_savings(&self, saved: u64) {
        self.tokens_saved.fetch_add(saved, Ordering::Relaxed);
    }

    /// Get tokens used at a specific layer.
    pub fn tokens_at(&self, layer: Layer) -> u64 {
        match layer {
            Layer::Cache => self.cache_tokens.load(Ordering::Relaxed),
            Layer::Index => self.index_tokens.load(Ordering::Relaxed),
            Layer::Scoped => self.scoped_tokens.load(Ordering::Relaxed),
            Layer::Delta => self.delta_tokens.load(Ordering::Relaxed),
            Layer::Full => self.full_tokens.load(Ordering::Relaxed),
        }
    }

    /// Total tokens used across all layers.
    pub fn total_tokens_used(&self) -> u64 {
        self.cache_tokens.load(Ordering::Relaxed)
            + self.index_tokens.load(Ordering::Relaxed)
            + self.scoped_tokens.load(Ordering::Relaxed)
            + self.delta_tokens.load(Ordering::Relaxed)
            + self.full_tokens.load(Ordering::Relaxed)
    }

    /// Total tokens saved.
    pub fn total_tokens_saved(&self) -> u64 {
        self.tokens_saved.load(Ordering::Relaxed)
    }

    /// Total number of queries recorded.
    pub fn total_queries(&self) -> u64 {
        self.total_queries.load(Ordering::Relaxed)
    }

    /// Conservation score: ratio of tokens saved to total possible tokens.
    /// Returns a value between 0.0 (no conservation) and 1.0 (perfect conservation).
    pub fn conservation_score(&self) -> f64 {
        let used = self.total_tokens_used() as f64;
        let saved = self.total_tokens_saved() as f64;
        let total = used + saved;
        if total == 0.0 {
            0.0
        } else {
            saved / total
        }
    }

    /// Reset all counters.
    pub fn reset(&self) {
        self.cache_tokens.store(0, Ordering::Relaxed);
        self.index_tokens.store(0, Ordering::Relaxed);
        self.scoped_tokens.store(0, Ordering::Relaxed);
        self.delta_tokens.store(0, Ordering::Relaxed);
        self.full_tokens.store(0, Ordering::Relaxed);
        self.tokens_saved.store(0, Ordering::Relaxed);
        self.total_queries.store(0, Ordering::Relaxed);
    }

    /// Take a serializable snapshot.
    pub fn snapshot(&self) -> TokenMetricsSnapshot {
        TokenMetricsSnapshot {
            cache_tokens: self.tokens_at(Layer::Cache),
            index_tokens: self.tokens_at(Layer::Index),
            scoped_tokens: self.tokens_at(Layer::Scoped),
            delta_tokens: self.tokens_at(Layer::Delta),
            full_tokens: self.tokens_at(Layer::Full),
            total_used: self.total_tokens_used(),
            total_saved: self.total_tokens_saved(),
            total_queries: self.total_queries(),
            conservation_score: self.conservation_score(),
        }
    }
}

impl Default for TokenMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable snapshot of token metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetricsSnapshot {
    pub cache_tokens: u64,
    pub index_tokens: u64,
    pub scoped_tokens: u64,
    pub delta_tokens: u64,
    pub full_tokens: u64,
    pub total_used: u64,
    pub total_saved: u64,
    pub total_queries: u64,
    pub conservation_score: f64,
}

/// Metrics for a single response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMetrics {
    /// The layer at which this response was resolved.
    pub layer: Layer,
    /// Tokens actually used for this response.
    pub tokens_used: u64,
    /// Tokens saved compared to a full retrieval.
    pub tokens_saved: u64,
    /// Whether this was a cache hit.
    pub cache_hit: bool,
}

impl ResponseMetrics {
    /// Create metrics for a cache hit (zero tokens used).
    pub fn cache_hit(full_cost: u64) -> Self {
        Self {
            layer: Layer::Cache,
            tokens_used: 0,
            tokens_saved: full_cost,
            cache_hit: true,
        }
    }

    /// Create metrics for a response at the given layer.
    pub fn at_layer(layer: Layer, tokens_used: u64, full_cost: u64) -> Self {
        Self {
            layer,
            tokens_used,
            tokens_saved: full_cost.saturating_sub(tokens_used),
            cache_hit: matches!(layer, Layer::Cache),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_cost_ordering() {
        assert!(Layer::Cache.cost_multiplier() < Layer::Index.cost_multiplier());
        assert!(Layer::Index.cost_multiplier() < Layer::Scoped.cost_multiplier());
        assert!(Layer::Scoped.cost_multiplier() < Layer::Delta.cost_multiplier());
        assert!(Layer::Delta.cost_multiplier() < Layer::Full.cost_multiplier());
    }

    #[test]
    fn test_new_metrics_zero() {
        let m = TokenMetrics::new();
        assert_eq!(m.total_tokens_used(), 0);
        assert_eq!(m.total_tokens_saved(), 0);
        assert_eq!(m.total_queries(), 0);
    }

    #[test]
    fn test_record_tokens() {
        let m = TokenMetrics::new();
        m.record(Layer::Cache, 0);
        m.record(Layer::Full, 100);
        assert_eq!(m.tokens_at(Layer::Full), 100);
        assert_eq!(m.total_queries(), 2);
    }

    #[test]
    fn test_conservation_score_no_data() {
        let m = TokenMetrics::new();
        assert_eq!(m.conservation_score(), 0.0);
    }

    #[test]
    fn test_conservation_score_all_saved() {
        let m = TokenMetrics::new();
        m.record(Layer::Cache, 0);
        m.record_savings(100);
        assert_eq!(m.conservation_score(), 1.0);
    }

    #[test]
    fn test_conservation_score_mixed() {
        let m = TokenMetrics::new();
        m.record(Layer::Scoped, 10);
        m.record_savings(90);
        // saved=90, used=10, total=100, score=0.9
        assert!((m.conservation_score() - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_response_metrics_cache_hit() {
        let rm = ResponseMetrics::cache_hit(100);
        assert_eq!(rm.tokens_used, 0);
        assert_eq!(rm.tokens_saved, 100);
        assert!(rm.cache_hit);
    }

    #[test]
    fn test_response_metrics_at_layer() {
        let rm = ResponseMetrics::at_layer(Layer::Scoped, 10, 100);
        assert_eq!(rm.tokens_used, 10);
        assert_eq!(rm.tokens_saved, 90);
        assert!(!rm.cache_hit);
    }

    #[test]
    fn test_snapshot() {
        let m = TokenMetrics::new();
        m.record(Layer::Cache, 5);
        m.record(Layer::Full, 100);
        m.record_savings(50);
        let snap = m.snapshot();
        assert_eq!(snap.cache_tokens, 5);
        assert_eq!(snap.full_tokens, 100);
        assert_eq!(snap.total_saved, 50);
    }

    #[test]
    fn test_reset() {
        let m = TokenMetrics::new();
        m.record(Layer::Full, 100);
        m.record_savings(50);
        m.reset();
        assert_eq!(m.total_tokens_used(), 0);
        assert_eq!(m.total_tokens_saved(), 0);
    }

    #[test]
    fn test_layer_serialization() {
        let layer = Layer::Scoped;
        let json = serde_json::to_string(&layer).unwrap();
        let back: Layer = serde_json::from_str(&json).unwrap();
        assert_eq!(layer, back);
    }
}
