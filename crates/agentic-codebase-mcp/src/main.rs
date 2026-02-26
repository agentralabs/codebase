//! MCP server entry point for AgenticCodebase.
//!
//! Runs the MCP server over stdin/stdout (default) or HTTP/SSE (with `sse` feature).
//! All logic lives in `agentic_codebase::mcp`.

use clap::{Parser, Subcommand};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use agentic_codebase::engine::compile::{CompileOptions, CompilePipeline};
use walkdir::WalkDir;

mod ghost_bridge;

#[derive(Parser)]
#[command(
    name = "agentic-codebase-mcp",
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

    let graph_path = cli.graph.or_else(auto_resolve_graph_path);
    let graph_name = cli
        .name
        .or_else(|| graph_path.as_ref().map(|path| graph_name_from_path(path)));

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
                // Don't exit — set deferred path for lazy retry on first tool call.
                tracing::warn!("Eager graph load failed ({e}), deferring to lazy load");
                server.set_deferred_graph(name, graph_path.to_string());
            }
        }
    }

    // Ghost Writer: sync codebase context to Claude, Cursor, Windsurf, Cody
    let mut ghost = ghost_bridge::GhostBridge::new();

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut stdout = stdout.lock();

    run_stdio_loop(&mut reader, &mut stdout, &mut server, &mut ghost);
}

/// Hard limit for framed stdio payloads (8 MiB).
const MAX_CONTENT_LENGTH_BYTES: usize = 8 * 1024 * 1024;

fn run_stdio_loop<R: BufRead + Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    server: &mut agentic_codebase::mcp::McpServer,
    ghost: &mut Option<ghost_bridge::GhostBridge>,
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
                Ok(n) if n <= MAX_CONTENT_LENGTH_BYTES => content_length = Some(n),
                Ok(n) => {
                    eprintln!(
                        "Content-Length {n} exceeds max frame size of {MAX_CONTENT_LENGTH_BYTES} bytes"
                    );
                    break;
                }
                Err(_) => {
                    eprintln!("Invalid Content-Length header: {trimmed}");
                    content_length = None;
                }
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
                if let Some(ref mut g) = ghost {
                    g.sync(server);
                }
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
        if let Some(ref mut g) = ghost {
            g.sync(server);
        }
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

fn auto_resolve_graph_path() -> Option<String> {
    // Respect standard resolution first when a concrete file exists.
    let resolved = agentic_codebase::config::resolve_graph_path(None);
    if Path::new(&resolved).is_file() {
        return Some(resolved);
    }

    let repo_root = resolve_repo_root()?;
    if is_common_root(&repo_root) {
        return None;
    }

    let cache_dir = resolve_graph_cache_dir();
    let graph_path = cache_dir.join(format!("{}.acb", repo_identity_key(&repo_root)));

    if graph_is_stale(&repo_root, &graph_path) {
        if let Err(err) = compile_graph_for_repo(&repo_root, &graph_path) {
            tracing::warn!(
                "Auto graph compile failed for '{}': {}",
                repo_root.display(),
                err
            );
        }
    }

    if graph_path.is_file() {
        return Some(graph_path.to_string_lossy().to_string());
    }

    None
}

fn resolve_repo_root() -> Option<PathBuf> {
    for key in ["AGENTRA_WORKSPACE_ROOT", "AGENTRA_PROJECT_ROOT"] {
        if let Ok(value) = env::var(key) {
            let path = PathBuf::from(value);
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
    {
        if output.status.success() {
            let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !root.is_empty() {
                let path = PathBuf::from(root);
                if path.is_dir() {
                    return Some(path);
                }
            }
        }
    }

    env::current_dir().ok().filter(|p| p.is_dir())
}

fn resolve_graph_cache_dir() -> PathBuf {
    if let Ok(path) = env::var("AGENTRA_GRAPH_CACHE_DIR") {
        return PathBuf::from(path);
    }
    if let Ok(codex_home) = env::var("CODEX_HOME") {
        return Path::new(&codex_home).join("graphs");
    }
    if let Ok(home) = env::var("HOME").or_else(|_| env::var("USERPROFILE")) {
        return Path::new(&home).join(".codex").join("graphs");
    }
    PathBuf::from(".acb")
}

fn is_common_root(path: &Path) -> bool {
    if path == Path::new("/") {
        return true;
    }
    let home = env::var("HOME").or_else(|_| env::var("USERPROFILE")).ok();
    if let Some(home) = home {
        let home = PathBuf::from(home);
        return path == home || path == home.join("Documents") || path == home.join("Desktop");
    }
    false
}

fn repo_identity_key(path: &Path) -> String {
    let raw_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("workspace");
    let mut slug = String::with_capacity(raw_name.len());
    for ch in raw_name.chars() {
        let mapped = if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
            ch.to_ascii_lowercase()
        } else {
            '-'
        };
        slug.push(mapped);
    }
    let slug = slug.trim_matches('-').to_string();
    let slug = if slug.is_empty() {
        "workspace".to_string()
    } else {
        slug
    };

    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let hash12 = format!("{:x}", digest);
    format!("{}-{}", slug, &hash12[..12])
}

fn graph_is_stale(repo_root: &Path, graph_path: &Path) -> bool {
    if !graph_path.is_file() {
        return true;
    }

    let graph_mtime = match fs::metadata(graph_path).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return true,
    };

    for entry in WalkDir::new(repo_root)
        .into_iter()
        .filter_entry(|e| !should_skip_path(e.path()))
    {
        let entry = match entry {
            Ok(v) => v,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() || !is_source_file(entry.path()) {
            continue;
        }
        let modified = match entry.metadata() {
            Ok(meta) => match meta.modified() {
                Ok(ts) => ts,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        if modified > graph_mtime {
            return true;
        }
    }

    false
}

fn should_skip_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(|name| {
            matches!(
                name,
                ".git"
                    | "target"
                    | "node_modules"
                    | ".venv"
                    | "venv"
                    | "dist"
                    | "build"
                    | ".next"
                    | ".cache"
            )
        })
        .unwrap_or(false)
}

fn is_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("rs")
            | Some("py")
            | Some("ts")
            | Some("tsx")
            | Some("js")
            | Some("jsx")
            | Some("go")
            | Some("java")
            | Some("c")
            | Some("cc")
            | Some("cpp")
            | Some("h")
            | Some("hpp")
    )
}

