//! Process runner - Executes the Beanstalk compiler binary as a subprocess
//!
//! This module owns subprocess execution of the built `bean` binary.
//! It measures wall-clock time and captures stdout/stderr.
//!
//! # What this module owns
//! - Spawning `std::process::Command` for the bean binary
//! - Measuring subprocess wall-clock duration
//! - Capturing stdout and stderr output
//!
//! # What this module does NOT own
//! - Building the compiler binary (see `compiler_binary.rs`)
//! - Parsing benchmark observations from stdout (see `bench_observations.rs`)
//! - Orchestration of warmup and measured iterations (see `bench.rs`)

use std::path::Path;
use std::process::Command;
use std::time::Instant;

/// Result of executing a single subprocess run
///
/// Contains timing data, success status, and captured output.
#[derive(Debug, Clone)]
pub struct ProcessRun {
    /// Duration in milliseconds
    pub duration_ms: f64,
    /// Whether the command succeeded (exit code 0)
    pub success: bool,
    /// Captured stderr for diagnostic output on failure
    pub stderr: String,
    /// Captured stdout for detailed timer parsing
    pub stdout: String,
}

/// Run a bean compiler command as a timed subprocess
///
/// Spawns the bean binary, measures wall-clock time, and captures output.
///
/// # Arguments
///
/// * `bean_path` - Path to the bean binary (e.g., target/release/bean)
/// * `command` - The subcommand to execute (e.g., "check", "build")
/// * `args` - Arguments to pass to the subcommand
///
/// # Returns
///
/// A `ProcessRun` with timing and output data, or an error message.
pub fn run_bean_command(
    bean_path: &Path,
    command: &str,
    args: &[String],
) -> Result<ProcessRun, String> {
    let start = Instant::now();

    let output = Command::new(bean_path)
        .arg(command)
        .args(args)
        .output()
        .map_err(|e| {
            format!(
                "Failed to execute bean binary at '{}': {}",
                bean_path.display(),
                e
            )
        })?;

    let elapsed = start.elapsed();
    let duration_ms = elapsed.as_secs_f64() * 1000.0;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let success = output.status.success();

    Ok(ProcessRun {
        duration_ms,
        success,
        stderr,
        stdout,
    })
}

#[cfg(test)]
mod tests;
