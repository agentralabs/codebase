//! CLI command implementations.
//!
//! Defines the `Cli` struct (clap derive) and a top-level `run` function that
//! dispatches to each subcommand: `compile`, `info`, `query`, `get`.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

use crate::cli::output::{format_size, progress, progress_done, Styled};
use crate::engine::query::{
    CallDirection, CallGraphParams, CouplingParams, DeadCodeParams, DependencyParams,
    HotspotParams, ImpactParams, MatchMode, ProphecyParams, QueryEngine, SimilarityParams,
    StabilityResult, SymbolLookupParams, TestGapParams,
};
use crate::format::{AcbReader, AcbWriter};
use crate::graph::CodeGraph;
use crate::parse::parser::{ParseOptions, Parser as AcbParser};
use crate::semantic::analyzer::{AnalyzeOptions, SemanticAnalyzer};
use crate::types::FileHeader;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

/// AgenticCodebase -- Semantic code compiler for AI agents.
#[derive(Parser)]
#[command(
    name = "acb",
    about = "AgenticCodebase \u{2014} Semantic code compiler for AI agents",
    long_about = "AgenticCodebase compiles multi-language codebases into navigable concept \
                   graphs that AI agents can query. Supports Python, Rust, TypeScript, and Go.\n\n\
                   Quick start:\n\
                   \x20 acb compile ./my-project            # build a graph\n\
                   \x20 acb info my-project.acb             # inspect the graph\n\
                   \x20 acb query my-project.acb symbol --name UserService\n\
                   \x20 acb query my-project.acb impact --unit-id 42\n\n\
                   For AI agent integration, use the companion MCP server: acb-mcp",
    after_help = "Run 'acb <command> --help' for details on a specific command.\n\
                  Set ACB_LOG=debug for verbose tracing. Set NO_COLOR=1 to disable colors.",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Output format: human-readable text or machine-readable JSON.
    #[arg(long, short = 'f', default_value = "text", global = true)]
    pub format: OutputFormat,

    /// Show detailed progress and diagnostic messages.
    #[arg(long, short = 'v', global = true)]
    pub verbose: bool,

    /// Suppress all non-error output.
    #[arg(long, short = 'q', global = true)]
    pub quiet: bool,
}

/// Output format selector.
#[derive(Clone, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text with optional colors.
    Text,
    /// Machine-readable JSON (one object per command).
    Json,
}

/// Top-level subcommands.
#[derive(Subcommand)]
pub enum Command {
    /// Compile a repository into an .acb graph file.
    ///
    /// Recursively scans the source directory, parses all supported languages
    /// (Python, Rust, TypeScript, Go), performs semantic analysis, and writes
    /// a compact binary .acb file for fast querying.
    ///
    /// Examples:
    ///   acb compile ./src
    ///   acb compile ./src -o myapp.acb
    ///   acb compile ./src --exclude="*test*" --exclude="vendor"
    #[command(alias = "build")]
    Compile {
        /// Path to the source directory to compile.
        path: PathBuf,

        /// Output file path (default: <directory-name>.acb in current dir).
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Glob patterns to exclude from parsing (may be repeated).
        #[arg(long, short = 'e')]
        exclude: Vec<String>,

        /// Include test files in the compilation (default: true).
        #[arg(long, default_value_t = true)]
        include_tests: bool,

        /// Write ingestion coverage report JSON to this path.
        #[arg(long)]
        coverage_report: Option<PathBuf>,
    },

    /// Display summary information about an .acb graph file.
    ///
    /// Shows version, unit/edge counts, language breakdown, and file size.
    /// Useful for verifying a compilation was successful.
    ///
    /// Examples:
    ///   acb info project.acb
    ///   acb info project.acb --format json
    #[command(alias = "stat")]
    Info {
        /// Path to the .acb file.
        file: PathBuf,
    },

    /// Run a query against a compiled .acb graph.
    ///
    /// Available query types:
    ///   symbol     Find code units by name (--name required)
    ///   deps       Forward dependencies of a unit (--unit-id required)
    ///   rdeps      Reverse dependencies (who depends on this unit)
    ///   impact     Impact analysis with risk scoring
    ///   calls      Call graph exploration
    ///   similar    Find structurally similar code units
    ///   prophecy   Predict which units are likely to break
    ///   stability  Stability score for a specific unit
    ///   coupling   Detect tightly coupled unit pairs
    ///   test-gap   Identify high-risk units without adequate tests
    ///   hotspots   Detect high-change concentration units
    ///   dead-code  List unreachable or orphaned units
    ///
    /// Examples:
    ///   acb query project.acb symbol --name "UserService"
    ///   acb query project.acb deps --unit-id 42 --depth 5
    ///   acb query project.acb impact --unit-id 42
    ///   acb query project.acb prophecy --limit 10
    #[command(alias = "q")]
    Query {
        /// Path to the .acb file.
        file: PathBuf,

        /// Query type: symbol, deps, rdeps, impact, calls, similar,
        /// prophecy, stability, coupling, test-gap, hotspots, dead-code.
        query_type: String,

        /// Search string for symbol queries.
        #[arg(long, short = 'n')]
        name: Option<String>,

        /// Unit ID for unit-centric queries (deps, impact, calls, etc.).
        #[arg(long, short = 'u')]
        unit_id: Option<u64>,

        /// Maximum traversal depth (default: 3).
        #[arg(long, short = 'd', default_value_t = 3)]
        depth: u32,

        /// Maximum results to return (default: 20).
        #[arg(long, short = 'l', default_value_t = 20)]
        limit: usize,
    },

    /// Get detailed information about a specific code unit by ID.
    ///
    /// Displays all metadata, edges, and relationships for the unit.
    /// Use `acb query ... symbol` first to find the unit ID.
    ///
    /// Examples:
    ///   acb get project.acb 42
    ///   acb get project.acb 42 --format json
    Get {
        /// Path to the .acb file.
        file: PathBuf,

        /// Unit ID to look up.
        unit_id: u64,
    },

    /// Generate shell completion scripts.
    ///
    /// Outputs a completion script for the specified shell to stdout.
    /// Source it in your shell profile for tab completion.
    ///
    /// Examples:
    ///   acb completions bash > ~/.local/share/bash-completion/completions/acb
    ///   acb completions zsh > ~/.zfunc/_acb
    ///   acb completions fish > ~/.config/fish/completions/acb.fish
    Completions {
        /// Shell type (bash, zsh, fish, powershell, elvish).
        shell: Shell,
    },

    /// Summarize graph health (risk, test gaps, hotspots, dead code).
    Health {
        /// Path to the .acb file.
        file: PathBuf,

        /// Maximum items to show per section.
        #[arg(long, short = 'l', default_value_t = 10)]
        limit: usize,
    },

    /// Enforce a CI risk gate for a proposed unit change.
    Gate {
        /// Path to the .acb file.
        file: PathBuf,

        /// Unit ID being changed.
        #[arg(long, short = 'u')]
        unit_id: u64,

        /// Max allowed overall risk score (0.0 - 1.0).
        #[arg(long, default_value_t = 0.60)]
        max_risk: f32,

        /// Traversal depth for impact analysis.
        #[arg(long, short = 'd', default_value_t = 3)]
        depth: u32,

        /// Fail if impacted units without tests are present.
        #[arg(long, default_value_t = true)]
        require_tests: bool,
    },
}

