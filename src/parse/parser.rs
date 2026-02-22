//! Main parser orchestrator.
//!
//! Delegates to language-specific parsers based on file extension.
//! Collects files via the `ignore` crate (respects .gitignore),
//! runs tree-sitter, and calls language extractors.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::types::{AcbError, AcbResult, Language};

use super::go::GoParser;
use super::python::PythonParser;
use super::rust::RustParser;
use super::treesitter::parse_with_language;
use super::typescript::TypeScriptParser;
use super::{LanguageParser, ParseFileError, RawCodeUnit, Severity};

/// Options controlling what and how to parse.
#[derive(Debug, Clone)]
pub struct ParseOptions {
    /// Languages to include (empty = all supported).
    pub languages: Vec<Language>,
    /// Glob patterns to exclude.
    pub exclude: Vec<String>,
    /// Include test files.
    pub include_tests: bool,
    /// Maximum file size to parse (bytes).
    pub max_file_size: usize,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            languages: vec![],
            exclude: vec![
                "**/node_modules/**".into(),
                "**/target/**".into(),
                "**/.git/**".into(),
                "**/__pycache__/**".into(),
                "**/venv/**".into(),
                "**/.venv/**".into(),
                "**/dist/**".into(),
                "**/build/**".into(),
            ],
            include_tests: true,
            max_file_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// Result of parsing a directory or set of files.
#[derive(Debug)]
pub struct ParseResult {
    /// All extracted code units.
    pub units: Vec<RawCodeUnit>,
    /// Errors and warnings encountered.
    pub errors: Vec<ParseFileError>,
    /// Aggregate statistics.
    pub stats: ParseStats,
}

/// Aggregate statistics from a parse run.
#[derive(Debug, Clone)]
pub struct ParseStats {
    /// Number of files successfully parsed.
    pub files_parsed: usize,
    /// Number of files skipped (excluded, too large, unknown lang).
    pub files_skipped: usize,
    /// Number of files that errored during parsing.
    pub files_errored: usize,
    /// Total source lines across all parsed files.
    pub total_lines: usize,
    /// Total parse time in milliseconds.
    pub parse_time_ms: u64,
    /// Files parsed per language.
    pub by_language: HashMap<Language, usize>,
    /// Detailed ingestion/skip accounting for auditability.
    pub coverage: ParseCoverageStats,
}

/// Detailed counters for ingestion fidelity and skip reasons.
#[derive(Debug, Clone, Default)]
pub struct ParseCoverageStats {
    /// Number of filesystem files seen by the walker.
    pub files_seen: usize,
    /// Number of files that made it into parser candidates.
    pub files_candidate: usize,
    /// Files skipped because language could not be resolved.
    pub skipped_unknown_language: usize,
    /// Files skipped by an explicit language filter.
    pub skipped_language_filter: usize,
    /// Files skipped by configured exclude patterns.
    pub skipped_excluded_pattern: usize,
    /// Files skipped because they exceeded size limits.
    pub skipped_too_large: usize,
    /// Files skipped because test files were disabled.
    pub skipped_test_file: usize,
    /// Files that failed to read from disk.
    pub read_errors: usize,
    /// Files that failed during parser/extractor execution.
    pub parse_errors: usize,
}

impl ParseCoverageStats {
    /// Total number of files skipped for known reasons.
    pub fn total_skipped(&self) -> usize {
        self.skipped_unknown_language
            + self.skipped_language_filter
            + self.skipped_excluded_pattern
            + self.skipped_too_large
            + self.skipped_test_file
    }
}

struct CollectFilesResult {
    files: Vec<PathBuf>,
    coverage: ParseCoverageStats,
}

/// Main parser that orchestrates multi-language parsing.
pub struct Parser {
    /// Language-specific parsers, keyed by Language.
    parsers: HashMap<Language, Box<dyn LanguageParser>>,
}

impl Parser {
    /// Create a new parser with all supported language parsers.
    pub fn new() -> Self {
        let mut parsers: HashMap<Language, Box<dyn LanguageParser>> = HashMap::new();
        parsers.insert(Language::Python, Box::new(PythonParser::new()));
        parsers.insert(Language::Rust, Box::new(RustParser::new()));
        parsers.insert(Language::TypeScript, Box::new(TypeScriptParser::new()));
        parsers.insert(Language::JavaScript, Box::new(TypeScriptParser::new()));
        parsers.insert(Language::Go, Box::new(GoParser::new()));
        Self { parsers }
    }

