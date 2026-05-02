//! Report generation module - JSONL and Markdown report generation
//!
//! This module provides functionality to write raw timing data to JSONL files
//! and generate human-readable Markdown summary reports with statistics.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::SystemTime;

/// A single benchmark measurement to be written to JSONL
#[derive(Debug, Clone)]
pub struct BenchmarkMeasurement {
    /// Name of the benchmark case
    pub case_name: String,
    /// Iteration number (1-indexed)
    pub iteration: usize,
    /// Duration in milliseconds
    pub duration_ms: f64,
    /// Whether the execution succeeded
    pub success: bool,
    /// ISO 8601 timestamp
    pub timestamp: String,
}

/// Aggregated statistics for a benchmark case
#[derive(Debug, Clone)]
pub struct BenchmarkStats {
    /// Name of the benchmark case
    pub case_name: String,
    /// Number of iterations
    pub iterations: usize,
    /// Mean duration in milliseconds
    pub mean_ms: f64,
    /// Median duration in milliseconds
    pub median_ms: f64,
    /// Minimum duration in milliseconds
    pub min_ms: f64,
    /// Maximum duration in milliseconds
    pub max_ms: f64,
    /// Standard deviation in milliseconds
    pub stddev_ms: f64,
    /// Number of failed iterations
    pub failures: usize,
}

/// Append a measurement to a JSONL file
///
/// Manually constructs a JSON string (no serde) and appends it to the file
/// with a newline separator.
///
/// # Arguments
///
/// * `file` - Open file handle to append to
/// * `measurement` - The measurement to write
///
/// # Returns
///
/// Ok(()) on success, or an error message on failure.
pub fn append_jsonl(file: &mut File, measurement: &BenchmarkMeasurement) -> Result<(), String> {
    let json = format_measurement_json(measurement);
    writeln!(file, "{}", json).map_err(|e| format!("Failed to write JSONL line: {}", e))
}

/// Format a measurement as a JSON string
///
/// WHAT: Manually constructs JSON without serde dependency
/// WHY: Design requirement to use only stdlib
fn format_measurement_json(m: &BenchmarkMeasurement) -> String {
    format!(
        r#"{{"case_name":"{}","iteration":{},"duration_ms":{:.2},"success":{},"timestamp":"{}"}}"#,
        escape_json_string(&m.case_name),
        m.iteration,
        m.duration_ms,
        m.success,
        escape_json_string(&m.timestamp)
    )
}

/// Escape special characters in JSON strings
///
/// WHAT: Escapes quotes, backslashes, newlines, tabs for JSON compliance
/// WHY: Ensures valid JSON output without external dependencies
fn escape_json_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 10);
    for ch in s.chars() {
        match ch {
            '"' => result.push_str(r#"\""#),
            '\\' => result.push_str(r"\\"),
            '\n' => result.push_str(r"\n"),
            '\r' => result.push_str(r"\r"),
            '\t' => result.push_str(r"\t"),
            _ => result.push(ch),
        }
    }
    result
}

/// Generate an ISO 8601 timestamp from SystemTime
///
/// WHAT: Converts SystemTime to ISO 8601 format string
/// WHY: Provides consistent timestamp format for JSONL records
pub fn generate_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("System time before UNIX epoch");

    let secs = now.as_secs();
    let nanos = now.subsec_nanos();

    // Convert to ISO 8601 format: YYYY-MM-DDTHH:MM:SS.nnnnnnnnnZ
    // Simplified version - converts seconds since epoch to datetime
    let days_since_epoch = secs / 86400;
    let seconds_today = secs % 86400;

    let hours = seconds_today / 3600;
    let minutes = (seconds_today % 3600) / 60;
    let seconds = seconds_today % 60;

    // Approximate year/month/day calculation
    // WHAT: Simple epoch-to-date conversion
    // WHY: Avoid external datetime dependencies
    let year = 1970 + (days_since_epoch / 365);
    let day_of_year = days_since_epoch % 365;
    let month = (day_of_year / 30) + 1;
    let day = (day_of_year % 30) + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}Z",
        year, month, day, hours, minutes, seconds, nanos
    )
}

/// Generate a directory-safe timestamp for results folder
///
/// Format: YYYY-MM-DD_HH-MM-SS
pub fn generate_directory_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("System time before UNIX epoch");

    let secs = now.as_secs();
    let days_since_epoch = secs / 86400;
    let seconds_today = secs % 86400;

    let hours = seconds_today / 3600;
    let minutes = (seconds_today % 3600) / 60;
    let seconds = seconds_today % 60;

    let year = 1970 + (days_since_epoch / 365);
    let day_of_year = days_since_epoch % 365;
    let month = (day_of_year / 30) + 1;
    let day = (day_of_year % 30) + 1;

    format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
        year, month, day, hours, minutes, seconds
    )
}

/// Calculate mean of a slice of values
pub fn calculate_mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Calculate median of a slice of values
///
/// WHAT: Sorts values and returns middle element (or average of two middle)
/// WHY: Median is more robust to outliers than mean
pub fn calculate_median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

/// Calculate standard deviation of a slice of values
pub fn calculate_stddev(values: &[f64], mean: f64) -> f64 {
    if values.len() <= 1 {
        return 0.0;
    }
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}