// ---------------------------------------------------------------------------
// Top-level dispatcher
// ---------------------------------------------------------------------------

/// Run the CLI with the parsed arguments.
///
/// Writes output to stdout. Returns an error on failure.
pub fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let command_name = match &cli.command {
        None => "repl",
        Some(Command::Compile { .. }) => "compile",
        Some(Command::Info { .. }) => "info",
        Some(Command::Query { .. }) => "query",
        Some(Command::Get { .. }) => "get",
        Some(Command::Completions { .. }) => "completions",
        Some(Command::Health { .. }) => "health",
        Some(Command::Gate { .. }) => "gate",
    };
    let started = Instant::now();
    let result = match &cli.command {
        // No subcommand → launch interactive REPL
        None => crate::cli::repl::run(),

        Some(Command::Compile {
            path,
            output,
            exclude,
            include_tests,
            coverage_report,
        }) => cmd_compile(
            path,
            output.as_deref(),
            exclude,
            *include_tests,
            coverage_report.as_deref(),
            &cli,
        ),
        Some(Command::Info { file }) => cmd_info(file, &cli),
        Some(Command::Query {
            file,
            query_type,
            name,
            unit_id,
            depth,
            limit,
        }) => cmd_query(
            file,
            query_type,
            name.as_deref(),
            *unit_id,
            *depth,
            *limit,
            &cli,
        ),
        Some(Command::Get { file, unit_id }) => cmd_get(file, *unit_id, &cli),
        Some(Command::Completions { shell }) => {
            let mut cmd = Cli::command();
            clap_complete::generate(*shell, &mut cmd, "acb", &mut std::io::stdout());
            Ok(())
        }
        Some(Command::Health { file, limit }) => cmd_health(file, *limit, &cli),
        Some(Command::Gate {
            file,
            unit_id,
            max_risk,
            depth,
            require_tests,
        }) => cmd_gate(file, *unit_id, *max_risk, *depth, *require_tests, &cli),
    };

    emit_cli_health_ledger(command_name, started.elapsed(), result.is_ok());
    result
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn emit_cli_health_ledger(command: &str, duration: std::time::Duration, ok: bool) {
    let dir = resolve_health_ledger_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("agentic-codebase-cli.json");
    let tmp = dir.join("agentic-codebase-cli.json.tmp");
    let profile = read_env_string("ACB_AUTONOMIC_PROFILE").unwrap_or_else(|| "desktop".to_string());
    let payload = serde_json::json!({
        "project": "AgenticCodebase",
        "surface": "cli",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "status": if ok { "ok" } else { "error" },
        "autonomic": {
            "profile": profile.to_ascii_lowercase(),
            "command": command,
            "duration_ms": duration.as_millis(),
        }
    });
    let Ok(bytes) = serde_json::to_vec_pretty(&payload) else {
        return;
    };
    if std::fs::write(&tmp, bytes).is_err() {
        return;
    }
    let _ = std::fs::rename(&tmp, &path);
}

fn resolve_health_ledger_dir() -> PathBuf {
    if let Some(custom) = read_env_string("ACB_HEALTH_LEDGER_DIR") {
        if !custom.is_empty() {
            return PathBuf::from(custom);
        }
    }
    if let Some(custom) = read_env_string("AGENTRA_HEALTH_LEDGER_DIR") {
        if !custom.is_empty() {
            return PathBuf::from(custom);
        }
    }
    let home = std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".agentra").join("health-ledger")
}

/// Get the styled output helper, respecting --format json (always plain).
fn styled(cli: &Cli) -> Styled {
    match cli.format {
        OutputFormat::Json => Styled::plain(),
        OutputFormat::Text => Styled::auto(),
    }
}

