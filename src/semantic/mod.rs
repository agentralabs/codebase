//! Semantic analysis layer.
//!
//! Takes parsed syntax and extracts meaning: resolves references across files,
//! traces FFI boundaries, detects patterns. This is where syntax becomes semantics.

pub mod analyzer;
pub mod architecture;
pub mod concept_extractor;
pub mod concept_nav;
pub mod ffi_tracer;
pub mod pattern_detector;
pub mod pattern_extract;
pub mod resolver;

pub use analyzer::{AnalyzeOptions, SemanticAnalyzer};
pub use architecture::{
    ArchitectureAnomaly, ArchitectureComponent, ArchitectureInferrer, ArchitectureLayer,
    ArchitecturePattern, ComponentRole, InferredArchitecture,
};
pub use concept_extractor::{ConceptExtractor, ConceptRole, ExtractedConcept};
pub use concept_nav::{CodeConcept, ConceptNavigator, ConceptQuery};
pub use ffi_tracer::{FfiEdge, FfiPatternType, FfiTracer};
pub use pattern_detector::{PatternDetector, PatternInstance};
pub use pattern_extract::{
    ExtractedPattern, PatternExtractor, PatternViolation, ViolationSeverity,
};
pub use resolver::{
    ExternalSymbol, ImportedSymbol, Resolution, ResolvedReference, ResolvedUnit, Resolver,
    SymbolTable,
};
