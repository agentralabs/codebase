use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build a simple symbol lookup payload from AgenticCodebase.
    let graph = std::env::var("ACB_GRAPH").unwrap_or_else(|_| "test.acb".to_string());
    let symbol = std::env::var("ACB_SYMBOL").unwrap_or_else(|_| "main".to_string());

    let acb = Command::new("acb")
        .args(["query", &graph, "symbol", "--name", &symbol])
        .output()?;

    if !acb.status.success() {
        eprintln!("acb query failed: {}", String::from_utf8_lossy(&acb.stderr));
        std::process::exit(1);
    }

    let context = String::from_utf8_lossy(&acb.stdout);
    let prompt = format!(
        "Analyze this code symbol result and summarize risk:\n{}",
        context.trim()
    );

    let status = Command::new("ollama")
        .args(["run", "llama3", &prompt])
        .status()?;

    if !status.success() {
        eprintln!("ollama run failed");
        std::process::exit(1);
    }

    Ok(())
}