fn compile_graph_for_repo(repo_root: &Path, graph_path: &Path) -> Result<(), String> {
    if let Some(parent) = graph_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create cache dir failed: {e}"))?;
    }

    with_graph_lock(graph_path, || {
        if !graph_is_stale(repo_root, graph_path) {
            return Ok(());
        }
        let options = CompileOptions {
            output: graph_path.to_path_buf(),
            ..CompileOptions::default()
        };

        let pipeline = CompilePipeline::new();
        pipeline
            .compile_and_write(repo_root, &options)
            .map(|_| ())
            .map_err(|e| format!("{e}"))
    })?;

    let graph_mtime = fs::metadata(graph_path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);
    tracing::info!(
        "Auto-indexed graph '{}' at {}",
        graph_name_from_path(&graph_path.to_string_lossy()),
        graph_path.display()
    );
    tracing::debug!("Graph mtime: {:?}", graph_mtime);
    Ok(())
}

fn with_graph_lock<F>(graph_path: &Path, f: F) -> Result<(), String>
where
    F: FnOnce() -> Result<(), String>,
{
    let lock_dir = PathBuf::from(format!("{}.lock", graph_path.display()));
    let pid_file = lock_dir.join("pid");
    let max_wait = env::var("AGENTRA_GRAPH_LOCK_WAIT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(90);
    let stale_secs = env::var("AGENTRA_GRAPH_LOCK_STALE_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(300);

    let mut waited = 0u64;
    loop {
        match fs::create_dir(&lock_dir) {
            Ok(_) => {
                let _ = fs::write(&pid_file, std::process::id().to_string());
                break;
            }
            Err(_) => {
                if lock_is_stale(&lock_dir, stale_secs) {
                    let _ = fs::remove_dir_all(&lock_dir);
                    continue;
                }
                if waited >= max_wait {
                    if graph_path.is_file() {
                        return Ok(());
                    }
                    return Err(format!(
                        "graph build lock timeout for {}",
                        graph_path.display()
                    ));
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
                waited += 1;
            }
        }
    }

    struct LockGuard {
        lock_dir: PathBuf,
        pid_file: PathBuf,
    }
    impl Drop for LockGuard {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.pid_file);
            let _ = fs::remove_dir(&self.lock_dir);
        }
    }
    let _guard = LockGuard { lock_dir, pid_file };
    f()
}

fn lock_is_stale(lock_dir: &Path, stale_secs: u64) -> bool {
    let modified = match fs::metadata(lock_dir).and_then(|m| m.modified()) {
        Ok(m) => m,
        Err(_) => return true,
    };
    match SystemTime::now().duration_since(modified) {
        Ok(age) => age.as_secs() >= stale_secs,
        Err(_) => true,
    }
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

    #[test]
    fn repo_identity_key_is_deterministic() {
        let path = PathBuf::from("/tmp/project-alpha");
        let a = repo_identity_key(&path);
        let b = repo_identity_key(&path);
        assert_eq!(a, b);
    }

    #[test]
    fn repo_identity_key_differs_for_same_basename_in_different_paths() {
        let a = PathBuf::from("/tmp/team-a/service");
        let b = PathBuf::from("/tmp/team-b/service");
        let ka = repo_identity_key(&a);
        let kb = repo_identity_key(&b);
        assert_ne!(ka, kb);
    }
}
