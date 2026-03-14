//! Phase 6: CLI integration tests.
//!
//! Tests the `acb` binary via `std::process::Command`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Returns the path to the compiled `acb` binary.
fn acb_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_acb"))
}

/// Create a temporary directory containing a single Rust source file.
fn create_sample_rust_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("sample.rs");
    fs::write(
        &src,
        r#"
/// A sample function.
pub fn hello(name: &str) -> String {
    format!("Hello, {}!", name)
}

/// Another function that calls hello.
pub fn greet() -> String {
    hello("world")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello() {
        assert_eq!(hello("x"), "Hello, x!");
    }
}
"#,
    )
    .unwrap();
    dir
}

fn create_gitignore_fixture_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(".git").join("info")).unwrap();
    fs::create_dir_all(dir.path().join("ignored")).unwrap();

    fs::write(dir.path().join(".gitignore"), "ignored/\n").unwrap();
    fs::write(dir.path().join("main.rs"), "pub fn root() {}\n").unwrap();
    fs::write(dir.path().join("ignored").join("extra.rs"), "pub fn extra() {}\n").unwrap();

    dir
}

fn read_coverage_counts(path: &std::path::Path) -> (u64, u64) {
    let raw = fs::read_to_string(path).unwrap();
    let payload: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let coverage = payload.get("coverage").expect("coverage object");
    let files_seen = coverage
        .get("files_seen")
        .and_then(|v| v.as_u64())
        .expect("files_seen");
    let files_candidate = coverage
        .get("files_candidate")
        .and_then(|v| v.as_u64())
        .expect("files_candidate");
    (files_seen, files_candidate)
}

