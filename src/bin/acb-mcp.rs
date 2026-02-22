//! MCP server entry point for AgenticCodebase.
//!
//! Runs the MCP server over stdin/stdout (default) or HTTP/SSE (with `sse` feature).
//! All logic lives in `agentic_codebase::mcp`.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "acb-mcp",
    about = "AgenticCodebase MCP server -- semantic code intelligence for AI agents",
    version
)]
struct Cli {
    /// Path to a TOML config file.
    #[arg(long, global = true)]
    config: Option<String>,

    /// Path to an .acb graph file to pre-load.
    #[arg(long, global = true)]
    graph: Option<String>,

    /// Name for the pre-loaded graph (defaults to filename stem).
    #[arg(long, global = true)]
    name: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the MCP server on stdin/stdout (default).
    Serve,

    /// Run the MCP server over HTTP/SSE.
    #[cfg(feature = "sse")]
    ServeHttp {
        /// Listen address.
        #[arg(long, default_value = "127.0.0.1:3000")]
        addr: String,

        /// Enable multi-tenant mode (routes by X-User-ID header).
        #[arg(long)]
        multi_tenant: bool,

        /// Data directory for multi-tenant graph files.
        #[arg(long)]
        data_dir: Option<String>,

        /// Bearer token for authentication.
        #[arg(long)]
        token: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    // Load config if provided
    let log_level = if let Some(config_path) = &cli.config {
        match agentic_codebase::config::load_config(config_path) {
            Ok(config) => config.log_level,
            Err(e) => {
                eprintln!("Warning: Failed to load config: {e}");
                "warn".to_string()
            }
        }
    } else {
        "warn".to_string()
    };

    // Initialize tracing (logs to stderr so MCP JSON stays clean on stdout)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&log_level)),
        )
        .with_writer(std::io::stderr)
        .init();

    let graph_path = cli.graph;
    let graph_name = cli.name;

    match cli.command {
        None | Some(Commands::Serve) => {
            run_stdio(graph_path.as_deref(), graph_name);
        }
        #[cfg(feature = "sse")]
        Some(Commands::ServeHttp {
            addr,
            multi_tenant,
            data_dir,
            token,
        }) => {
            run_sse(
                graph_path.as_deref(),
                graph_name,
                &addr,
                multi_tenant,
                data_dir,
                token,
            );
        }
    }
}

/// Run the stdio transport.
fn run_stdio(graph_path: Option<&str>, graph_name: Option<String>) {
    let mut server = agentic_codebase::mcp::McpServer::new();

    // Pre-load graph if specified
    if let Some(graph_path) = graph_path {
        let name = graph_name.unwrap_or_else(|| graph_name_from_path(graph_path));

        match agentic_codebase::AcbReader::read_from_file(std::path::Path::new(graph_path)) {
            Ok(graph) => {
                tracing::info!("Pre-loaded graph '{name}' from {graph_path}");
                server.load_graph(name, graph);
            }
            Err(e) => {
                eprintln!("Error: Failed to load graph: {e}");
                std::process::exit(1);
            }
        }
    }

    // Run stdio server
    use std::io::{BufRead, Write};
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = server.handle_raw(trimmed);
        if response.is_empty() {
            continue;
        }
        if writeln!(stdout, "{}", response).is_err() {
            break;
        }
        if stdout.flush().is_err() {
            break;
        }
    }
}

/// Run the SSE transport.
#[cfg(feature = "sse")]
fn run_sse(
    graph_path: Option<&str>,
    graph_name: Option<String>,
    addr: &str,
    multi_tenant: bool,
    data_dir: Option<String>,
    token: Option<String>,
) {
    use agentic_codebase::mcp::sse::{ServerMode, SseTransport};
    use agentic_codebase::mcp::tenant::TenantRegistry;
    use std::sync::Arc;

    let effective_token = token.or_else(|| std::env::var("AGENTIC_TOKEN").ok());

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    rt.block_on(async {
        let mode = if multi_tenant {
            let dir = data_dir.unwrap_or_else(|| {
                eprintln!("Error: --data-dir required for multi-tenant mode");
                std::process::exit(1);
            });
            ServerMode::MultiTenant {
                data_dir: std::path::PathBuf::from(&dir),
                registry: Arc::new(tokio::sync::Mutex::new(TenantRegistry::new(
                    std::path::Path::new(&dir),
                ))),
            }
        } else {
            let mut server = agentic_codebase::mcp::McpServer::new();

            // Pre-load graph if specified
            if let Some(gp) = graph_path {
                let name = graph_name.unwrap_or_else(|| graph_name_from_path(gp));

                match agentic_codebase::AcbReader::read_from_file(std::path::Path::new(gp)) {
                    Ok(graph) => {
                        tracing::info!("Pre-loaded graph '{name}' from {gp}");
                        server.load_graph(name, graph);
                    }
                    Err(e) => {
                        eprintln!("Error: Failed to load graph: {e}");
                        std::process::exit(1);
                    }
                }
            }

            ServerMode::Single(Arc::new(tokio::sync::Mutex::new(server)))
        };

        let transport = SseTransport::with_config(effective_token, mode);
        if let Err(e) = transport.run(addr).await {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    });
}

/// Extract a graph name from a file path.
fn graph_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("default")
        .to_string()
}
