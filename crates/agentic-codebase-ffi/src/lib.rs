//! Minimal FFI facade for AgenticCodebase.

/// Crate version exposed for foreign runtimes.
pub fn agentic_codebase_ffi_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
