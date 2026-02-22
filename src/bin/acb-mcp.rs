//! MCP server entry point for AgenticCodebase.
//!
//! Runs the MCP server over stdin/stdout (default) or HTTP/SSE (with `sse` feature).
//! All logic lives in `agentic_codebase::mcp`.

use clap::{Parser, Subcommand};
use std::io::{BufRead, BufReader, Read, Write};

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

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut stdout = stdout.lock();

    run_stdio_loop(&mut reader, &mut stdout, &mut server);
}

fn run_stdio_loop<R: BufRead + Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    server: &mut agentic_codebase::mcp::McpServer,
) {
    let mut line = String::new();
    let mut content_length: Option<usize> = None;

    loop {
        line.clear();
        let bytes = match reader.read_line(&mut line) {
            Ok(n) => n,
            Err(_) => break,
        };
        if bytes == 0 {
            break;
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);

        // Support header-framed MCP messages:
        //   Content-Length: <n>\r\n
        //   \r\n
        //   <json body>
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("content-length:") {
            let rest = trimmed.split_once(':').map(|(_, rhs)| rhs).unwrap_or("");
            match rest.trim().parse::<usize>() {
                Ok(n) => content_length = Some(n),
                Err(_) => content_length = None,
            }
            continue;
        }

        if let Some(n) = content_length {
            // Skip optional header separator line.
            if trimmed.is_empty() {
                let mut buf = vec![0u8; n];
                if reader.read_exact(&mut buf).is_err() {
                    break;
                }
                let raw = String::from_utf8_lossy(&buf).to_string();
                let response = server.handle_raw(raw.trim());
                if !response.is_empty() && write_framed(writer, &response).is_err() {
                    break;
                }
                content_length = None;
                continue;
            }

            // Ignore extra header lines (e.g. Content-Type).
            continue;
        }

        if trimmed.is_empty() {
            continue;
        }

        let response = server.handle_raw(trimmed);
        if response.is_empty() {
            continue;
        }
        if writeln!(writer, "{}", response).is_err() {
            break;
        }
        if writer.flush().is_err() {
            break;
        }
    }
}

fn write_framed<W: Write>(writer: &mut W, response: &str) -> std::io::Result<()> {
    let len = response.len();
    write!(writer, "Content-Length: {}\r\n\r\n{}", len, response)?;
    writer.flush()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn stdio_loop_handles_json_lines() {
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let mut out = Vec::new();
        let mut server = agentic_codebase::mcp::McpServer::new();

        run_stdio_loop(&mut reader, &mut out, &mut server);

        let output = String::from_utf8(out).expect("utf8 output");
        assert!(output.contains("\"id\":1"));
        assert!(output.contains("\"id\":2"));
        assert!(output.contains("\"tools\""));
    }

    #[test]
    fn stdio_loop_handles_content_length_framing() {
        let init = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}";
        let tools = "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}";
        let input = format!(
            "Content-Length: {}\r\n\r\n{}Content-Length: {}\r\n\r\n{}",
            init.len(),
            init,
            tools.len(),
            tools
        );

        let mut reader = Cursor::new(input.into_bytes());
        let mut out = Vec::new();
        let mut server = agentic_codebase::mcp::McpServer::new();

        run_stdio_loop(&mut reader, &mut out, &mut server);

        let output = String::from_utf8(out).expect("utf8 output");
        assert!(output.contains("Content-Length:"));
        assert!(output.contains("\"id\":1"));
        assert!(output.contains("\"id\":2"));
    }
}