/// Calculate statistics from a list of measurements
///
/// Groups measurements by case name and computes mean, median, min, max,
/// standard deviation, and failure count.
pub fn calculate_stats(measurements: &[BenchmarkMeasurement]) -> BenchmarkStats {
    if measurements.is_empty() {
        return BenchmarkStats {
            case_name: String::new(),
            iterations: 0,
            mean_ms: 0.0,
            median_ms: 0.0,
            min_ms: 0.0,
            max_ms: 0.0,
            stddev_ms: 0.0,
            failures: 0,
        };
    }

    let mut durations: Vec<f64> = measurements.iter().map(|m| m.duration_ms).collect();

    let mean = calculate_mean(&durations);
    let median = calculate_median(&mut durations);
    let min = durations.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = durations.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let stddev = calculate_stddev(&durations, mean);
    let failures = measurements.iter().filter(|m| !m.success).count();

    BenchmarkStats {
        case_name: measurements[0].case_name.clone(),
        iterations: measurements.len(),
        mean_ms: mean,
        median_ms: median,
        min_ms: min,
        max_ms: max,
        stddev_ms: stddev,
        failures,
    }
}

/// Write a Markdown summary report
///
/// Creates a Markdown table with benchmark statistics and metadata.
///
/// # Arguments
///
/// * `path` - Path to write the summary.md file
/// * `stats` - Vector of statistics for each benchmark case
/// * `mode` - Mode name (full/quick/ci)
/// * `timestamp` - Human-readable timestamp for the run
///
/// # Returns
///
/// Ok(()) on success, or an error message on failure.
pub fn write_summary(
    path: &Path,
    stats: &[BenchmarkStats],
    mode: &str,
    timestamp: &str,
) -> Result<(), String> {
    let mut content = String::new();

    // Header
    content.push_str("# Benchmark Results\n\n");
    content.push_str(&format!("**Run**: {}  \n", timestamp));
    content.push_str(&format!("**Mode**: {}\n\n", mode));

    // Table header
    content.push_str("| Case | Iterations | Mean (ms) | Median (ms) | Min (ms) | Max (ms) | StdDev (ms) | Failures |\n");
    content.push_str("|------|------------|-----------|-------------|----------|----------|-------------|----------|\n");

    // Table rows
    for stat in stats {
        content.push_str(&format!(
            "| {} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {} |\n",
            stat.case_name,
            stat.iterations,
            stat.mean_ms,
            stat.median_ms,
            stat.min_ms,
            stat.max_ms,
            stat.stddev_ms,
            stat.failures
        ));
    }

    // Footer
    let total_cases = stats.len();
    let total_failures: usize = stats.iter().map(|s| s.failures).sum();
    content.push_str(&format!(
        "\n**Total**: {} cases, {} failures\n",
        total_cases, total_failures
    ));

    // Write to file
    std::fs::write(path, content)
        .map_err(|e| format!("Failed to write summary to '{}': {}", path.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_json_string() {
        assert_eq!(escape_json_string("simple"), "simple");
        assert_eq!(escape_json_string(r#"with "quotes""#), r#"with \"quotes\""#);
        assert_eq!(escape_json_string("with\\backslash"), r"with\\backslash");
        assert_eq!(escape_json_string("with\nnewline"), r"with\nnewline");
        assert_eq!(escape_json_string("with\ttab"), r"with\ttab");
    }

    #[test]
    fn test_format_measurement_json() {
        let measurement = BenchmarkMeasurement {
            case_name: "test_case".to_string(),
            iteration: 1,
            duration_ms: 123.45,
            success: true,
            timestamp: "2024-01-15T14:30:00Z".to_string(),
        };

        let json = format_measurement_json(&measurement);
        assert!(json.contains(r#""case_name":"test_case""#));
        assert!(json.contains(r#""iteration":1"#));
        assert!(json.contains(r#""duration_ms":123.45"#));
        assert!(json.contains(r#""success":true"#));
    }

    #[test]
    fn test_calculate_mean() {
        assert_eq!(calculate_mean(&[100.0, 200.0, 300.0]), 200.0);
        assert_eq!(calculate_mean(&[]), 0.0);
        assert_eq!(calculate_mean(&[42.0]), 42.0);
    }

    #[test]
    fn test_calculate_median() {
        let mut values = vec![100.0, 200.0, 300.0];
        assert_eq!(calculate_median(&mut values), 200.0);

        let mut values = vec![100.0, 200.0, 300.0, 400.0];
        assert_eq!(calculate_median(&mut values), 250.0);

        let mut values = vec![];
        assert_eq!(calculate_median(&mut values), 0.0);
    }

    #[test]
    fn test_calculate_stddev() {
        let values = vec![100.0, 200.0, 300.0];
        let mean = calculate_mean(&values);
        let stddev = calculate_stddev(&values, mean);
        assert!((stddev - 81.65).abs() < 0.1); // Approximately 81.65

        let values = vec![100.0];
        let mean = calculate_mean(&values);
        let stddev = calculate_stddev(&values, mean);
        assert_eq!(stddev, 0.0);
    }

    #[test]
    fn test_calculate_stats() {
        let measurements = vec![
            BenchmarkMeasurement {
                case_name: "test".to_string(),
                iteration: 1,
                duration_ms: 100.0,
                success: true,
                timestamp: "2024-01-15T14:30:00Z".to_string(),
            },
            BenchmarkMeasurement {
                case_name: "test".to_string(),
                iteration: 2,
                duration_ms: 200.0,
                success: true,
                timestamp: "2024-01-15T14:30:01Z".to_string(),
            },
            BenchmarkMeasurement {
                case_name: "test".to_string(),
                iteration: 3,
                duration_ms: 300.0,
                success: false,
                timestamp: "2024-01-15T14:30:02Z".to_string(),
            },
        ];

        let stats = calculate_stats(&measurements);
        assert_eq!(stats.case_name, "test");
        assert_eq!(stats.iterations, 3);
        assert_eq!(stats.mean_ms, 200.0);
        assert_eq!(stats.median_ms, 200.0);
        assert_eq!(stats.min_ms, 100.0);
        assert_eq!(stats.max_ms, 300.0);
        assert_eq!(stats.failures, 1);
    }
}
