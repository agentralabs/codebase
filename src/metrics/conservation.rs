use serde::{Deserialize, Serialize};

use super::audit::AuditLog;
use super::tokens::TokenMetrics;

/// Verdict on the overall conservation performance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConservationVerdict {
    /// Conservation score >= 0.9
    Excellent,
    /// Conservation score >= 0.7
    Good,
    /// Conservation score >= 0.5
    Fair,
    /// Conservation score >= 0.3
    Poor,
    /// Conservation score < 0.3
    Wasteful,
}

impl ConservationVerdict {
    /// Determine the verdict from a conservation score (0.0 to 1.0).
    pub fn from_score(score: f64) -> Self {
        if score >= 0.9 {
            ConservationVerdict::Excellent
        } else if score >= 0.7 {
            ConservationVerdict::Good
        } else if score >= 0.5 {
            ConservationVerdict::Fair
        } else if score >= 0.3 {
            ConservationVerdict::Poor
        } else {
            ConservationVerdict::Wasteful
        }
    }

    /// Whether this verdict meets the minimum target (Good or better).
    pub fn meets_target(&self) -> bool {
        matches!(
            self,
            ConservationVerdict::Excellent | ConservationVerdict::Good
        )
    }
}

/// A comprehensive conservation performance report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConservationReport {
    /// Overall conservation score (0.0 to 1.0).
    pub score: f64,
    /// The verdict based on the score.
    pub verdict: ConservationVerdict,
    /// Total tokens used.
    pub total_tokens_used: u64,
    /// Total tokens saved.
    pub total_tokens_saved: u64,
    /// Total queries processed.
    pub total_queries: u64,
    /// Cache hit rate (0.0 to 1.0).
    pub cache_hit_rate: f64,
    /// Average tokens per query.
    pub avg_tokens_per_query: f64,
    /// Layer distribution as (layer_name, percentage) pairs.
    pub layer_distribution: Vec<(String, f64)>,
    /// Recommendations for improving conservation.
    pub recommendations: Vec<String>,
}

/// Generate a conservation report from token metrics and audit log.
pub fn generate_report(metrics: &TokenMetrics, audit_log: &AuditLog) -> ConservationReport {
    let score = metrics.conservation_score();
    let verdict = ConservationVerdict::from_score(score);
    let total_used = metrics.total_tokens_used();
    let total_saved = metrics.total_tokens_saved();
    let total_queries = metrics.total_queries();
    let cache_hit_rate = audit_log.cache_hit_rate();

    let avg_tokens_per_query = if total_queries > 0 {
        total_used as f64 / total_queries as f64
    } else {
        0.0
    };

    let layer_dist = audit_log.layer_distribution();
    let total_entries = audit_log.len() as f64;
    let layer_distribution: Vec<(String, f64)> = if total_entries > 0.0 {
        layer_dist
            .into_iter()
            .map(|(layer, count)| (format!("{:?}", layer), count as f64 / total_entries))
            .collect()
    } else {
        Vec::new()
    };

    let mut recommendations = Vec::new();
    if cache_hit_rate < 0.5 {
        recommendations.push("Increase cache usage to reduce redundant queries".to_string());
    }
    if score < 0.7 {
        recommendations.push("Use IdsOnly or Summary intents instead of Full".to_string());
    }
    if score < 0.5 {
        recommendations
            .push("Enable delta queries to avoid re-fetching unchanged data".to_string());
    }
    if avg_tokens_per_query > 100.0 {
        recommendations
            .push("Average tokens per query is high; consider scoped extraction".to_string());
    }

    ConservationReport {
        score,
        verdict,
        total_tokens_used: total_used,
        total_tokens_saved: total_saved,
        total_queries,
        cache_hit_rate,
        avg_tokens_per_query,
        layer_distribution,
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::audit::AuditEntry;
    use crate::metrics::tokens::Layer;
    use crate::query::ExtractionIntent;

    fn make_entry(layer: Layer, used: u64, saved: u64, cache_hit: bool) -> AuditEntry {
        AuditEntry::new(
            "test_tool",
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
    fn test_verdict_excellent() {
        assert_eq!(
            ConservationVerdict::from_score(0.95),
            ConservationVerdict::Excellent
        );
    }

    #[test]
    fn test_verdict_good() {
        assert_eq!(
            ConservationVerdict::from_score(0.75),
            ConservationVerdict::Good
        );
    }

    #[test]
    fn test_verdict_fair() {
        assert_eq!(
            ConservationVerdict::from_score(0.55),
            ConservationVerdict::Fair
        );
    }

    #[test]
    fn test_verdict_poor() {
        assert_eq!(
            ConservationVerdict::from_score(0.35),
            ConservationVerdict::Poor
        );
    }

    #[test]
    fn test_verdict_wasteful() {
        assert_eq!(
            ConservationVerdict::from_score(0.1),
            ConservationVerdict::Wasteful
        );
    }

    #[test]
    fn test_meets_target() {
        assert!(ConservationVerdict::Excellent.meets_target());
        assert!(ConservationVerdict::Good.meets_target());
        assert!(!ConservationVerdict::Fair.meets_target());
        assert!(!ConservationVerdict::Poor.meets_target());
        assert!(!ConservationVerdict::Wasteful.meets_target());
    }

    #[test]
    fn test_generate_report_empty() {
        let metrics = TokenMetrics::new();
        let log = AuditLog::new(100);
        let report = generate_report(&metrics, &log);
        assert_eq!(report.score, 0.0);
        assert_eq!(report.verdict, ConservationVerdict::Wasteful);
        assert_eq!(report.total_queries, 0);
    }

    #[test]
    fn test_generate_report_good_conservation() {
        let metrics = TokenMetrics::new();
        let log = AuditLog::new(100);

        // Simulate: 9 cache hits (0 tokens each, 100 saved) + 1 full (100 tokens, 0 saved)
        for _ in 0..9 {
            metrics.record(Layer::Cache, 0);
            metrics.record_savings(100);
            log.record(make_entry(Layer::Cache, 0, 100, true));
        }
        metrics.record(Layer::Full, 100);
        log.record(make_entry(Layer::Full, 100, 0, false));

        let report = generate_report(&metrics, &log);
        assert!(report.score >= 0.9);
        assert_eq!(report.verdict, ConservationVerdict::Excellent);
        assert!((report.cache_hit_rate - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_generate_report_recommendations() {
        let metrics = TokenMetrics::new();
        let log = AuditLog::new(100);

        // All full queries — poor conservation
        for _ in 0..5 {
            metrics.record(Layer::Full, 200);
            log.record(make_entry(Layer::Full, 200, 0, false));
        }

        let report = generate_report(&metrics, &log);
        assert!(!report.recommendations.is_empty());
    }

    #[test]
    fn test_serialization() {
        let verdict = ConservationVerdict::Good;
        let json = serde_json::to_string(&verdict).unwrap();
        let back: ConservationVerdict = serde_json::from_str(&json).unwrap();
        assert_eq!(verdict, back);
    }
}