/// Validate that the path points to an existing file with .acb extension.
fn validate_acb_path(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let s = Styled::auto();
    if !path.exists() {
        return Err(format!(
            "{} File not found: {}\n  {} Check the path and try again",
            s.fail(),
            path.display(),
            s.info()
        )
        .into());
    }
    if !path.is_file() {
        return Err(format!(
            "{} Not a file: {}\n  {} Provide a path to an .acb file, not a directory",
            s.fail(),
            path.display(),
            s.info()
        )
        .into());
    }
    if path.extension().and_then(|e| e.to_str()) != Some("acb") {
        return Err(format!(
            "{} Expected .acb file, got: {}\n  {} Compile a repository first: acb compile <dir>",
            s.fail(),
            path.display(),
            s.info()
        )
        .into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// compile
// ---------------------------------------------------------------------------

fn cmd_compile(
    path: &Path,
    output: Option<&std::path::Path>,
    exclude: &[String],
    include_tests: bool,
    coverage_report: Option<&Path>,
    cli: &Cli,
) -> Result<(), Box<dyn std::error::Error>> {
    let s = styled(cli);

    if !path.exists() {
        return Err(format!(
            "{} Path does not exist: {}\n  {} Create the directory or check the path",
            s.fail(),
            path.display(),
            s.info()
        )
        .into());
    }
    if !path.is_dir() {
        return Err(format!(
            "{} Path is not a directory: {}\n  {} Provide the root directory of a source repository",
            s.fail(),
            path.display(),
            s.info()
        )
        .into());
    }

    let out_path = match output {
        Some(p) => p.to_path_buf(),
        None => {
            let dir_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "output".to_string());
            PathBuf::from(format!("{}.acb", dir_name))
        }
    };

    // Build parse options.
    let mut opts = ParseOptions {
        include_tests,
        ..ParseOptions::default()
    };
    for pat in exclude {
        opts.exclude.push(pat.clone());
    }

    if !cli.quiet {
        if let OutputFormat::Text = cli.format {
            eprintln!(
                "  {} Compiling {} {} {}",
                s.info(),
                s.bold(&path.display().to_string()),
                s.arrow(),
                s.cyan(&out_path.display().to_string()),
            );
        }
    }

    // 1. Parse
    if cli.verbose {
        eprintln!("  {} Parsing source files...", s.info());
    }
    let parser = AcbParser::new();
    let parse_result = parser.parse_directory(path, &opts)?;

    if !cli.quiet {
        if let OutputFormat::Text = cli.format {
            eprintln!(
                "  {} Parsed {} files ({} units found)",
                s.ok(),
                parse_result.stats.files_parsed,
                parse_result.units.len(),
            );
            let cov = &parse_result.stats.coverage;
            eprintln!(
                "  {} Ingestion seen:{} candidate:{} skipped:{} errored:{}",
                s.info(),
                cov.files_seen,
                cov.files_candidate,
                cov.total_skipped(),
                parse_result.stats.files_errored
            );
            if !parse_result.errors.is_empty() {
                eprintln!(
                    "  {} {} parse errors (use --verbose to see details)",
                    s.warn(),
                    parse_result.errors.len()
                );
            }
        }
    }

    if cli.verbose && !parse_result.errors.is_empty() {
        for err in &parse_result.errors {
            eprintln!("    {} {:?}", s.warn(), err);
        }
    }

    // 2. Semantic analysis
    if cli.verbose {
        eprintln!("  {} Running semantic analysis...", s.info());
    }
    let unit_count = parse_result.units.len();
    progress("Analyzing", 0, unit_count);
    let analyzer = SemanticAnalyzer::new();
    let analyze_opts = AnalyzeOptions::default();
    let graph = analyzer.analyze(parse_result.units, &analyze_opts)?;
    progress("Analyzing", unit_count, unit_count);
    progress_done();

    if cli.verbose {
        eprintln!(
            "  {} Graph built: {} units, {} edges",
            s.ok(),
            graph.unit_count(),
            graph.edge_count()
        );
    }

    // 3. Write .acb
    if cli.verbose {
        eprintln!("  {} Writing binary format...", s.info());
    }
    let backup_path = maybe_backup_existing_output(&out_path)?;
    if cli.verbose {
        if let Some(backup) = &backup_path {
            eprintln!(
                "  {} Backed up previous graph to {}",
                s.info(),
                s.dim(&backup.display().to_string())
            );
        }
    }
    let writer = AcbWriter::with_default_dimension();
    writer.write_to_file(&graph, &out_path)?;

    // Final output
    let file_size = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
    let cov = &parse_result.stats.coverage;
    let coverage_json = serde_json::json!({
        "files_seen": cov.files_seen,
        "files_candidate": cov.files_candidate,
        "files_parsed": parse_result.stats.files_parsed,
        "files_skipped_total": cov.total_skipped(),
        "files_errored_total": parse_result.stats.files_errored,
        "skip_reasons": {
            "unknown_language": cov.skipped_unknown_language,
            "language_filter": cov.skipped_language_filter,
            "exclude_pattern": cov.skipped_excluded_pattern,
            "too_large": cov.skipped_too_large,
            "test_file_filtered": cov.skipped_test_file
        },
        "errors": {
            "read_errors": cov.read_errors,
            "parse_errors": cov.parse_errors
        },
        "parse_time_ms": parse_result.stats.parse_time_ms,
        "by_language": parse_result.stats.by_language,
    });

    if let Some(report_path) = coverage_report {
        if let Some(parent) = report_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let payload = serde_json::json!({
            "status": "ok",
            "source_root": path.display().to_string(),
            "output_graph": out_path.display().to_string(),
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "coverage": coverage_json,
        });
        std::fs::write(report_path, serde_json::to_string_pretty(&payload)? + "\n")?;
    }

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match cli.format {
        OutputFormat::Text => {
            if !cli.quiet {
                let _ = writeln!(out);
                let _ = writeln!(out, "  {} Compiled successfully!", s.ok());
                let _ = writeln!(
                    out,
                    "     Units:     {}",
                    s.bold(&graph.unit_count().to_string())
                );
                let _ = writeln!(
                    out,
                    "     Edges:     {}",
                    s.bold(&graph.edge_count().to_string())
                );
                let _ = writeln!(
                    out,
                    "     Languages: {}",
                    s.bold(&graph.languages().len().to_string())
                );
                let _ = writeln!(out, "     Size:      {}", s.dim(&format_size(file_size)));
                let _ = writeln!(
                    out,
                    "     Coverage:  seen={} candidate={} skipped={} errored={}",
                    cov.files_seen,
                    cov.files_candidate,
                    cov.total_skipped(),
                    parse_result.stats.files_errored
                );
                if let Some(report_path) = coverage_report {
                    let _ = writeln!(
                        out,
                        "     Report:    {}",
                        s.dim(&report_path.display().to_string())
                    );
                }
                let _ = writeln!(out);
                let _ = writeln!(
                    out,
                    "  Next: {} or {}",
                    s.cyan(&format!("acb info {}", out_path.display())),
                    s.cyan(&format!(
                        "acb query {} symbol --name <search>",
                        out_path.display()
                    )),
                );
            }
        }
        OutputFormat::Json => {
            let obj = serde_json::json!({
                "status": "ok",
                "source": path.display().to_string(),
                "output": out_path.display().to_string(),
                "units": graph.unit_count(),
                "edges": graph.edge_count(),
                "languages": graph.languages().len(),
                "file_size_bytes": file_size,
                "coverage": coverage_json,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }

    Ok(())
}

fn maybe_backup_existing_output(
    out_path: &Path,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    if !auto_backup_enabled() || !out_path.exists() || !out_path.is_file() {
        return Ok(None);
    }

    let backups_dir = resolve_backup_dir(out_path);
    std::fs::create_dir_all(&backups_dir)?;

    let original_name = out_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("graph.acb");
    let ts = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let backup_path = backups_dir.join(format!("{original_name}.{ts}.bak"));
    std::fs::copy(out_path, &backup_path)?;
    prune_old_backups(&backups_dir, original_name, auto_backup_retention())?;

    Ok(Some(backup_path))
}

fn auto_backup_enabled() -> bool {
    match std::env::var("ACB_AUTO_BACKUP") {
        Ok(v) => {
            let value = v.trim().to_ascii_lowercase();
            value != "0" && value != "false" && value != "off" && value != "no"
        }
        Err(_) => true,
    }
}

fn auto_backup_retention() -> usize {
    let default_retention = match read_env_string("ACB_AUTONOMIC_PROFILE")
        .unwrap_or_else(|| "desktop".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "cloud" => 40,
        "aggressive" => 12,
        _ => 20,
    };
    std::env::var("ACB_AUTO_BACKUP_RETENTION")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default_retention)
        .max(1)
}

fn resolve_backup_dir(out_path: &Path) -> PathBuf {
    if let Ok(custom) = std::env::var("ACB_AUTO_BACKUP_DIR") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    out_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".acb-backups")
}

fn read_env_string(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_string())
}

fn prune_old_backups(
    backup_dir: &Path,
    original_name: &str,
    retention: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut backups = std::fs::read_dir(backup_dir)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|name| name.starts_with(original_name) && name.ends_with(".bak"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    if backups.len() <= retention {
        return Ok(());
    }

    backups.sort_by_key(|entry| {
        entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });

    let to_remove = backups.len().saturating_sub(retention);
    for entry in backups.into_iter().take(to_remove) {
        let _ = std::fs::remove_file(entry.path());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// info
// ---------------------------------------------------------------------------

fn cmd_info(file: &PathBuf, cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let s = styled(cli);
    validate_acb_path(file)?;
    let graph = AcbReader::read_from_file(file)?;

    // Also read header for metadata.
    let data = std::fs::read(file)?;
    let header_bytes: [u8; 128] = data[..128]
        .try_into()
        .map_err(|_| "File too small for header")?;
    let header = FileHeader::from_bytes(&header_bytes)?;
    let file_size = data.len() as u64;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match cli.format {
        OutputFormat::Text => {
            let _ = writeln!(
                out,
                "\n  {} {}",
                s.info(),
                s.bold(&file.display().to_string())
            );
            let _ = writeln!(out, "     Version:   v{}", header.version);
            let _ = writeln!(
                out,
                "     Units:     {}",
                s.bold(&graph.unit_count().to_string())
            );
            let _ = writeln!(
                out,
                "     Edges:     {}",
                s.bold(&graph.edge_count().to_string())
            );
            let _ = writeln!(
                out,
                "     Languages: {}",
                s.bold(&graph.languages().len().to_string())
            );
            let _ = writeln!(out, "     Dimension: {}", header.dimension);
            let _ = writeln!(out, "     File size: {}", format_size(file_size));
            let _ = writeln!(out);
            for lang in graph.languages() {
                let count = graph.units().iter().filter(|u| u.language == *lang).count();
                let _ = writeln!(
                    out,
                    "     {} {} {}",
                    s.arrow(),
                    s.cyan(&format!("{:12}", lang)),
                    s.dim(&format!("{} units", count))
                );
            }
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let mut lang_map = serde_json::Map::new();
            for lang in graph.languages() {
                let count = graph.units().iter().filter(|u| u.language == *lang).count();
                lang_map.insert(lang.to_string(), serde_json::json!(count));
            }
            let obj = serde_json::json!({
                "file": file.display().to_string(),
                "version": header.version,
                "units": graph.unit_count(),
                "edges": graph.edge_count(),
                "languages": graph.languages().len(),
                "dimension": header.dimension,
                "file_size_bytes": file_size,
                "language_breakdown": lang_map,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }

    Ok(())
}

fn cmd_health(file: &Path, limit: usize, cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    validate_acb_path(file)?;
    let graph = AcbReader::read_from_file(file)?;
    let engine = QueryEngine::new();
    let s = styled(cli);

    let prophecy = engine.prophecy(
        &graph,
        ProphecyParams {
            top_k: limit,
            min_risk: 0.45,
        },
    )?;
    let test_gaps = engine.test_gap(
        &graph,
        TestGapParams {
            min_changes: 5,
            min_complexity: 10,
            unit_types: vec![],
        },
    )?;
    let hotspots = engine.hotspot_detection(
        &graph,
        HotspotParams {
            top_k: limit,
            min_score: 0.55,
            unit_types: vec![],
        },
    )?;
    let dead_code = engine.dead_code(
        &graph,
        DeadCodeParams {
            unit_types: vec![],
            include_tests_as_roots: true,
        },
    )?;

    let high_risk = prophecy
        .predictions
        .iter()
        .filter(|p| p.risk_score >= 0.70)
        .count();
    let avg_risk = if prophecy.predictions.is_empty() {
        0.0
    } else {
        prophecy
            .predictions
            .iter()
            .map(|p| p.risk_score)
            .sum::<f32>()
            / prophecy.predictions.len() as f32
    };
    let status = if high_risk >= 3 || test_gaps.len() >= 8 {
        "fail"
    } else if high_risk > 0 || !test_gaps.is_empty() || !hotspots.is_empty() {
        "warn"
    } else {
        "pass"
    };

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match cli.format {
        OutputFormat::Text => {
            let status_label = match status {
                "pass" => s.green("PASS"),
                "warn" => s.yellow("WARN"),
                _ => s.red("FAIL"),
            };
            let _ = writeln!(
                out,
                "\n  Graph health for {} [{}]\n",
                s.bold(&file.display().to_string()),
                status_label
            );
            let _ = writeln!(out, "  Units:      {}", graph.unit_count());
            let _ = writeln!(out, "  Edges:      {}", graph.edge_count());
            let _ = writeln!(out, "  Avg risk:   {:.2}", avg_risk);
            let _ = writeln!(out, "  High risk:  {}", high_risk);
            let _ = writeln!(out, "  Test gaps:  {}", test_gaps.len());
            let _ = writeln!(out, "  Hotspots:   {}", hotspots.len());
            let _ = writeln!(out, "  Dead code:  {}", dead_code.len());
            let _ = writeln!(out);

            if !prophecy.predictions.is_empty() {
                let _ = writeln!(out, "  Top risk predictions:");
                for p in prophecy.predictions.iter().take(5) {
                    let name = graph
                        .get_unit(p.unit_id)
                        .map(|u| u.qualified_name.clone())
                        .unwrap_or_else(|| format!("unit_{}", p.unit_id));
                    let _ = writeln!(out, "    {} {:.2} {}", s.arrow(), p.risk_score, name);
                }
                let _ = writeln!(out);
            }

            if !test_gaps.is_empty() {
                let _ = writeln!(out, "  Top test gaps:");
                for g in test_gaps.iter().take(5) {
                    let name = graph
                        .get_unit(g.unit_id)
                        .map(|u| u.qualified_name.clone())
                        .unwrap_or_else(|| format!("unit_{}", g.unit_id));
                    let _ = writeln!(
                        out,
                        "    {} {:.2} {} ({})",
                        s.arrow(),
                        g.priority,
                        name,
                        g.reason
                    );
                }
                let _ = writeln!(out);
            }

            let _ = writeln!(
                out,
                "  Next: acb gate {} --unit-id <id> --max-risk 0.60",
                file.display()
            );
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let predictions = prophecy
                .predictions
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "unit_id": p.unit_id,
                        "name": graph.get_unit(p.unit_id).map(|u| u.qualified_name.clone()).unwrap_or_default(),
                        "risk_score": p.risk_score,
                        "reason": p.reason,
                    })
                })
                .collect::<Vec<_>>();
            let gaps = test_gaps
                .iter()
                .map(|g| {
                    serde_json::json!({
                        "unit_id": g.unit_id,
                        "name": graph.get_unit(g.unit_id).map(|u| u.qualified_name.clone()).unwrap_or_default(),
                        "priority": g.priority,
                        "reason": g.reason,
                    })
                })
                .collect::<Vec<_>>();
            let hotspot_rows = hotspots
                .iter()
                .map(|h| {
                    serde_json::json!({
                        "unit_id": h.unit_id,
                        "name": graph.get_unit(h.unit_id).map(|u| u.qualified_name.clone()).unwrap_or_default(),
                        "score": h.score,
                        "factors": h.factors,
                    })
                })
                .collect::<Vec<_>>();
            let dead_rows = dead_code
                .iter()
                .map(|u| {
                    serde_json::json!({
                        "unit_id": u.id,
                        "name": u.qualified_name,
                        "type": u.unit_type.label(),
                    })
                })
                .collect::<Vec<_>>();

            let obj = serde_json::json!({
                "status": status,
                "graph": file.display().to_string(),
                "summary": {
                    "units": graph.unit_count(),
                    "edges": graph.edge_count(),
                    "avg_risk": avg_risk,
                    "high_risk_count": high_risk,
                    "test_gap_count": test_gaps.len(),
                    "hotspot_count": hotspots.len(),
                    "dead_code_count": dead_code.len(),
                },
                "risk_predictions": predictions,
                "test_gaps": gaps,
                "hotspots": hotspot_rows,
                "dead_code": dead_rows,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }

    Ok(())
}

fn cmd_gate(
    file: &Path,
    unit_id: u64,
    max_risk: f32,
    depth: u32,
    require_tests: bool,
    cli: &Cli,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_acb_path(file)?;
    let graph = AcbReader::read_from_file(file)?;
    let engine = QueryEngine::new();
    let s = styled(cli);

    let result = engine.impact_analysis(
        &graph,
        ImpactParams {
            unit_id,
            max_depth: depth,
            edge_types: vec![],
        },
    )?;
    let untested_count = result.impacted.iter().filter(|u| !u.has_tests).count();
    let risk_pass = result.overall_risk <= max_risk;
    let test_pass = !require_tests || untested_count == 0;
    let passed = risk_pass && test_pass;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match cli.format {
        OutputFormat::Text => {
            let label = if passed {
                s.green("PASS")
            } else {
                s.red("FAIL")
            };
            let unit_name = graph
                .get_unit(unit_id)
                .map(|u| u.qualified_name.clone())
                .unwrap_or_else(|| format!("unit_{}", unit_id));
            let _ = writeln!(out, "\n  Gate {} for {}\n", label, s.bold(&unit_name));
            let _ = writeln!(
                out,
                "  Overall risk:  {:.2} (max {:.2})",
                result.overall_risk, max_risk
            );
            let _ = writeln!(out, "  Impacted:      {}", result.impacted.len());
            let _ = writeln!(out, "  Untested:      {}", untested_count);
            let _ = writeln!(out, "  Require tests: {}", require_tests);
            if !result.recommendations.is_empty() {
                let _ = writeln!(out);
                for rec in &result.recommendations {
                    let _ = writeln!(out, "  {} {}", s.info(), rec);
                }
            }
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let obj = serde_json::json!({
                "gate": if passed { "pass" } else { "fail" },
                "file": file.display().to_string(),
                "unit_id": unit_id,
                "max_risk": max_risk,
                "overall_risk": result.overall_risk,
                "impacted_count": result.impacted.len(),
                "untested_count": untested_count,
                "require_tests": require_tests,
                "recommendations": result.recommendations,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }

    if !passed {
        return Err(format!(
            "{} gate failed: risk_pass={} test_pass={} (risk {:.2} / max {:.2}, untested {})",
            s.fail(),
            risk_pass,
            test_pass,
            result.overall_risk,
            max_risk,
            untested_count
        )
        .into());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// query
// ---------------------------------------------------------------------------

fn cmd_query(
    file: &Path,
    query_type: &str,
    name: Option<&str>,
    unit_id: Option<u64>,
    depth: u32,
    limit: usize,
    cli: &Cli,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_acb_path(file)?;
    let graph = AcbReader::read_from_file(file)?;
    let engine = QueryEngine::new();
    let s = styled(cli);

    match query_type {
        "symbol" | "sym" | "s" => query_symbol(&graph, &engine, name, limit, cli, &s),
        "deps" | "dep" | "d" => query_deps(&graph, &engine, unit_id, depth, cli, &s),
        "rdeps" | "rdep" | "r" => query_rdeps(&graph, &engine, unit_id, depth, cli, &s),
        "impact" | "imp" | "i" => query_impact(&graph, &engine, unit_id, depth, cli, &s),
        "calls" | "call" | "c" => query_calls(&graph, &engine, unit_id, depth, cli, &s),
        "similar" | "sim" => query_similar(&graph, &engine, unit_id, limit, cli, &s),
        "prophecy" | "predict" | "p" => query_prophecy(&graph, &engine, limit, cli, &s),
        "stability" | "stab" => query_stability(&graph, &engine, unit_id, cli, &s),
        "coupling" | "couple" => query_coupling(&graph, &engine, unit_id, cli, &s),
        "test-gap" | "testgap" | "gaps" => query_test_gap(&graph, &engine, limit, cli, &s),
        "hotspot" | "hotspots" => query_hotspots(&graph, &engine, limit, cli, &s),
        "dead" | "dead-code" | "deadcode" => query_dead_code(&graph, &engine, limit, cli, &s),
        other => {
            let known = [
                "symbol",
                "deps",
                "rdeps",
                "impact",
                "calls",
                "similar",
                "prophecy",
                "stability",
                "coupling",
                "test-gap",
                "hotspots",
                "dead-code",
            ];
            let suggestion = known
                .iter()
                .filter(|k| k.starts_with(&other[..1.min(other.len())]))
                .copied()
                .collect::<Vec<_>>();
            let hint = if suggestion.is_empty() {
                format!("Available: {}", known.join(", "))
            } else {
                format!("Did you mean: {}?", suggestion.join(", "))
            };
            Err(format!(
                "{} Unknown query type: {}\n  {} {}",
                s.fail(),
                other,
                s.info(),
                hint
            )
            .into())
        }
    }
}

fn query_symbol(
    graph: &CodeGraph,
    engine: &QueryEngine,
    name: Option<&str>,
    limit: usize,
    cli: &Cli,
    s: &Styled,
) -> Result<(), Box<dyn std::error::Error>> {
    let search_name = name.ok_or_else(|| {
        format!(
            "{} --name is required for symbol queries\n  {} Example: acb query file.acb symbol --name UserService",
            s.fail(),
            s.info()
        )
    })?;
    let params = SymbolLookupParams {
        name: search_name.to_string(),
        mode: MatchMode::Contains,
        limit,
        ..Default::default()
    };
    let results = engine.symbol_lookup(graph, params)?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match cli.format {
        OutputFormat::Text => {
            let _ = writeln!(
                out,
                "\n  Symbol lookup: {} ({} results)\n",
                s.bold(&format!("\"{}\"", search_name)),
                results.len()
            );
            if results.is_empty() {
                let _ = writeln!(
                    out,
                    "  {} No matches found. Try a broader search term.",
                    s.warn()
                );
            }
            for (i, unit) in results.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "  {:>3}. {} {} {}",
                    s.dim(&format!("#{}", i + 1)),
                    s.bold(&unit.qualified_name),
                    s.dim(&format!("({})", unit.unit_type)),
                    s.dim(&format!(
                        "{}:{}",
                        unit.file_path.display(),
                        unit.span.start_line
                    ))
                );
            }
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let entries: Vec<serde_json::Value> = results
                .iter()
                .map(|u| {
                    serde_json::json!({
                        "id": u.id,
                        "name": u.name,
                        "qualified_name": u.qualified_name,
                        "unit_type": u.unit_type.label(),
                        "language": u.language.name(),
                        "file": u.file_path.display().to_string(),
                        "line": u.span.start_line,
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "query": "symbol",
                "name": search_name,
                "count": results.len(),
                "results": entries,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }
    Ok(())
}

fn query_deps(
    graph: &CodeGraph,
    engine: &QueryEngine,
    unit_id: Option<u64>,
    depth: u32,
    cli: &Cli,
    s: &Styled,
) -> Result<(), Box<dyn std::error::Error>> {
    let uid = unit_id.ok_or_else(|| {
        format!(
            "{} --unit-id is required for deps queries\n  {} Find an ID first: acb query file.acb symbol --name <name>",
            s.fail(), s.info()
        )
    })?;
    let params = DependencyParams {
        unit_id: uid,
        max_depth: depth,
        edge_types: vec![],
        include_transitive: true,
    };
    let result = engine.dependency_graph(graph, params)?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match cli.format {
        OutputFormat::Text => {
            let root_name = graph
                .get_unit(uid)
                .map(|u| u.qualified_name.as_str())
                .unwrap_or("?");
            let _ = writeln!(
                out,
                "\n  Dependencies of {} ({} found)\n",
                s.bold(root_name),
                result.nodes.len()
            );
            for node in &result.nodes {
                let unit_name = graph
                    .get_unit(node.unit_id)
                    .map(|u| u.qualified_name.as_str())
                    .unwrap_or("?");
                let indent = "  ".repeat(node.depth as usize);
                let _ = writeln!(
                    out,
                    "  {}{} {} {}",
                    indent,
                    s.arrow(),
                    s.cyan(unit_name),
                    s.dim(&format!("[id:{}]", node.unit_id))
                );
            }
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let entries: Vec<serde_json::Value> = result
                .nodes
                .iter()
                .map(|n| {
                    let unit_name = graph
                        .get_unit(n.unit_id)
                        .map(|u| u.qualified_name.clone())
                        .unwrap_or_default();
                    serde_json::json!({
                        "unit_id": n.unit_id,
                        "name": unit_name,
                        "depth": n.depth,
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "query": "deps",
                "root_id": uid,
                "count": result.nodes.len(),
                "results": entries,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }
    Ok(())
}

fn query_rdeps(
    graph: &CodeGraph,
    engine: &QueryEngine,
    unit_id: Option<u64>,
    depth: u32,
    cli: &Cli,
    s: &Styled,
) -> Result<(), Box<dyn std::error::Error>> {
    let uid = unit_id.ok_or_else(|| {
        format!(
            "{} --unit-id is required for rdeps queries\n  {} Find an ID first: acb query file.acb symbol --name <name>",
            s.fail(), s.info()
        )
    })?;
    let params = DependencyParams {
        unit_id: uid,
        max_depth: depth,
        edge_types: vec![],
        include_transitive: true,
    };
    let result = engine.reverse_dependency(graph, params)?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match cli.format {
        OutputFormat::Text => {
            let root_name = graph
                .get_unit(uid)
                .map(|u| u.qualified_name.as_str())
                .unwrap_or("?");
            let _ = writeln!(
                out,
                "\n  Reverse dependencies of {} ({} found)\n",
                s.bold(root_name),
                result.nodes.len()
            );
            for node in &result.nodes {
                let unit_name = graph
                    .get_unit(node.unit_id)
                    .map(|u| u.qualified_name.as_str())
                    .unwrap_or("?");
                let indent = "  ".repeat(node.depth as usize);
                let _ = writeln!(
                    out,
                    "  {}{} {} {}",
                    indent,
                    s.arrow(),
                    s.cyan(unit_name),
                    s.dim(&format!("[id:{}]", node.unit_id))
                );
            }
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let entries: Vec<serde_json::Value> = result
                .nodes
                .iter()
                .map(|n| {
                    let unit_name = graph
                        .get_unit(n.unit_id)
                        .map(|u| u.qualified_name.clone())
                        .unwrap_or_default();
                    serde_json::json!({
                        "unit_id": n.unit_id,
                        "name": unit_name,
                        "depth": n.depth,
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "query": "rdeps",
                "root_id": uid,
                "count": result.nodes.len(),
                "results": entries,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }
    Ok(())
}

fn query_impact(
    graph: &CodeGraph,
    engine: &QueryEngine,
    unit_id: Option<u64>,
    depth: u32,
    cli: &Cli,
    s: &Styled,
) -> Result<(), Box<dyn std::error::Error>> {
    let uid =
        unit_id.ok_or_else(|| format!("{} --unit-id is required for impact queries", s.fail()))?;
    let params = ImpactParams {
        unit_id: uid,
        max_depth: depth,
        edge_types: vec![],
    };
    let result = engine.impact_analysis(graph, params)?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match cli.format {
        OutputFormat::Text => {
            let root_name = graph
                .get_unit(uid)
                .map(|u| u.qualified_name.as_str())
                .unwrap_or("?");

            let risk_label = if result.overall_risk >= 0.7 {
                s.red("HIGH")
            } else if result.overall_risk >= 0.4 {
                s.yellow("MEDIUM")
            } else {
                s.green("LOW")
            };

            let _ = writeln!(
                out,
                "\n  Impact analysis for {} (risk: {})\n",
                s.bold(root_name),
                risk_label,
            );
            let _ = writeln!(
                out,
                "  {} impacted units, overall risk {:.2}\n",
                result.impacted.len(),
                result.overall_risk
            );
            for imp in &result.impacted {
                let unit_name = graph
                    .get_unit(imp.unit_id)
                    .map(|u| u.qualified_name.as_str())
                    .unwrap_or("?");
                let risk_sym = if imp.risk_score >= 0.7 {
                    s.fail()
                } else if imp.risk_score >= 0.4 {
                    s.warn()
                } else {
                    s.ok()
                };
                let test_badge = if imp.has_tests {
                    s.green("tested")
                } else {
                    s.red("untested")
                };
                let _ = writeln!(
                    out,
                    "  {} {} {} risk:{:.2} {}",
                    risk_sym,
                    s.cyan(unit_name),
                    s.dim(&format!("(depth {})", imp.depth)),
                    imp.risk_score,
                    test_badge,
                );
            }
            if !result.recommendations.is_empty() {
                let _ = writeln!(out);
                for rec in &result.recommendations {
                    let _ = writeln!(out, "  {} {}", s.info(), rec);
                }
            }
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let entries: Vec<serde_json::Value> = result
                .impacted
                .iter()
                .map(|imp| {
                    serde_json::json!({
                        "unit_id": imp.unit_id,
                        "depth": imp.depth,
                        "risk_score": imp.risk_score,
                        "has_tests": imp.has_tests,
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "query": "impact",
                "root_id": uid,
                "count": result.impacted.len(),
                "overall_risk": result.overall_risk,
                "results": entries,
                "recommendations": result.recommendations,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }
    Ok(())
}

fn query_calls(
    graph: &CodeGraph,
    engine: &QueryEngine,
    unit_id: Option<u64>,
    depth: u32,
    cli: &Cli,
    s: &Styled,
) -> Result<(), Box<dyn std::error::Error>> {
    let uid =
        unit_id.ok_or_else(|| format!("{} --unit-id is required for calls queries", s.fail()))?;
    let params = CallGraphParams {
        unit_id: uid,
        direction: CallDirection::Both,
        max_depth: depth,
    };
    let result = engine.call_graph(graph, params)?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match cli.format {
        OutputFormat::Text => {
            let root_name = graph
                .get_unit(uid)
                .map(|u| u.qualified_name.as_str())
                .unwrap_or("?");
            let _ = writeln!(
                out,
                "\n  Call graph for {} ({} nodes)\n",
                s.bold(root_name),
                result.nodes.len()
            );
            for (nid, d) in &result.nodes {
                let unit_name = graph
                    .get_unit(*nid)
                    .map(|u| u.qualified_name.as_str())
                    .unwrap_or("?");
                let indent = "  ".repeat(*d as usize);
                let _ = writeln!(out, "  {}{} {}", indent, s.arrow(), s.cyan(unit_name),);
            }
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let entries: Vec<serde_json::Value> = result
                .nodes
                .iter()
                .map(|(nid, d)| {
                    let unit_name = graph
                        .get_unit(*nid)
                        .map(|u| u.qualified_name.clone())
                        .unwrap_or_default();
                    serde_json::json!({
                        "unit_id": nid,
                        "name": unit_name,
                        "depth": d,
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "query": "calls",
                "root_id": uid,
                "count": result.nodes.len(),
                "results": entries,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }
    Ok(())
}

fn query_similar(
    graph: &CodeGraph,
    engine: &QueryEngine,
    unit_id: Option<u64>,
    limit: usize,
    cli: &Cli,
    s: &Styled,
) -> Result<(), Box<dyn std::error::Error>> {
    let uid =
        unit_id.ok_or_else(|| format!("{} --unit-id is required for similar queries", s.fail()))?;
    let params = SimilarityParams {
        unit_id: uid,
        top_k: limit,
        min_similarity: 0.0,
    };
    let results = engine.similarity(graph, params)?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match cli.format {
        OutputFormat::Text => {
            let root_name = graph
                .get_unit(uid)
                .map(|u| u.qualified_name.as_str())
                .unwrap_or("?");
            let _ = writeln!(
                out,
                "\n  Similar to {} ({} matches)\n",
                s.bold(root_name),
                results.len()
            );
            for (i, m) in results.iter().enumerate() {
                let unit_name = graph
                    .get_unit(m.unit_id)
                    .map(|u| u.qualified_name.as_str())
                    .unwrap_or("?");
                let score_str = format!("{:.2}%", m.score * 100.0);
                let _ = writeln!(
                    out,
                    "  {:>3}. {} {} {}",
                    s.dim(&format!("#{}", i + 1)),
                    s.cyan(unit_name),
                    s.dim(&format!("[id:{}]", m.unit_id)),
                    s.yellow(&score_str),
                );
            }
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let entries: Vec<serde_json::Value> = results
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "unit_id": m.unit_id,
                        "score": m.score,
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "query": "similar",
                "root_id": uid,
                "count": results.len(),
                "results": entries,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }
    Ok(())
}

fn query_prophecy(
    graph: &CodeGraph,
    engine: &QueryEngine,
    limit: usize,
    cli: &Cli,
    s: &Styled,
) -> Result<(), Box<dyn std::error::Error>> {
    let params = ProphecyParams {
        top_k: limit,
        min_risk: 0.0,
    };
    let result = engine.prophecy(graph, params)?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match cli.format {
        OutputFormat::Text => {
            let _ = writeln!(
                out,
                "\n  {} Code prophecy ({} predictions)\n",
                s.info(),
                result.predictions.len()
            );
            if result.predictions.is_empty() {
                let _ = writeln!(
                    out,
                    "  {} No high-risk predictions. Codebase looks stable!",
                    s.ok()
                );
            }
            for pred in &result.predictions {
                let unit_name = graph
                    .get_unit(pred.unit_id)
                    .map(|u| u.qualified_name.as_str())
                    .unwrap_or("?");
                let risk_sym = if pred.risk_score >= 0.7 {
                    s.fail()
                } else if pred.risk_score >= 0.4 {
                    s.warn()
                } else {
                    s.ok()
                };
                let _ = writeln!(
                    out,
                    "  {} {} {}: {}",
                    risk_sym,
                    s.cyan(unit_name),
                    s.dim(&format!("(risk {:.2})", pred.risk_score)),
                    pred.reason,
                );
            }
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let entries: Vec<serde_json::Value> = result
                .predictions
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "unit_id": p.unit_id,
                        "risk_score": p.risk_score,
                        "reason": p.reason,
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "query": "prophecy",
                "count": result.predictions.len(),
                "results": entries,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }
    Ok(())
}

fn query_stability(
    graph: &CodeGraph,
    engine: &QueryEngine,
    unit_id: Option<u64>,
    cli: &Cli,
    s: &Styled,
) -> Result<(), Box<dyn std::error::Error>> {
    let uid = unit_id
        .ok_or_else(|| format!("{} --unit-id is required for stability queries", s.fail()))?;
    let result: StabilityResult = engine.stability_analysis(graph, uid)?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match cli.format {
        OutputFormat::Text => {
            let root_name = graph
                .get_unit(uid)
                .map(|u| u.qualified_name.as_str())
                .unwrap_or("?");

            let score_color = if result.overall_score >= 0.7 {
                s.green(&format!("{:.2}", result.overall_score))
            } else if result.overall_score >= 0.4 {
                s.yellow(&format!("{:.2}", result.overall_score))
            } else {
                s.red(&format!("{:.2}", result.overall_score))
            };

            let _ = writeln!(
                out,
                "\n  Stability of {}: {}\n",
                s.bold(root_name),
                score_color,
            );
            for factor in &result.factors {
                let _ = writeln!(
                    out,
                    "  {} {} = {:.2}: {}",
                    s.arrow(),
                    s.bold(&factor.name),
                    factor.value,
                    s.dim(&factor.description),
                );
            }
            let _ = writeln!(out, "\n  {} {}", s.info(), result.recommendation);
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let factors: Vec<serde_json::Value> = result
                .factors
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "name": f.name,
                        "value": f.value,
                        "description": f.description,
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "query": "stability",
                "unit_id": uid,
                "overall_score": result.overall_score,
                "factors": factors,
                "recommendation": result.recommendation,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }
    Ok(())
}

fn query_coupling(
    graph: &CodeGraph,
    engine: &QueryEngine,
    unit_id: Option<u64>,
    cli: &Cli,
    s: &Styled,
) -> Result<(), Box<dyn std::error::Error>> {
    let params = CouplingParams {
        unit_id,
        min_strength: 0.0,
    };
    let results = engine.coupling_detection(graph, params)?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match cli.format {
        OutputFormat::Text => {
            let _ = writeln!(
                out,
                "\n  Coupling analysis ({} pairs detected)\n",
                results.len()
            );
            if results.is_empty() {
                let _ = writeln!(out, "  {} No tightly coupled pairs detected.", s.ok());
            }
            for c in &results {
                let name_a = graph
                    .get_unit(c.unit_a)
                    .map(|u| u.qualified_name.as_str())
                    .unwrap_or("?");
                let name_b = graph
                    .get_unit(c.unit_b)
                    .map(|u| u.qualified_name.as_str())
                    .unwrap_or("?");
                let strength_str = format!("{:.0}%", c.strength * 100.0);
                let _ = writeln!(
                    out,
                    "  {} {} {} {} {}",
                    s.warn(),
                    s.cyan(name_a),
                    s.dim("<->"),
                    s.cyan(name_b),
                    s.yellow(&strength_str),
                );
            }
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let entries: Vec<serde_json::Value> = results
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "unit_a": c.unit_a,
                        "unit_b": c.unit_b,
                        "strength": c.strength,
                        "kind": format!("{:?}", c.kind),
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "query": "coupling",
                "count": results.len(),
                "results": entries,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }
    Ok(())
}

fn query_test_gap(
    graph: &CodeGraph,
    engine: &QueryEngine,
    limit: usize,
    cli: &Cli,
    s: &Styled,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut gaps = engine.test_gap(
        graph,
        TestGapParams {
            min_changes: 5,
            min_complexity: 10,
            unit_types: vec![],
        },
    )?;
    if limit > 0 {
        gaps.truncate(limit);
    }

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match cli.format {
        OutputFormat::Text => {
            let _ = writeln!(out, "\n  Test gaps ({} results)\n", gaps.len());
            for g in &gaps {
                let name = graph
                    .get_unit(g.unit_id)
                    .map(|u| u.qualified_name.as_str())
                    .unwrap_or("?");
                let _ = writeln!(
                    out,
                    "  {} {} priority:{:.2} {}",
                    s.arrow(),
                    s.cyan(name),
                    g.priority,
                    s.dim(&g.reason)
                );
            }
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let rows = gaps
                .iter()
                .map(|g| {
                    serde_json::json!({
                        "unit_id": g.unit_id,
                        "name": graph.get_unit(g.unit_id).map(|u| u.qualified_name.clone()).unwrap_or_default(),
                        "priority": g.priority,
                        "reason": g.reason,
                    })
                })
                .collect::<Vec<_>>();
            let obj = serde_json::json!({
                "query": "test-gap",
                "count": rows.len(),
                "results": rows,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }
    Ok(())
}

fn query_hotspots(
    graph: &CodeGraph,
    engine: &QueryEngine,
    limit: usize,
    cli: &Cli,
    s: &Styled,
) -> Result<(), Box<dyn std::error::Error>> {
    let hotspots = engine.hotspot_detection(
        graph,
        HotspotParams {
            top_k: limit,
            min_score: 0.55,
            unit_types: vec![],
        },
    )?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match cli.format {
        OutputFormat::Text => {
            let _ = writeln!(out, "\n  Hotspots ({} results)\n", hotspots.len());
            for h in &hotspots {
                let name = graph
                    .get_unit(h.unit_id)
                    .map(|u| u.qualified_name.as_str())
                    .unwrap_or("?");
                let _ = writeln!(out, "  {} {} score:{:.2}", s.arrow(), s.cyan(name), h.score);
            }
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let rows = hotspots
                .iter()
                .map(|h| {
                    serde_json::json!({
                        "unit_id": h.unit_id,
                        "name": graph.get_unit(h.unit_id).map(|u| u.qualified_name.clone()).unwrap_or_default(),
                        "score": h.score,
                        "factors": h.factors,
                    })
                })
                .collect::<Vec<_>>();
            let obj = serde_json::json!({
                "query": "hotspots",
                "count": rows.len(),
                "results": rows,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }
    Ok(())
}

fn query_dead_code(
    graph: &CodeGraph,
    engine: &QueryEngine,
    limit: usize,
    cli: &Cli,
    s: &Styled,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut dead = engine.dead_code(
        graph,
        DeadCodeParams {
            unit_types: vec![],
            include_tests_as_roots: true,
        },
    )?;
    if limit > 0 {
        dead.truncate(limit);
    }

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match cli.format {
        OutputFormat::Text => {
            let _ = writeln!(out, "\n  Dead code ({} results)\n", dead.len());
            for unit in &dead {
                let _ = writeln!(
                    out,
                    "  {} {} {}",
                    s.arrow(),
                    s.cyan(&unit.qualified_name),
                    s.dim(&format!("({})", unit.unit_type.label()))
                );
            }
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let rows = dead
                .iter()
                .map(|u| {
                    serde_json::json!({
                        "unit_id": u.id,
                        "name": u.qualified_name,
                        "unit_type": u.unit_type.label(),
                        "file": u.file_path.display().to_string(),
                        "line": u.span.start_line,
                    })
                })
                .collect::<Vec<_>>();
            let obj = serde_json::json!({
                "query": "dead-code",
                "count": rows.len(),
                "results": rows,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// get
// ---------------------------------------------------------------------------

fn cmd_get(file: &Path, unit_id: u64, cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let s = styled(cli);
    validate_acb_path(file)?;
    let graph = AcbReader::read_from_file(file)?;

    let unit = graph.get_unit(unit_id).ok_or_else(|| {
        format!(
            "{} Unit {} not found\n  {} Use 'acb query ... symbol' to find valid unit IDs",
            s.fail(),
            unit_id,
            s.info()
        )
    })?;

    let outgoing = graph.edges_from(unit_id);
    let incoming = graph.edges_to(unit_id);

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match cli.format {
        OutputFormat::Text => {
            let _ = writeln!(
                out,
                "\n  {} {}",
                s.info(),
                s.bold(&format!("Unit {}", unit.id))
            );
            let _ = writeln!(out, "     Name:           {}", s.cyan(&unit.name));
            let _ = writeln!(out, "     Qualified name: {}", s.bold(&unit.qualified_name));
            let _ = writeln!(out, "     Type:           {}", unit.unit_type);
            let _ = writeln!(out, "     Language:       {}", unit.language);
            let _ = writeln!(
                out,
                "     File:           {}",
                s.cyan(&unit.file_path.display().to_string())
            );
            let _ = writeln!(out, "     Span:           {}", unit.span);
            let _ = writeln!(out, "     Visibility:     {}", unit.visibility);
            let _ = writeln!(out, "     Complexity:     {}", unit.complexity);
            if unit.is_async {
                let _ = writeln!(out, "     Async:          {}", s.green("yes"));
            }
            if unit.is_generator {
                let _ = writeln!(out, "     Generator:      {}", s.green("yes"));
            }

            let stability_str = format!("{:.2}", unit.stability_score);
            let stability_color = if unit.stability_score >= 0.7 {
                s.green(&stability_str)
            } else if unit.stability_score >= 0.4 {
                s.yellow(&stability_str)
            } else {
                s.red(&stability_str)
            };
            let _ = writeln!(out, "     Stability:      {}", stability_color);

            if let Some(sig) = &unit.signature {
                let _ = writeln!(out, "     Signature:      {}", s.dim(sig));
            }
            if let Some(doc) = &unit.doc_summary {
                let _ = writeln!(out, "     Doc:            {}", s.dim(doc));
            }

            if !outgoing.is_empty() {
                let _ = writeln!(
                    out,
                    "\n     {} Outgoing edges ({})",
                    s.arrow(),
                    outgoing.len()
                );
                for edge in &outgoing {
                    let target_name = graph
                        .get_unit(edge.target_id)
                        .map(|u| u.qualified_name.as_str())
                        .unwrap_or("?");
                    let _ = writeln!(
                        out,
                        "       {} {} {}",
                        s.arrow(),
                        s.cyan(target_name),
                        s.dim(&format!("({})", edge.edge_type))
                    );
                }
            }
            if !incoming.is_empty() {
                let _ = writeln!(
                    out,
                    "\n     {} Incoming edges ({})",
                    s.arrow(),
                    incoming.len()
                );
                for edge in &incoming {
                    let source_name = graph
                        .get_unit(edge.source_id)
                        .map(|u| u.qualified_name.as_str())
                        .unwrap_or("?");
                    let _ = writeln!(
                        out,
                        "       {} {} {}",
                        s.arrow(),
                        s.cyan(source_name),
                        s.dim(&format!("({})", edge.edge_type))
                    );
                }
            }
            let _ = writeln!(out);
        }
        OutputFormat::Json => {
            let out_edges: Vec<serde_json::Value> = outgoing
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "target_id": e.target_id,
                        "edge_type": e.edge_type.label(),
                        "weight": e.weight,
                    })
                })
                .collect();
            let in_edges: Vec<serde_json::Value> = incoming
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "source_id": e.source_id,
                        "edge_type": e.edge_type.label(),
                        "weight": e.weight,
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "id": unit.id,
                "name": unit.name,
                "qualified_name": unit.qualified_name,
                "unit_type": unit.unit_type.label(),
                "language": unit.language.name(),
                "file": unit.file_path.display().to_string(),
                "span": unit.span.to_string(),
                "visibility": unit.visibility.to_string(),
                "complexity": unit.complexity,
                "is_async": unit.is_async,
                "is_generator": unit.is_generator,
                "stability_score": unit.stability_score,
                "signature": unit.signature,
                "doc_summary": unit.doc_summary,
                "outgoing_edges": out_edges,
                "incoming_edges": in_edges,
            });
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&obj)?);
        }
    }

    Ok(())
}