    /// Parse a single file given its path and content.
    pub fn parse_file(&self, path: &Path, content: &str) -> AcbResult<Vec<RawCodeUnit>> {
        let lang = Language::from_path(path);
        if lang == Language::Unknown {
            return Err(AcbError::ParseError {
                path: path.to_path_buf(),
                message: "Unknown language".into(),
            });
        }

        let parser = self
            .parsers
            .get(&lang)
            .ok_or_else(|| AcbError::ParseError {
                path: path.to_path_buf(),
                message: format!("No parser for language: {}", lang),
            })?;

        // For TSX files, use the TSX language
        let ts_lang = if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("tsx") | Some("jsx")
        ) {
            tree_sitter_typescript::language_tsx()
        } else {
            lang.tree_sitter_language()
                .ok_or_else(|| AcbError::ParseError {
                    path: path.to_path_buf(),
                    message: format!("No tree-sitter grammar for: {}", lang),
                })?
        };

        let tree = parse_with_language(content, ts_lang)?;
        parser.extract_units(&tree, content, path)
    }

    /// Parse all matching files in a directory tree.
    pub fn parse_directory(&self, root: &Path, options: &ParseOptions) -> AcbResult<ParseResult> {
        let start = Instant::now();

        let collected = self.collect_files(root, options)?;
        let files = collected.files;

        let mut all_units = Vec::new();
        let mut all_errors = Vec::new();
        let mut files_parsed = 0usize;
        let mut files_errored = 0usize;
        let mut total_lines = 0usize;
        let mut by_language: HashMap<Language, usize> = HashMap::new();
        let mut coverage = collected.coverage;

        for file_path in &files {
            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(e) => {
                    all_errors.push(ParseFileError {
                        path: file_path.clone(),
                        span: None,
                        message: format!("Could not read file: {}", e),
                        severity: Severity::Error,
                    });
                    files_errored += 1;
                    coverage.read_errors += 1;
                    continue;
                }
            };

            // Check file size
            if content.len() > options.max_file_size {
                coverage.skipped_too_large += 1;
                continue;
            }

            let lang = Language::from_path(file_path);
            if lang == Language::Unknown {
                coverage.skipped_unknown_language += 1;
                continue;
            }

            // Check test file filtering
            if !options.include_tests {
                if let Some(parser) = self.parsers.get(&lang) {
                    if parser.is_test_file(file_path, &content) {
                        coverage.skipped_test_file += 1;
                        continue;
                    }
                }
            }

            match self.parse_file(file_path, &content) {
                Ok(units) => {
                    total_lines += content.lines().count();
                    *by_language.entry(lang).or_insert(0) += 1;
                    all_units.extend(units);
                    files_parsed += 1;
                }
                Err(e) => {
                    all_errors.push(ParseFileError {
                        path: file_path.clone(),
                        span: None,
                        message: format!("{}", e),
                        severity: Severity::Error,
                    });
                    files_errored += 1;
                    coverage.parse_errors += 1;
                }
            }
        }

        let elapsed = start.elapsed();
        let files_skipped = coverage.total_skipped();

        Ok(ParseResult {
            units: all_units,
            errors: all_errors,
            stats: ParseStats {
                files_parsed,
                files_skipped,
                files_errored,
                total_lines,
                parse_time_ms: elapsed.as_millis() as u64,
                by_language,
                coverage,
            },
        })
    }

    /// Returns true if a file should be parsed based on language filters.
    pub fn should_parse(&self, path: &Path) -> bool {
        let lang = Language::from_path(path);
        lang != Language::Unknown && self.parsers.contains_key(&lang)
    }

    /// Collect files to parse from a directory tree using the `ignore` crate.
    fn collect_files(&self, root: &Path, options: &ParseOptions) -> AcbResult<CollectFilesResult> {
        use ignore::WalkBuilder;

        let mut files = Vec::new();
        let mut coverage = ParseCoverageStats::default();

        let walker = WalkBuilder::new(root).hidden(true).git_ignore(true).build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();

            if !path.is_file() {
                continue;
            }
            coverage.files_seen += 1;

            let lang = Language::from_path(path);
            if lang == Language::Unknown {
                coverage.skipped_unknown_language += 1;
                continue;
            }

            // Check language filter
            if !options.languages.is_empty() && !options.languages.contains(&lang) {
                coverage.skipped_language_filter += 1;
                continue;
            }

            // Check exclude patterns
            if self.is_excluded(path, &options.exclude) {
                coverage.skipped_excluded_pattern += 1;
                continue;
            }

            files.push(path.to_path_buf());
        }
        coverage.files_candidate = files.len();

        Ok(CollectFilesResult { files, coverage })
    }

    /// Check if a path matches any exclude patterns.
    fn is_excluded(&self, path: &Path, excludes: &[String]) -> bool {
        let path_str = path.to_string_lossy();
        for pattern in excludes {
            // Simple glob matching: check if any component matches
            let pattern_str = pattern.replace("**", "");
            let pattern_str = pattern_str.trim_matches('/');
            if !pattern_str.is_empty() && path_str.contains(pattern_str) {
                return true;
            }
        }
        false
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}
