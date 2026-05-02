//! Command execution module - Executes bean binary with timing
//!
//! This module provides functionality to execute the bean compiler binary
//! with timing measurements and output capture. It supports both warmup
//! runs (not recorded) and measured runs (recorded to JSONL).

use std::path::Path;
use std::process::Command;
use std::time::Instant;

/// Distinguishes warmup runs from measured runs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    /// Warmup run - timing measured but not recorded
    Warmup,
    /// Measured run - timing recorded to JSONL
    Measured,
}

/// Result of executing a single command
///
/// Contains timing data, success status, and captured output.
#[derive(Debug, Clone)]
pub struct CommandRun {
    /// Duration in milliseconds
    pub duration_ms: f64,
    /// Whether the command succeeded (exit code 0)
    pub success: bool,
    /// Captured stdout
    pub stdout: String,
    /// Captured stderr
    pub stderr: String,
}

/// Execute a benchmark case against the bean binary
///
/// Spawns the bean binary as a subprocess, measures wall-clock time,
/// and captures stdout/stderr output.
///
/// # Arguments
///
/// * `bean_path` - Path to the bean binary (e.g., target/release/bean)
/// * `command` - The command to execute (e.g., "check", "build")
/// * `args` - Arguments to pass to the command
/// * `kind` - Whether this is a warmup or measured run
///
/// # Returns
///
/// A CommandRun with timing and output data, or an error message.
///
/// # Errors
///
/// Returns an error if:
/// - The bean binary cannot be executed
/// - Output cannot be captured
///
/// # Example
///
/// ```ignore
/// use std::path::Path;
/// use command::{execute_benchmark, RunKind};
///
/// let bean_path = Path::new("target/release/bean");
/// let run = execute_benchmark(bean_path, "check", &["test.bst"], RunKind::Measured)?;
/// println!("Duration: {:.2}ms", run.duration_ms);
/// ```
pub fn execute_benchmark(
    bean_path: &Path,
    command: &str,
    args: &[String],
    kind: RunKind,
) -> Result<CommandRun, String> {
    // Start timing
    let start = Instant::now();

    // Execute the bean binary with the command and arguments
    // WHAT: Spawn bean binary as subprocess with command + args
    // WHY: Need to measure actual compiler execution time
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

    // Measure elapsed time
    let elapsed = start.elapsed();
    let duration_ms = elapsed.as_secs_f64() * 1000.0;

    // Capture output
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Check success status
    let success = output.status.success();

    // Log warmup vs measured distinction
    let _kind_str = match kind {
        RunKind::Warmup => "warmup",
        RunKind::Measured => "measured",
    };

    Ok(CommandRun {
        duration_ms,
        success,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    // Helper to create a mock executable for testing
    #[cfg(unix)]
    fn create_mock_executable(path: &Path, exit_code: i32, stdout: &str, stderr: &str) {
        use std::os::unix::fs::PermissionsExt;

        let script = format!(
            r#"#!/bin/sh
echo -n "{}"
echo -n "{}" >&2
exit {}
"#,
            stdout, stderr, exit_code
        );

        fs::write(path, script).unwrap();
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    #[cfg(windows)]
    fn create_mock_executable(path: &Path, exit_code: i32, stdout: &str, stderr: &str) {
        let script = format!(
            r#"@echo off
echo|set /p="{}
echo|set /p="{}" 1>&2
exit {}
"#,
            stdout, stderr, exit_code
        );

        fs::write(path, script).unwrap();
    }

    #[test]
    fn test_execute_benchmark_success() {
        let temp_dir = std::env::temp_dir();
        let mock_bean = temp_dir.join("mock_bean_success");

        #[cfg(unix)]
        let mock_bean = mock_bean.with_extension("");
        #[cfg(windows)]
        let mock_bean = mock_bean.with_extension("bat");

        create_mock_executable(&mock_bean, 0, "success output", "");

        let result = execute_benchmark(
            &mock_bean,
            "check",
            &["test.bst".to_string()],
            RunKind::Measured,
        );

        assert!(result.is_ok());
        let run = result.unwrap();
        assert!(run.success);
        assert!(run.duration_ms >= 0.0);
        assert!(run.stdout.contains("success"));

        // Cleanup
        let _ = fs::remove_file(&mock_bean);
    }

    #[test]
    fn test_execute_benchmark_failure() {
        let temp_dir = std::env::temp_dir();
        let mock_bean = temp_dir.join("mock_bean_failure");

        #[cfg(unix)]
        let mock_bean = mock_bean.with_extension("");
        #[cfg(windows)]
        let mock_bean = mock_bean.with_extension("bat");

        create_mock_executable(&mock_bean, 1, "", "error output");

        let result = execute_benchmark(
            &mock_bean,
            "check",
            &["test.bst".to_string()],
            RunKind::Measured,
        );

        assert!(result.is_ok());
        let run = result.unwrap();
        assert!(!run.success);
        assert!(run.stderr.contains("error"));

        // Cleanup
        let _ = fs::remove_file(&mock_bean);
    }

    #[test]
    fn test_execute_benchmark_nonexistent() {
        let nonexistent = PathBuf::from("/nonexistent/bean");
        let result = execute_benchmark(
            &nonexistent,
            "check",
            &["test.bst".to_string()],
            RunKind::Measured,
        );

        assert!(result.is_err());
    }
}
