use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::tokens::Layer;
use crate::query::ExtractionIntent;

/// A single audit entry recording a tool invocation's token usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// When this operation occurred.
    pub timestamp: DateTime<Utc>,
    /// The tool that was invoked.
    pub tool: String,
    /// The layer at which the query was resolved.
    pub layer: Layer,
    /// Tokens used for this operation.
    pub tokens_used: u64,
    /// Tokens saved compared to full retrieval.
    pub tokens_saved: u64,
    /// Whether the result was served from cache.
    pub cache_hit: bool,
    /// The extraction intent used.
    pub intent: ExtractionIntent,
    /// Size of the source data (before scoping).
    pub source_size: u64,
    /// Size of the result data (after scoping).
    pub result_size: u64,
}

impl AuditEntry {
    /// Create a new audit entry with the current timestamp.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tool: impl Into<String>,
        layer: Layer,
        tokens_used: u64,
        tokens_saved: u64,
        cache_hit: bool,
        intent: ExtractionIntent,
        source_size: u64,
        result_size: u64,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            tool: tool.into(),
            layer,
            tokens_used,
            tokens_saved,
            cache_hit,
            intent,
            source_size,
            result_size,
        }
    }

    /// The conservation ratio for this entry (0.0 to 1.0).
    pub fn conservation_ratio(&self) -> f64 {
        let total = self.tokens_used + self.tokens_saved;
        if total == 0 {
            0.0
        } else {
            self.tokens_saved as f64 / total as f64
        }
    }
}

/// A thread-safe audit log of token usage entries.
pub struct AuditLog {
    entries: Mutex<Vec<AuditEntry>>,
    max_entries: usize,
}

impl AuditLog {
    /// Create a new audit log with the given maximum entry count.
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            max_entries,
        }
    }

    /// Record an audit entry.
    pub fn record(&self, entry: AuditEntry) {
        let mut entries = self.entries.lock().unwrap();
        entries.push(entry);
        if entries.len() > self.max_entries {
            let drain_count = entries.len() - self.max_entries;
            entries.drain(..drain_count);
        }
    }

    /// Get a snapshot of all entries.
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.entries.lock().unwrap().clone()
    }

    /// Number of recorded entries.
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    /// Whether the log is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Calculate the cache hit rate across all entries.
    pub fn cache_hit_rate(&self) -> f64 {
        let entries = self.entries.lock().unwrap();
        if entries.is_empty() {
            return 0.0;
        }
        let hits = entries.iter().filter(|e| e.cache_hit).count() as f64;
        hits / entries.len() as f64
    }

    /// Calculate the distribution of queries across layers.
    /// Returns a vector of (Layer, count) pairs.
    pub fn layer_distribution(&self) -> Vec<(Layer, usize)> {
        let entries = self.entries.lock().unwrap();
        let mut counts = std::collections::HashMap::new();
        for entry in entries.iter() {
            *counts.entry(entry.layer).or_insert(0usize) += 1;
        }
        let mut result: Vec<(Layer, usize)> = counts.into_iter().collect();
        result.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        result
    }

    /// Total tokens used across all entries.
    pub fn total_tokens_used(&self) -> u64 {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.tokens_used)
            .sum()
    }

    /// Total tokens saved across all entries.
    pub fn total_tokens_saved(&self) -> u64 {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.tokens_saved)
            .sum()
    }

    /// Overall conservation ratio.
    pub fn conservation_ratio(&self) -> f64 {
        let used = self.total_tokens_used() as f64;
        let saved = self.total_tokens_saved() as f64;
        let total = used + saved;
        if total == 0.0 {
            0.0
        } else {
            saved / total
        }
    }

    /// Clear all entries.
    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new(10_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(tool: &str, layer: Layer, used: u64, saved: u64, cache_hit: bool) -> AuditEntry {
        AuditEntry::new(
            tool,
            layer,
            used,
            saved,
            cache_hit,
            ExtractionIntent::IdsOnly,
            100,
            10,
        )
    }

    #[test]
    fn test_record_entry() {
        let log = AuditLog::new(100);
        log.record(make_entry("test", Layer::Cache, 0, 100, true));
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn test_cache_hit_rate() {
        let log = AuditLog::new(100);
        log.record(make_entry("t1", Layer::Cache, 0, 100, true));
        log.record(make_entry("t2", Layer::Full, 100, 0, false));
        assert!((log.cache_hit_rate() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cache_hit_rate_empty() {
        let log = AuditLog::new(100);
        assert_eq!(log.cache_hit_rate(), 0.0);
    }

    #[test]
    fn test_layer_distribution() {
        let log = AuditLog::new(100);
        log.record(make_entry("t1", Layer::Cache, 0, 100, true));
        log.record(make_entry("t2", Layer::Cache, 0, 100, true));
        log.record(make_entry("t3", Layer::Full, 100, 0, false));
        let dist = log.layer_distribution();
        assert_eq!(dist[0].0, Layer::Cache);
        assert_eq!(dist[0].1, 2);
    }

    #[test]
    fn test_total_tokens() {
        let log = AuditLog::new(100);
        log.record(make_entry("t1", Layer::Scoped, 10, 90, false));
        log.record(make_entry("t2", Layer::Full, 100, 0, false));
        assert_eq!(log.total_tokens_used(), 110);
        assert_eq!(log.total_tokens_saved(), 90);
    }

    #[test]
    fn test_conservation_ratio() {
        let log = AuditLog::new(100);
        log.record(make_entry("t1", Layer::Scoped, 10, 90, false));
        assert!((log.conservation_ratio() - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_max_entries() {
        let log = AuditLog::new(3);
        for i in 0..5 {
            log.record(make_entry(&format!("t{}", i), Layer::Full, 100, 0, false));
        }
        assert_eq!(log.len(), 3);
    }

    #[test]
    fn test_clear() {
        let log = AuditLog::new(100);
        log.record(make_entry("t1", Layer::Full, 100, 0, false));
        log.clear();
        assert!(log.is_empty());
    }

    #[test]
    fn test_audit_entry_conservation_ratio() {
        let entry = make_entry("t1", Layer::Scoped, 10, 90, false);
        assert!((entry.conservation_ratio() - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_entries_snapshot() {
        let log = AuditLog::new(100);
        log.record(make_entry("t1", Layer::Cache, 0, 100, true));
        let entries = log.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tool, "t1");
    }
}
