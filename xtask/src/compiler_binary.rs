//! Compiler binary builder - Builds the release Beanstalk compiler
//!
//! This module owns building the release compiler binary with the
//! `detailed_timers` feature enabled.
//!
//! # What this module owns
//! - Executing `cargo build --release --features detailed_timers`
//! - Locating the built binary in `target/release/`
//! - Platform-specific executable suffix handling
//!
//! # What this module does NOT own
//! - Running the built binary (see `process_runner.rs`)
//! - Benchmark orchestration or case execution (see `bench.rs`)

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Path to the built compiler binary
///
/// A thin wrapper around `PathBuf` that identifies a successfully built
/// release compiler artifact.
#[derive(Debug, Clone)]
pub struct CompilerBinary {
    pub path: PathBuf,
}

impl CompilerBinary {
    /// Return the path as a borrowed `Path`.
    pub fn as_path(&self) -> &Path {
        &self.path
    }
}

/// Build the compiler with release profile and detailed timers
///
/// Executes `cargo build --release --features detailed_timers`, then
/// verifies and returns the path to the built binary.
///
/// # Returns
///
/// A `CompilerBinary` pointing to the built artifact, or an error message.
pub fn build_release_compiler_with_timers() -> Result<CompilerBinary, String> {
    let status = Command::new("cargo")
        .args(["build", "--release", "--features", "detailed_timers"])
        .status()
        .map_err(|e| format!("Failed to execute cargo build: {}", e))?;

    if !status.success() {
        return Err("Compiler build failed".to_string());
    }

    let bean_path = release_compiler_path(env::consts::EXE_SUFFIX);

    if !bean_path.exists() {
        return Err(format!(
            "Bean binary not found at '{}' after build",
            bean_path.display()
        ));
    }

    Ok(CompilerBinary { path: bean_path })
}

/// Build the release compiler path for the current platform suffix.
fn release_compiler_path(exe_suffix: &str) -> PathBuf {
    let mut bean_path = PathBuf::from("target/release/bean");
    if !exe_suffix.is_empty() {
        bean_path.set_extension(exe_suffix.trim_start_matches('.'));
    }

    bean_path
}

#[cfg(test)]
mod tests;
