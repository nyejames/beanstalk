//! Compiler binary builder - Builds the release and profiling Beanstalk compiler
//!
//! This module owns building compiler binaries with the `detailed_timers` feature
//! enabled. It supports two build profiles:
//!
//! - **Release** (`target/release/bean`): standard benchmark build.
//! - **Profiling** (`target/profiling/bean`): dedicated profiling build with
//!   frame pointers forced on for reliable Samply stack unwinding.
//!
//! # What this module owns
//! - Executing `cargo build --release --features detailed_timers`
//! - Executing `cargo build --profile profiling --features detailed_timers` with
//!   `RUSTFLAGS="-C force-frame-pointers=yes"`
//! - Locating built binaries in `target/release/` and `target/profiling/`
//! - Platform-specific executable suffix handling
//!
//! # What this module does NOT own
//! - Running the built binary (see `process_runner.rs`)
//! - Benchmark orchestration or case execution (see `bench.rs`)
//! - Profile orchestration or Samply integration (see `profile/`)

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Path to the built compiler binary
///
/// A thin wrapper around `PathBuf` that identifies a successfully built
/// compiler artifact from either release or profiling profiles.
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

/// Build the compiler with profiling profile, detailed timers, and forced frame pointers
///
/// Executes:
/// ```bash
/// RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --features detailed_timers --bin bean
/// ```
///
/// The profiling profile inherits from release but includes line-table debug
/// info, no stripping, thin LTO, and one codegen unit. Frame pointers are
/// forced on so Samply can unwind stacks reliably.
///
/// # Returns
///
/// A `CompilerBinary` pointing to `target/profiling/bean`, or an error message.
///
/// TODO: Phase 2 will wire this into the profile orchestration flow.
#[allow(dead_code)]
pub fn build_profiling_compiler_with_timers() -> Result<CompilerBinary, String> {
    let status = Command::new("cargo")
        .args([
            "build",
            "--profile",
            "profiling",
            "--features",
            "detailed_timers",
            "--bin",
            "bean",
        ])
        .env("RUSTFLAGS", "-C force-frame-pointers=yes")
        .status()
        .map_err(|e| format!("Failed to execute cargo build: {}", e))?;

    if !status.success() {
        return Err("Profiling compiler build failed".to_string());
    }

    let bean_path = profiling_compiler_path(env::consts::EXE_SUFFIX);

    if !bean_path.exists() {
        return Err(format!(
            "Bean binary not found at '{}' after profiling build",
            bean_path.display()
        ));
    }

    Ok(CompilerBinary { path: bean_path })
}

/// Build the release compiler path for the current platform suffix.
///
/// WHAT: Constructs the expected binary path under `target/release/`.
/// WHY: Platform-specific executable suffixes (e.g., `.exe` on Windows)
/// must be appended correctly so `exists()` checks find the real artifact.
fn release_compiler_path(exe_suffix: &str) -> PathBuf {
    compiler_path_with_suffix("target/release/bean", exe_suffix)
}

/// Build the profiling compiler path for the current platform suffix.
///
/// WHAT: Constructs the expected binary path under `target/profiling/`.
/// WHY: The profiling build uses a different target directory than release,
/// so it needs its own path resolver.
///
/// TODO: Phase 2 will wire this into the profile orchestration flow.
#[allow(dead_code)]
pub fn profiling_compiler_path(exe_suffix: &str) -> PathBuf {
    compiler_path_with_suffix("target/profiling/bean", exe_suffix)
}

/// Resolve a base binary path with the platform executable suffix.
///
/// WHAT: Appends the platform suffix to a base binary path (e.g., `.exe`).
/// WHY: Both release and profiling paths share the same suffix logic;
/// extracting it avoids duplication.
fn compiler_path_with_suffix(base: &str, exe_suffix: &str) -> PathBuf {
    let mut bean_path = PathBuf::from(base);
    if !exe_suffix.is_empty() {
        bean_path.set_extension(exe_suffix.trim_start_matches('.'));
    }

    bean_path
}

#[cfg(test)]
mod tests;
