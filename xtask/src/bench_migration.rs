//! Benchmark result migration - Archives legacy benchmark result directories
//!
//! This module owns migrating old benchmark results from the legacy
//! `benchmarks/results/` path to timestamped archives under
//! `benchmarks/old-benchmarks/`.
//!
//! # What this module owns
//! - Detecting the presence of legacy result directories
//! - Creating timestamped archive directories
//! - Moving (renaming) legacy data without copying contents
//!
//! # What this module does NOT own
//! - Reading or parsing benchmark result contents
//! - Writing new benchmark results
//! - Orchestration of when migration runs (see `bench.rs`)
use crate::bench_time::BenchmarkTimestamp;
use std::fs;
use std::path::Path;

/// Migrate legacy benchmark results from `results_path` to `old_benchmarks_dir`.
///
/// Only runs when `results_path` exists. On failure, prints a warning and
/// continues without panicking.
///
/// # Arguments
///
/// * `results_path` - Path to the legacy results directory
/// * `old_benchmarks_dir` - Parent directory for timestamped archives
pub fn migrate_old_results(results_path: &Path, old_benchmarks_dir: &Path) {
    if !results_path.exists() {
        return;
    }

    let now = BenchmarkTimestamp::now();
    let archive_name = format!(
        "results-{:04}-{:02}-{:02}-{:02}-{:02}-{:02}",
        now.year, now.month, now.day, now.hour, now.minute, 0
    );
    let archive_path = old_benchmarks_dir.join(&archive_name);

    if let Err(e) = fs::create_dir_all(old_benchmarks_dir) {
        eprintln!(
            "Warning: could not create old-benchmarks directory '{}': {}. Leaving benchmarks/results/ in place.",
            old_benchmarks_dir.display(),
            e
        );
        return;
    }

    if let Err(e) = fs::rename(results_path, &archive_path) {
        eprintln!(
            "Warning: could not migrate old benchmark results from '{}' to '{}': {}. Leaving in place.",
            results_path.display(),
            archive_path.display(),
            e
        );
        return;
    }

    println!(
        "Migrated old benchmark results to '{}'",
        archive_path.display()
    );
}

#[cfg(test)]
mod tests;
