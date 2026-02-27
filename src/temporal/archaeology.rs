//! Version Archaeology — Invention 11.
//!
//! Understand the history and evolution of code units. Why does this code
//! look the way it does? When did it change? What decisions led here?

use serde::{Deserialize, Serialize};

use crate::graph::CodeGraph;
use crate::temporal::history::ChangeHistory;

// ── Types ────────────────────────────────────────────────────────────────────

/// Historical change type categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoricalChangeType {
    /// Initial creation of the code.
    Creation,
    /// Bug fix.
    BugFix,
    /// Feature addition.
    Feature,
    /// Refactoring / cleanup.
    Refactor,
    /// Performance optimization.
    Performance,
    /// Unknown / general modification.
    Unknown,
}

impl HistoricalChangeType {
    /// Classify a commit message.
    pub fn classify(message: &str) -> Self {
        let msg = message.to_lowercase();
        if msg.contains("fix")
            || msg.contains("bug")
            || msg.contains("patch")
            || msg.contains("hotfix")
        {
            return Self::BugFix;
        }
        if msg.contains("refactor")
            || msg.contains("cleanup")
            || msg.contains("clean up")
            || msg.contains("rename")
        {
            return Self::Refactor;
        }
        if msg.contains("perf")
            || msg.contains("optim")
            || msg.contains("speed")
            || msg.contains("fast")
        {
            return Self::Performance;
        }
        if msg.contains("feat")
            || msg.contains("add")
            || msg.contains("implement")
            || msg.contains("new")
        {
            return Self::Feature;
        }
        Self::Unknown
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Creation => "creation",
            Self::BugFix => "bugfix",
            Self::Feature => "feature",
            Self::Refactor => "refactor",
            Self::Performance => "performance",
            Self::Unknown => "unknown",
        }
    }
}

/// A historical decision inferred from code changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalDecision {
    /// Description of the decision.
    pub description: String,
    /// When the decision was made (approximate commit timestamp).
    pub timestamp: u64,
    /// Who made the decision.
    pub author: String,
    /// Change type associated with this decision.
    pub change_type: HistoricalChangeType,
    /// Inferred reasoning.
    pub reasoning: String,
}

/// Evolution summary of a code unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEvolution {
    /// Node ID.
    pub node_id: u64,
    /// Node name.
    pub name: String,
    /// File path.
    pub file_path: String,
    /// Total number of changes.
    pub total_changes: usize,
    /// Number of bug fixes.
    pub bugfix_count: usize,
    /// Number of authors who touched this code.
    pub author_count: usize,
    /// Authors.
    pub authors: Vec<String>,
    /// Age in seconds (from first to latest change).
    pub age_seconds: u64,
    /// Churn (total lines added + deleted).
    pub churn: u64,
    /// Stability score (from CodeUnit).
    pub stability_score: f32,
    /// Key decisions.
    pub decisions: Vec<HistoricalDecision>,
    /// Evolution phase.
    pub phase: EvolutionPhase,
}

/// The current phase in a code unit's lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvolutionPhase {
    /// Newly created, still evolving rapidly.
    Active,
    /// Maturing, changes slowing down.
    Maturing,
    /// Stable, rarely changes.
    Stable,
    /// Decaying, mostly bugfixes and patches.
    Decaying,
    /// No history available.
    Unknown,
}

impl EvolutionPhase {
    pub fn label(&self) -> &str {
        match self {
            Self::Active => "active",
            Self::Maturing => "maturing",
            Self::Stable => "stable",
            Self::Decaying => "decaying",
            Self::Unknown => "unknown",
        }
    }
}

/// Archaeological investigation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchaeologyResult {
    /// The code unit investigated.
    pub evolution: CodeEvolution,
    /// "Why" explanation.
    pub why_explanation: String,
    /// "When" timeline.
    pub timeline: Vec<TimelineEvent>,
}

/// A single event in a code unit's timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    /// Timestamp.
    pub timestamp: u64,
    /// Description of what happened.
    pub description: String,
    /// Who did it.
    pub author: String,
    /// Change category.
    pub change_type: HistoricalChangeType,
}

// ── CodeArchaeologist ────────────────────────────────────────────────────────

/// Investigates the history and evolution of code.
pub struct CodeArchaeologist<'g> {
    graph: &'g CodeGraph,
    history: ChangeHistory,
}

