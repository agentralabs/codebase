//! Translation tracking — monitors the porting status of symbols between
//! a source context and a target context within a workspace.
//!
//! A [`TranslationMap`] records which source symbols have been ported, which
//! are in progress, and which remain untouched. The [`TranslationProgress`]
//! summary provides at-a-glance metrics for migration dashboards.

// ---------------------------------------------------------------------------
// TranslationStatus
// ---------------------------------------------------------------------------

/// The porting status of a single source symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationStatus {
    /// No work has begun on this symbol.
    NotStarted,
    /// Porting is underway but not yet complete.
    InProgress,
    /// The symbol has been ported to the target context.
    Ported,
    /// The ported symbol has been reviewed and verified.
    Verified,
    /// The symbol was intentionally excluded from porting.
    Skipped,
}

impl TranslationStatus {
    /// Parse a status from a string (case-insensitive).
    ///
    /// Accepts both hyphenated (`"not-started"`, `"in-progress"`) and
    /// underscore (`"not_started"`, `"in_progress"`) variants.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "not_started" => Some(Self::NotStarted),
            "in_progress" => Some(Self::InProgress),
            "ported" => Some(Self::Ported),
            "verified" => Some(Self::Verified),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }

    /// A human-readable label for this status.
    pub fn label(&self) -> &str {
        match self {
            Self::NotStarted => "not_started",
            Self::InProgress => "in_progress",
            Self::Ported => "ported",
            Self::Verified => "verified",
            Self::Skipped => "skipped",
        }
    }

    /// Whether this status counts toward "complete" progress.
    fn is_complete(&self) -> bool {
        matches!(self, Self::Ported | Self::Verified | Self::Skipped)
    }
}

impl std::fmt::Display for TranslationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// ---------------------------------------------------------------------------
// TranslationMapping
// ---------------------------------------------------------------------------

/// A mapping from a single source symbol to its target counterpart.
#[derive(Debug)]
pub struct TranslationMapping {
    /// Name of the symbol in the source context.
    pub source_symbol: String,
    /// Name of the corresponding symbol in the target context, if one exists.
    pub target_symbol: Option<String>,
    /// Current porting status.
    pub status: TranslationStatus,
    /// Free-form notes (e.g., "needs manual review", "API changed").
    pub notes: Option<String>,
}

// ---------------------------------------------------------------------------
// TranslationProgress
// ---------------------------------------------------------------------------

/// Summary statistics for a [`TranslationMap`].
#[derive(Debug, Clone)]
pub struct TranslationProgress {
    /// Total number of tracked source symbols.
    pub total: usize,
    /// Symbols with [`TranslationStatus::NotStarted`].
    pub not_started: usize,
    /// Symbols with [`TranslationStatus::InProgress`].
    pub in_progress: usize,
    /// Symbols with [`TranslationStatus::Ported`].
    pub ported: usize,
    /// Symbols with [`TranslationStatus::Verified`].
    pub verified: usize,
    /// Symbols with [`TranslationStatus::Skipped`].
    pub skipped: usize,
    /// Percentage of symbols considered complete:
    /// `(ported + verified + skipped) / total * 100.0`.
    /// Returns `0.0` when there are no mappings.
    pub percent_complete: f32,
}

// ---------------------------------------------------------------------------
// TranslationMap
// ---------------------------------------------------------------------------

/// Tracks the porting status of symbols from one context to another.
///
/// Symbols are keyed by their source name. Calling [`record`](Self::record)
/// with a source name that already exists will update the existing mapping
/// rather than creating a duplicate.
#[derive(Debug)]
pub struct TranslationMap {
    /// Context ID of the source codebase.
    pub source_context: String,
    /// Context ID of the target codebase.
    pub target_context: String,
    /// Ordered list of symbol mappings.
    mappings: Vec<TranslationMapping>,
}

impl TranslationMap {
    /// Create an empty translation map between two contexts.
    pub fn new(source_context: String, target_context: String) -> Self {
        Self {
            source_context,
            target_context,
            mappings: Vec::new(),
        }
    }

    /// Record or update a translation mapping.
    ///
    /// If a mapping for `source` already exists it is updated in place;
    /// otherwise a new entry is appended.
    pub fn record(
        &mut self,
        source: &str,
        target: Option<&str>,
        status: TranslationStatus,
        notes: Option<String>,
    ) {
        // Update in place if the source symbol already exists.
        if let Some(existing) = self.mappings.iter_mut().find(|m| m.source_symbol == source) {
            existing.target_symbol = target.map(|s| s.to_string());
            existing.status = status;
            existing.notes = notes;
            return;
        }

        self.mappings.push(TranslationMapping {
            source_symbol: source.to_string(),
            target_symbol: target.map(|s| s.to_string()),
            status,
            notes,
        });
    }

    /// Look up the current mapping for a source symbol.
    pub fn status(&self, source: &str) -> Option<&TranslationMapping> {
        self.mappings.iter().find(|m| m.source_symbol == source)
    }

