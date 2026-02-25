//! CLI entry point for the `acb` binary.

use agentic_codebase::cli::commands::{run, Cli};
use clap::Parser;

fn main() {
    // Initialize tracing (logs to stderr)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        // Error messages from commands already include styled formatting,
        // so print them directly without adding another "Error:" prefix.
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