impl<'g> CodeArchaeologist<'g> {
    pub fn new(graph: &'g CodeGraph, history: ChangeHistory) -> Self {
        Self { graph, history }
    }

    /// Investigate the full history of a code unit.
    pub fn investigate(&self, unit_id: u64) -> Option<ArchaeologyResult> {
        let unit = self.graph.get_unit(unit_id)?;
        let file_path = unit.file_path.display().to_string();

        let changes = self.history.changes_for_path(&unit.file_path);
        let total_changes = changes.len();
        let bugfix_count = changes.iter().filter(|c| c.is_bugfix).count();
        let authors = self.history.authors_for_path(&unit.file_path);
        let churn = self.history.total_churn(&unit.file_path);

        let oldest = self.history.oldest_timestamp(&unit.file_path);
        let latest = self.history.latest_timestamp(&unit.file_path);
        let age_seconds = if latest > oldest { latest - oldest } else { 0 };

        // Infer evolution phase
        let phase = self.infer_phase(
            unit.stability_score,
            total_changes,
            bugfix_count,
            age_seconds,
        );

        // Build decisions from changes
        let decisions: Vec<HistoricalDecision> = changes
            .iter()
            .map(|c| {
                let change_type = if c.is_bugfix {
                    HistoricalChangeType::BugFix
                } else {
                    HistoricalChangeType::Unknown
                };
                HistoricalDecision {
                    description: format!(
                        "{} {} (+{} -{})",
                        c.change_type, file_path, c.lines_added, c.lines_deleted
                    ),
                    timestamp: c.timestamp,
                    author: c.author.clone(),
                    change_type,
                    reasoning: format!("Change to {} via commit {}", file_path, c.commit_id),
                }
            })
            .collect();

        // Build timeline
        let timeline: Vec<TimelineEvent> = changes
            .iter()
            .map(|c| TimelineEvent {
                timestamp: c.timestamp,
                description: format!(
                    "{} (+{} -{})",
                    c.change_type, c.lines_added, c.lines_deleted
                ),
                author: c.author.clone(),
                change_type: if c.is_bugfix {
                    HistoricalChangeType::BugFix
                } else {
                    HistoricalChangeType::Unknown
                },
            })
            .collect();

        let evolution = CodeEvolution {
            node_id: unit_id,
            name: unit.name.clone(),
            file_path: file_path.clone(),
            total_changes,
            bugfix_count,
            author_count: authors.len(),
            authors: authors.clone(),
            age_seconds,
            churn,
            stability_score: unit.stability_score,
            decisions,
            phase,
        };

        let why_explanation = self.explain_why(&evolution);

        Some(ArchaeologyResult {
            evolution,
            why_explanation,
            timeline,
        })
    }

    /// Answer "why does this code look the way it does?"
    pub fn explain_why(&self, evolution: &CodeEvolution) -> String {
        let mut explanations = Vec::new();

        if evolution.total_changes == 0 {
            return format!(
                "'{}' has no recorded change history. It may be new or history is unavailable.",
                evolution.name
            );
        }

        // Age analysis
        let age_days = evolution.age_seconds / 86400;
        if age_days > 365 {
            explanations.push(format!(
                "This code is {} days old, suggesting it's a mature part of the codebase.",
                age_days
            ));
        } else if age_days < 30 {
            explanations.push("This code is relatively new (< 30 days old).".to_string());
        }

        // Bugfix ratio
        if evolution.total_changes > 0 {
            let bugfix_ratio = evolution.bugfix_count as f64 / evolution.total_changes as f64;
            if bugfix_ratio > 0.5 {
                explanations.push(format!(
                    "High bugfix ratio ({:.0}%) suggests this code has been problematic.",
                    bugfix_ratio * 100.0
                ));
            }
        }

        // Author count
        if evolution.author_count > 3 {
            explanations.push(format!(
                "Modified by {} different authors, indicating shared ownership.",
                evolution.author_count
            ));
        } else if evolution.author_count == 1 {
            explanations.push("Single author — likely has clear ownership.".to_string());
        }

        // Churn
        if evolution.churn > 500 {
            explanations.push(format!(
                "High churn ({} lines changed) suggests significant rework.",
                evolution.churn
            ));
        }

        // Stability
        if evolution.stability_score < 0.3 {
            explanations.push("Low stability score suggests ongoing volatility.".to_string());
        } else if evolution.stability_score > 0.8 {
            explanations.push("High stability score indicates the code has settled.".to_string());
        }

        if explanations.is_empty() {
            format!(
                "'{}' has a typical change history with {} changes.",
                evolution.name, evolution.total_changes
            )
        } else {
            explanations.join(" ")
        }
    }