    /// Compute aggregate progress across all tracked symbols.
    pub fn progress(&self) -> TranslationProgress {
        let total = self.mappings.len();
        let mut not_started = 0usize;
        let mut in_progress = 0usize;
        let mut ported = 0usize;
        let mut verified = 0usize;
        let mut skipped = 0usize;

        for m in &self.mappings {
            match m.status {
                TranslationStatus::NotStarted => not_started += 1,
                TranslationStatus::InProgress => in_progress += 1,
                TranslationStatus::Ported => ported += 1,
                TranslationStatus::Verified => verified += 1,
                TranslationStatus::Skipped => skipped += 1,
            }
        }

        let percent_complete = if total > 0 {
            (ported + verified + skipped) as f32 / total as f32 * 100.0
        } else {
            0.0
        };

        TranslationProgress {
            total,
            not_started,
            in_progress,
            ported,
            verified,
            skipped,
            percent_complete,
        }
    }

    /// Return all mappings that still need work ([`NotStarted`](TranslationStatus::NotStarted)
    /// or [`InProgress`](TranslationStatus::InProgress)).
    pub fn remaining(&self) -> Vec<&TranslationMapping> {
        self.mappings
            .iter()
            .filter(|m| !m.status.is_complete())
            .collect()
    }

    /// Return all mappings that are considered complete ([`Ported`](TranslationStatus::Ported),
    /// [`Verified`](TranslationStatus::Verified), or [`Skipped`](TranslationStatus::Skipped)).
    pub fn completed(&self) -> Vec<&TranslationMapping> {
        self.mappings
            .iter()
            .filter(|m| m.status.is_complete())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_status() {
        let mut tm = TranslationMap::new("ctx-1".into(), "ctx-2".into());
        tm.record("foo", Some("foo_rs"), TranslationStatus::Ported, None);

        let m = tm.status("foo").unwrap();
        assert_eq!(m.target_symbol.as_deref(), Some("foo_rs"));
        assert_eq!(m.status, TranslationStatus::Ported);
    }

    #[test]
    fn record_updates_existing() {
        let mut tm = TranslationMap::new("ctx-1".into(), "ctx-2".into());
        tm.record("bar", None, TranslationStatus::NotStarted, None);
        tm.record(
            "bar",
            Some("bar_rs"),
            TranslationStatus::InProgress,
            Some("WIP".into()),
        );

        assert_eq!(tm.mappings.len(), 1, "should update, not duplicate");
        let m = tm.status("bar").unwrap();
        assert_eq!(m.status, TranslationStatus::InProgress);
        assert_eq!(m.notes.as_deref(), Some("WIP"));
    }

    #[test]
    fn progress_calculation() {
        let mut tm = TranslationMap::new("a".into(), "b".into());
        tm.record("s1", None, TranslationStatus::NotStarted, None);
        tm.record("s2", None, TranslationStatus::InProgress, None);
        tm.record("s3", Some("t3"), TranslationStatus::Ported, None);
        tm.record("s4", Some("t4"), TranslationStatus::Verified, None);
        tm.record("s5", None, TranslationStatus::Skipped, None);

        let p = tm.progress();
        assert_eq!(p.total, 5);
        assert_eq!(p.not_started, 1);
        assert_eq!(p.in_progress, 1);
        assert_eq!(p.ported, 1);
        assert_eq!(p.verified, 1);
        assert_eq!(p.skipped, 1);
        // (1 + 1 + 1) / 5 * 100 = 60.0
        assert!((p.percent_complete - 60.0).abs() < 0.01);
    }

    #[test]
    fn progress_empty() {
        let tm = TranslationMap::new("a".into(), "b".into());
        let p = tm.progress();
        assert_eq!(p.total, 0);
        assert!((p.percent_complete - 0.0).abs() < 0.01);
    }

    #[test]
    fn remaining_and_completed() {
        let mut tm = TranslationMap::new("a".into(), "b".into());
        tm.record("s1", None, TranslationStatus::NotStarted, None);
        tm.record("s2", None, TranslationStatus::InProgress, None);
        tm.record("s3", Some("t3"), TranslationStatus::Ported, None);
        tm.record("s4", Some("t4"), TranslationStatus::Verified, None);
        tm.record("s5", None, TranslationStatus::Skipped, None);

        let rem: Vec<_> = tm.remaining().iter().map(|m| m.source_symbol.as_str()).collect();
        assert_eq!(rem, vec!["s1", "s2"]);

        let done: Vec<_> = tm.completed().iter().map(|m| m.source_symbol.as_str()).collect();
        assert_eq!(done, vec!["s3", "s4", "s5"]);
    }

    #[test]
    fn translation_status_roundtrip() {
        for label in &["not_started", "in_progress", "ported", "verified", "skipped"] {
            let status = TranslationStatus::from_str(label).unwrap();
            assert_eq!(status.label(), *label);
        }
        // Also accept hyphenated forms.
        assert_eq!(
            TranslationStatus::from_str("not-started"),
            Some(TranslationStatus::NotStarted)
        );
        assert_eq!(
            TranslationStatus::from_str("in-progress"),
            Some(TranslationStatus::InProgress)
        );
        assert!(TranslationStatus::from_str("bogus").is_none());
    }

    #[test]
    fn status_returns_none_for_unknown() {
        let tm = TranslationMap::new("a".into(), "b".into());
        assert!(tm.status("nonexistent").is_none());
    }
}
