//! Benchmark orchestration module - Coordinates benchmark execution
//!
//! This module orchestrates the entire benchmark workflow: building the compiler,
//! parsing benchmark cases, executing warmup and measured runs, collecting results,
//! and generating reports.

use crate::case_parser::parse_cases;
use crate::command::{CommandRun, RunKind, execute_benchmark};
use crate::report::{
    BenchmarkMeasurement, BenchmarkStats, append_jsonl, calculate_stats,
    generate_directory_timestamp, generate_timestamp, write_summary,
};
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Benchmark execution options
#[derive(Debug, Clone)]
pub struct BenchOptions {
    /// Number of warmup runs (not recorded)
    pub warmup_runs: usize,
    /// Number of measured iterations (recorded)
    pub measured_iterations: usize,
    /// Mode name for reporting
    pub mode_name: String,
}

impl BenchOptions {
    /// Full benchmark mode: 2 warmup, 10 measured
    pub fn full() -> Self {
        Self {
            warmup_runs: 2,
            measured_iterations: 10,
            mode_name: "full (2 warmup, 10 measured)".to_string(),
        }
    }

    /// Quick benchmark mode: 1 warmup, 3 measured
    pub fn quick() -> Self {
        Self {
            warmup_runs: 1,
            measured_iterations: 3,
            mode_name: "quick (1 warmup, 3 measured)".to_string(),
        }
    }

    /// CI benchmark mode: 0 warmup, 1 measured
    pub fn ci() -> Self {
        Self {
            warmup_runs: 0,
            measured_iterations: 1,
            mode_name: "ci (0 warmup, 1 measured)".to_string(),
        }
    }
}

/// Run the complete benchmark suite
///
/// Orchestrates the entire benchmark workflow:
/// 1. Build the compiler
/// 2. Parse benchmark cases
/// 3. Create results directory
/// 4. Execute benchmarks with warmup and measurement
/// 5. Generate reports
///
/// # Arguments
///
/// * `options` - Benchmark execution options (warmup, iterations, mode)
///
/// # Returns
///
/// Ok(()) on success, or an error message on failure.
pub fn run_benchmarks(options: BenchOptions) -> Result<(), String> {
    println!("=== Beanstalk Benchmark Suite ===");
    println!("Mode: {}", options.mode_name);
    println!();

    // Step 1: Build the compiler
    println!("[1/6] Building compiler...");
    let bean_path = build_compiler()?;
    println!("      Built: {}", bean_path.display());
    println!();

    // Step 2: Parse benchmark cases
    println!("[2/6] Parsing benchmark cases...");
    let cases_path = PathBuf::from("benchmarks/cases.txt");
    let cases = parse_cases(&cases_path)?;
    println!("      Found {} benchmark cases", cases.len());
    println!();

    // Step 3: Create results directory
    println!("[3/6] Creating results directory...");
    let timestamp = generate_directory_timestamp();
    let results_dir = PathBuf::from("benchmarks/results").join(&timestamp);
    let logs_dir = results_dir.join("logs");
    fs::create_dir_all(&logs_dir)
        .map_err(|e| format!("Failed to create results directory: {}", e))?;
    println!("      Created: {}", results_dir.display());
    println!();

    // Open JSONL file for writing
    let jsonl_path = results_dir.join("raw.jsonl");
    let mut jsonl_file =
        File::create(&jsonl_path).map_err(|e| format!("Failed to create JSONL file: {}", e))?;

    // Step 4: Execute benchmarks
    println!("[4/6] Executing benchmarks...");
    let mut all_measurements: Vec<BenchmarkMeasurement> = Vec::new();
    let mut case_measurements: HashMap<String, Vec<BenchmarkMeasurement>> = HashMap::new();
    let mut total_failures = 0;

    for (case_idx, case) in cases.iter().enumerate() {
        println!("      [{}/{}] {}", case_idx + 1, cases.len(), case.name);

        // Execute warmup runs
        if options.warmup_runs > 0 {
            print!("              Warmup: ");
            for warmup_idx in 0..options.warmup_runs {
                match execute_benchmark(&bean_path, &case.command, &case.args, RunKind::Warmup) {
                    Ok(_) => print!("."),
                    Err(e) => {
                        eprintln!(
                            "\n              Warning: Warmup {} failed: {}",
                            warmup_idx + 1,
                            e
                        );
                    }
                }
            }
            println!();
        }

        // Execute measured iterations
        print!("              Measured: ");
        for iter_idx in 1..=options.measured_iterations {
            match execute_benchmark(&bean_path, &case.command, &case.args, RunKind::Measured) {
                Ok(run) => {
                    print!(".");

                    // Create measurement
                    let measurement = BenchmarkMeasurement {
                        case_name: case.name.clone(),
                        iteration: iter_idx,
                        duration_ms: run.duration_ms,
                        success: run.success,
                        timestamp: generate_timestamp(),
                    };

                    // Write to JSONL
                    append_jsonl(&mut jsonl_file, &measurement)
                        .map_err(|e| format!("Failed to write measurement: {}", e))?;

                    // Store measurement
                    all_measurements.push(measurement.clone());
                    case_measurements
                        .entry(case.name.clone())
                        .or_default()
                        .push(measurement.clone());

                    // Write log file
                    write_log_file(&logs_dir, &case.name, iter_idx, &run)?;

                    if !run.success {
                        total_failures += 1;
                    }
                }
                Err(e) => {
                    eprintln!(
                        "\n              Error: Iteration {} failed: {}",
                        iter_idx, e
                    );
                    total_failures += 1;
                }
            }
        }
        println!();
    }
    println!();

    // Step 5: Calculate statistics
    println!("[5/6] Calculating statistics...");
    let mut all_stats: Vec<BenchmarkStats> = Vec::new();
    for case in &cases {
        if let Some(measurements) = case_measurements.get(&case.name) {
            let stats = calculate_stats(measurements);
            all_stats.push(stats);
        }
    }
    println!("      Computed stats for {} cases", all_stats.len());
    println!();

    // Step 6: Write summary report
    println!("[6/6] Writing summary report...");
    let summary_path = results_dir.join("summary.md");
    write_summary(&summary_path, &all_stats, &options.mode_name, &timestamp)?;
    println!("      Summary: {}", summary_path.display());
    println!();

    // Update symlinks
    update_symlinks(&results_dir)?;

    // Print final summary
    println!("=== Benchmark Complete ===");
    println!("Total cases: {}", cases.len());
    println!("Total iterations: {}", all_measurements.len());
    println!("Total failures: {}", total_failures);
    println!();
    println!("Results: {}", results_dir.display());
    println!("Raw data: {}", jsonl_path.display());
    println!("Summary: {}", summary_path.display());

    // Exit with error if all benchmarks failed
    if total_failures == all_measurements.len() && !all_measurements.is_empty() {
        return Err("All benchmarks failed".to_string());
    }

    Ok(())
}