    /// Answer "when did important changes happen?"
    pub fn when_changed(&self, unit_id: u64) -> Vec<TimelineEvent> {
        let unit = match self.graph.get_unit(unit_id) {
            Some(u) => u,
            None => return Vec::new(),
        };

        self.history
            .changes_for_path(&unit.file_path)
            .iter()
            .map(|c| TimelineEvent {
                timestamp: c.timestamp,
                description: format!(
                    "{} by {} (+{} -{})",
                    c.change_type, c.author, c.lines_added, c.lines_deleted
                ),
                author: c.author.clone(),
                change_type: if c.is_bugfix {
                    HistoricalChangeType::BugFix
                } else {
                    HistoricalChangeType::Unknown
                },
            })
            .collect()
    }

    // ── Internal ─────────────────────────────────────────────────────────

    fn infer_phase(
        &self,
        stability_score: f32,
        total_changes: usize,
        bugfix_count: usize,
        age_seconds: u64,
    ) -> EvolutionPhase {
        if total_changes == 0 {
            return EvolutionPhase::Unknown;
        }

        let age_days = age_seconds / 86400;
        let bugfix_ratio = bugfix_count as f64 / total_changes as f64;

        if stability_score > 0.8 && age_days > 180 {
            EvolutionPhase::Stable
        } else if bugfix_ratio > 0.6 && age_days > 90 {
            EvolutionPhase::Decaying
        } else if age_days < 30 || total_changes > 10 {
            EvolutionPhase::Active
        } else {
            EvolutionPhase::Maturing
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temporal::history::FileChange;
    use crate::types::{CodeUnit, CodeUnitType, Language, Span};
    use std::path::PathBuf;

    fn test_graph_and_history() -> (CodeGraph, ChangeHistory) {
        let mut graph = CodeGraph::with_default_dimension();
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            "process_payment".to_string(),
            "billing::process_payment".to_string(),
            PathBuf::from("src/billing.rs"),
            Span::new(1, 0, 20, 0),
        ));

        let mut history = ChangeHistory::new();
        history.add_change(FileChange {
            path: PathBuf::from("src/billing.rs"),
            change_type: crate::temporal::history::ChangeType::Add,
            commit_id: "abc123".to_string(),
            timestamp: 1000000,
            author: "alice".to_string(),
            is_bugfix: false,
            lines_added: 50,
            lines_deleted: 0,
            old_path: None,
        });
        history.add_change(FileChange {
            path: PathBuf::from("src/billing.rs"),
            change_type: crate::temporal::history::ChangeType::Modify,
            commit_id: "def456".to_string(),
            timestamp: 2000000,
            author: "bob".to_string(),
            is_bugfix: true,
            lines_added: 5,
            lines_deleted: 3,
            old_path: None,
        });

        (graph, history)
    }

    #[test]
    fn investigate_returns_evolution() {
        let (graph, history) = test_graph_and_history();
        let archaeologist = CodeArchaeologist::new(&graph, history);
        let result = archaeologist.investigate(0).unwrap();

        assert_eq!(result.evolution.name, "process_payment");
        assert_eq!(result.evolution.total_changes, 2);
        assert_eq!(result.evolution.bugfix_count, 1);
        assert_eq!(result.evolution.author_count, 2);
    }

    #[test]
    fn when_changed_returns_timeline() {
        let (graph, history) = test_graph_and_history();
        let archaeologist = CodeArchaeologist::new(&graph, history);
        let timeline = archaeologist.when_changed(0);

        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline[0].timestamp, 1000000);
    }

    #[test]
    fn classify_change_type() {
        assert_eq!(
            HistoricalChangeType::classify("fix: null pointer bug"),
            HistoricalChangeType::BugFix
        );
        assert_eq!(
            HistoricalChangeType::classify("refactor: extract method"),
            HistoricalChangeType::Refactor
        );
        assert_eq!(
            HistoricalChangeType::classify("feat: add payment"),
            HistoricalChangeType::Feature
        );
        assert_eq!(
            HistoricalChangeType::classify("optimize query performance"),
            HistoricalChangeType::Performance
        );
    }
}