/// Compile a sample directory and return the path to the .acb output.
fn compile_sample() -> (TempDir, PathBuf) {
    let src_dir = create_sample_rust_dir();
    let out_dir = TempDir::new().unwrap();
    let acb_path = out_dir.path().join("test_output.acb");

    let output = Command::new(acb_bin())
        .args([
            "compile",
            src_dir.path().to_str().unwrap(),
            "-o",
            acb_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(acb_path.exists(), "Expected .acb file to be created");

    // Keep src_dir alive as a separate return (it drops when test ends)
    // We only need out_dir and acb_path
    // But we must keep src_dir alive until compile is done - it already is.
    // Return out_dir to keep it alive.
    let _ = src_dir; // consumed, but compile already ran
    (out_dir, acb_path)
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[test]
fn test_cli_help() {
    let output = Command::new(acb_bin()).arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Semantic code compiler") || stdout.contains("AgenticCodebase"),
        "Expected help text to mention the project: {}",
        stdout
    );
}

#[test]
fn test_cli_version() {
    let output = Command::new(acb_bin()).arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("acb"));
}

#[test]
fn test_cli_compile() {
    let src_dir = create_sample_rust_dir();
    let out_dir = TempDir::new().unwrap();
    let acb_path = out_dir.path().join("compiled.acb");

    let output = Command::new(acb_bin())
        .args([
            "compile",
            src_dir.path().to_str().unwrap(),
            "-o",
            acb_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(acb_path.exists());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Compiled"));
    assert!(stdout.contains("Units:"));
}

#[test]
fn test_cli_compile_with_exclude() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("lib.rs");
    fs::write(&src, "pub fn excluded_fn() {}").unwrap();
    let gen_dir = dir.path().join("generated");
    fs::create_dir(&gen_dir).unwrap();
    fs::write(gen_dir.join("gen.rs"), "pub fn gen_fn() {}").unwrap();

    let out_dir = TempDir::new().unwrap();
    let acb_path = out_dir.path().join("excl.acb");

    let output = Command::new(acb_bin())
        .args([
            "compile",
            dir.path().to_str().unwrap(),
            "-o",
            acb_path.to_str().unwrap(),
            "--exclude",
            "**/generated/**",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "compile with exclude failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(acb_path.exists());
}

#[test]
fn test_cli_compile_no_gitignore() {
    let src_dir = create_gitignore_fixture_dir();
    let out_dir = TempDir::new().unwrap();

    let default_acb = out_dir.path().join("default.acb");
    let no_gitignore_acb = out_dir.path().join("no-gitignore.acb");
    let default_cov = out_dir.path().join("default-coverage.json");
    let no_gitignore_cov = out_dir.path().join("no-gitignore-coverage.json");

    let default_output = Command::new(acb_bin())
        .args([
            "compile",
            src_dir.path().to_str().unwrap(),
            "-o",
            default_acb.to_str().unwrap(),
            "--coverage-report",
            default_cov.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        default_output.status.success(),
        "compile default failed: {}",
        String::from_utf8_lossy(&default_output.stderr)
    );

    let no_gitignore_output = Command::new(acb_bin())
        .args([
            "compile",
            src_dir.path().to_str().unwrap(),
            "-o",
            no_gitignore_acb.to_str().unwrap(),
            "--no-gitignore",
            "--coverage-report",
            no_gitignore_cov.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        no_gitignore_output.status.success(),
        "compile --no-gitignore failed: {}",
        String::from_utf8_lossy(&no_gitignore_output.stderr)
    );

    let (default_seen, default_candidate) = read_coverage_counts(&default_cov);
    let (no_gitignore_seen, no_gitignore_candidate) = read_coverage_counts(&no_gitignore_cov);

    assert!(
        no_gitignore_seen >= default_seen,
        "--no-gitignore should not reduce files_seen (default={default_seen}, no_gitignore={no_gitignore_seen})"
    );
    assert!(
        no_gitignore_candidate > default_candidate,
        "--no-gitignore should include ignored candidates (default={default_candidate}, no_gitignore={no_gitignore_candidate})"
    );
}

#[test]
fn test_cli_compile_output_flag() {
    let src_dir = create_sample_rust_dir();
    let out_dir = TempDir::new().unwrap();
    let custom_path = out_dir.path().join("custom_name.acb");

    let output = Command::new(acb_bin())
        .args([
            "compile",
            src_dir.path().to_str().unwrap(),
            "-o",
            custom_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "compile with -o failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(custom_path.exists(), "Custom output path should exist");
}

#[test]
fn test_cli_info() {
    let (_out_dir, acb_path) = compile_sample();

    let output = Command::new(acb_bin())
        .args(["info", acb_path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "info failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Units:"),
        "Expected unit count in output: {}",
        stdout
    );
    assert!(
        stdout.contains("Edges:"),
        "Expected edge count in output: {}",
        stdout
    );
}

#[test]
fn test_cli_query_symbol() {
    let (_out_dir, acb_path) = compile_sample();

    let output = Command::new(acb_bin())
        .args([
            "query",
            acb_path.to_str().unwrap(),
            "symbol",
            "--name",
            "hello",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "query symbol failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Symbol lookup"),
        "Expected symbol lookup header: {}",
        stdout
    );
}

#[test]
fn test_cli_query_impact() {
    let (_out_dir, acb_path) = compile_sample();

    // Use unit_id 0 which should exist (first unit added to graph).
    let output = Command::new(acb_bin())
        .args([
            "query",
            acb_path.to_str().unwrap(),
            "impact",
            "--unit-id",
            "0",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "query impact failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Impact analysis"),
        "Expected impact analysis header: {}",
        stdout
    );
}

#[test]
fn test_cli_query_deps() {
    let (_out_dir, acb_path) = compile_sample();

    let output = Command::new(acb_bin())
        .args([
            "query",
            acb_path.to_str().unwrap(),
            "deps",
            "--unit-id",
            "0",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "query deps failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Dependencies of"),
        "Expected deps header: {}",
        stdout
    );
}

#[test]
fn test_cli_json_output() {
    let (_out_dir, acb_path) = compile_sample();

    let output = Command::new(acb_bin())
        .args(["--format", "json", "info", acb_path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "json info failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Verify it parses as valid JSON.
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("Output is not valid JSON: {}\nOutput was: {}", e, stdout);
    });
    assert!(
        parsed.get("units").is_some(),
        "JSON should have 'units' field"
    );
    assert!(
        parsed.get("edges").is_some(),
        "JSON should have 'edges' field"
    );
}

#[test]
fn test_cli_invalid_path() {
    let output = Command::new(acb_bin())
        .args(["compile", "/nonexistent/path/does/not/exist"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "Expected failure for invalid path"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not exist")
            || stderr.contains("not found")
            || stderr.contains("FAIL"),
        "Expected error message in stderr: {}",
        stderr
    );
}

#[test]
fn test_cli_invalid_acb() {
    let dir = TempDir::new().unwrap();
    let not_acb = dir.path().join("fake.txt");
    fs::write(&not_acb, "this is not an acb file").unwrap();

    let output = Command::new(acb_bin())
        .args(["info", not_acb.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "Expected failure for non-.acb file"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Expected .acb") || stderr.contains("FAIL"),
        "Expected error in stderr: {}",
        stderr
    );
}

#[test]
fn test_cli_get() {
    let (_out_dir, acb_path) = compile_sample();

    let output = Command::new(acb_bin())
        .args(["get", acb_path.to_str().unwrap(), "0"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "get failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Unit 0"),
        "Expected unit details: {}",
        stdout
    );
    assert!(
        stdout.contains("Name:") || stdout.contains("name"),
        "Expected name field: {}",
        stdout
    );
}

#[test]
fn test_cli_empty_dir() {
    let dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let acb_path = out_dir.path().join("empty.acb");

    let output = Command::new(acb_bin())
        .args([
            "compile",
            dir.path().to_str().unwrap(),
            "-o",
            acb_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    // Empty dir should still succeed (produces a file with 0 units).
    assert!(
        output.status.success(),
        "compile empty dir failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(acb_path.exists(), "Expected .acb file even for empty dir");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Units:"), "Expected units line: {}", stdout);
}

#[test]
fn test_cli_get_json() {
    let (_out_dir, acb_path) = compile_sample();

    let output = Command::new(acb_bin())
        .args(["--format", "json", "get", acb_path.to_str().unwrap(), "0"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "get json failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("Output is not valid JSON: {}\nOutput was: {}", e, stdout);
    });
    assert!(parsed.get("id").is_some(), "JSON should have 'id' field");
    assert!(
        parsed.get("name").is_some(),
        "JSON should have 'name' field"
    );
    assert!(
        parsed.get("unit_type").is_some(),
        "JSON should have 'unit_type' field"
    );
}