/// Build the compiler with release profile
///
/// WHAT: Executes cargo build --release --features detailed_timers
/// WHY: Need optimized binary for accurate performance measurements
fn build_compiler() -> Result<PathBuf, String> {
    let status = Command::new("cargo")
        .args(["build", "--release", "--features", "detailed_timers"])
        .status()
        .map_err(|e| format!("Failed to execute cargo build: {}", e))?;

    if !status.success() {
        return Err("Compiler build failed".to_string());
    }

    // Determine bean binary path with platform-specific extension
    let mut bean_path = PathBuf::from("target/release/bean");
    if !env::consts::EXE_SUFFIX.is_empty() {
        bean_path.set_extension(&env::consts::EXE_SUFFIX[1..]); // Skip leading '.'
    }

    // Verify binary exists
    if !bean_path.exists() {
        return Err(format!(
            "Bean binary not found at '{}' after build",
            bean_path.display()
        ));
    }

    Ok(bean_path)
}

/// Write a log file for a benchmark run
///
/// WHAT: Writes stdout and stderr to logs/<case_name>_iter_<N>.log
/// WHY: Preserves execution output for debugging and analysis
fn write_log_file(
    logs_dir: &Path,
    case_name: &str,
    iteration: usize,
    run: &CommandRun,
) -> Result<(), String> {
    let log_filename = format!("{}_iter_{}.log", case_name, iteration);
    let log_path = logs_dir.join(log_filename);

    let mut content = String::new();
    content.push_str("=== STDOUT ===\n");
    content.push_str(&run.stdout);
    content.push_str("\n\n=== STDERR ===\n");
    content.push_str(&run.stderr);
    content.push('\n');

    fs::write(&log_path, content)
        .map_err(|e| format!("Failed to write log file '{}': {}", log_path.display(), e))
}

/// Update symlinks to point to latest results
///
/// WHAT: Creates/updates latest.jsonl and latest-summary.md symlinks
/// WHY: Provides convenient access to most recent benchmark results
fn update_symlinks(results_dir: &Path) -> Result<(), String> {
    let results_parent = PathBuf::from("benchmarks/results");

    let jsonl_symlink = results_parent.join("latest.jsonl");
    let summary_symlink = results_parent.join("latest-summary.md");

    let jsonl_target = results_dir.join("raw.jsonl");
    let summary_target = results_dir.join("summary.md");

    // Remove old symlinks if they exist
    let _ = fs::remove_file(&jsonl_symlink);
    let _ = fs::remove_file(&summary_symlink);

    // Create new symlinks (platform-specific)
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(&jsonl_target, &jsonl_symlink)
            .map_err(|e| format!("Failed to create symlink: {}", e))?;
        symlink(&summary_target, &summary_symlink)
            .map_err(|e| format!("Failed to create symlink: {}", e))?;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_file;
        symlink_file(&jsonl_target, &jsonl_symlink)
            .map_err(|e| format!("Failed to create symlink: {}", e))?;
        symlink_file(&summary_target, &summary_symlink)
            .map_err(|e| format!("Failed to create symlink: {}", e))?;
    }

    Ok(())
}
