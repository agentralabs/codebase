//! Time-based analysis for code evolution.
//!
//! Change history tracking, stability scores, coupling detection,
//! and predictive analysis. Can work with or without git integration.

pub mod archaeology;
pub mod coupling;
pub mod history;
pub mod prophecy;
pub mod prophecy_v2;
pub mod stability;

pub use archaeology::{
    ArchaeologyResult, CodeArchaeologist, CodeEvolution, EvolutionPhase, HistoricalChangeType,
    HistoricalDecision, TimelineEvent,
};
pub use coupling::{Coupling, CouplingDetector, CouplingOptions, CouplingType};
pub use history::{ChangeHistory, ChangeType, FileChange, HistoryOptions};
pub use prophecy::{
    AlertType, EcosystemAlert, Prediction, PredictionType, ProphecyEngine, ProphecyOptions,
    ProphecyResult,
};
pub use prophecy_v2::{
    CodeProphecy, EnhancedPrediction, EnhancedProphecyEngine, ProphecyEvidence, ProphecyHorizon,
    ProphecySubject, Sentiment,
};
pub use stability::{
    StabilityAnalyzer, StabilityFactor, StabilityOptions, StabilityRecommendation, StabilityResult,
};
