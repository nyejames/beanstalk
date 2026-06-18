//! Profile artifact layout and file writing
//!
//! WHAT: Manages the local directory structure for one profiling run and
//! provides helpers to write per-case artifacts (stdout/stderr logs,
//! detailed observations JSON, per-case summary) and root artifacts
//! (run-manifest.json, index.md).
//!
//! WHY: Keeping artifact paths and writers in one module ensures the
//! directory layout stays consistent across all profiling phases and
//! that JSON formatting is centralized rather than scattered across
//! orchestrators and parsers.
//!
//! # What this module owns
//! - `ProfileRunPaths` for the root run directory and manifest
//! - `ProfileCasePaths` for per-case subdirectories
//! - Helpers to write stdout.log, stderr.log, detailed-observations.json,
//!   run-manifest.json, index.md, and per-case hotspots.json
//!
//! # What this module does NOT own
//! - Observation data collection (see `observations.rs`)
//! - Samply runner integration (see `runner.rs`)
//! - Profile JSON parsing or hotspot extraction (see `parse.rs`, `hotspots.rs`)
//! - Agent summaries, enriched per-case summaries, and hint generation
//!   (see `summary.rs`)

use crate::bench_history::json_escape;
use crate::bench_time::BenchmarkTimestamp;
use crate::bench_types::BenchmarkMetric;
use std::fs;
use std::path::{Path, PathBuf};

use super::hotspots::HotspotExtractionResult;
use super::observations::ProfileObservation;
use super::options::ProfileFilterMode;

/// Current on-disk format version for profiling artifacts.
const FORMAT_VERSION: u32 = 1;

/// All paths for one profiling run.
///
/// WHAT: Owns the root run directory path and provides accessors for
/// root-level artifacts.
/// WHY: A single struct prevents path inconsistencies between manifest
/// writing, index generation, and per-case artifact creation.
pub(crate) struct ProfileRunPaths {
    /// Unique run identifier: `<timestamp>-<commit-or-unknown>`.
    pub(crate) run_id: String,
    /// Root directory for this run: `benchmarks/local-data/profiles/<run-id>/`.
    pub(crate) root: PathBuf,
}

/// All paths for one case within a profiling run.
///
/// WHAT: Owns the per-case subdirectory and provides accessors for
/// stdout, stderr, observations, and summary artifacts.
/// WHY: Per-case path construction is repeated for every case; a struct
/// keeps the layout deterministic and testable.
pub(crate) struct ProfileCasePaths {
    /// Case subdirectory: `<run-root>/cases/<case-name>/`.
    pub(crate) case_dir: PathBuf,
    /// Path to stdout.log within the case directory.
    pub(crate) stdout_log: PathBuf,
    /// Path to stderr.log within the case directory.
    pub(crate) stderr_log: PathBuf,
    /// Path to detailed-observations.json within the case directory.
    pub(crate) observations_json: PathBuf,
    /// Path to summary.md within the case directory.
    pub(crate) summary_md: PathBuf,
    /// Path to profile.json.gz (Samply output) within the case directory.
    ///
    /// Samply 0.13.1 writes gzip-compressed profiles regardless of the
    /// requested extension. We use `.json.gz` to match the actual format.
    pub(crate) profile_json: PathBuf,
    /// Path to hotspots.json within the case directory.
    ///
    /// Written by Phase 4 after parsing the Samply profile. Contains
    /// ranked function hotspots with percentage and millisecond estimates.
    pub(crate) hotspots_json: PathBuf,
}

/// Manifest data for one case, written into the root run-manifest.json.
///
/// WHAT: Compact representation of one case's artifacts and timing.
/// WHY: The manifest is a machine-readable index of the run so later
/// phases can discover cases without scanning directories.
pub(crate) struct ProfileCaseManifest {
    pub(crate) case_name: String,
    pub(crate) group_name: String,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) observation_wall_ms: f64,
    pub(crate) profile_path: String,
    pub(crate) stdout_path: String,
    pub(crate) stderr_path: String,
    pub(crate) summary_path: String,
}

impl ProfileRunPaths {
    /// Create a new run directory with a unique run id.
    ///
    /// The run id is `<YYYY-MM-DDThh-mm>-<short-commit-or-unknown>`.
    /// Creates the root directory and `cases/` subdirectory.
    pub(crate) fn create(profiles_root: &Path, commit: Option<&str>) -> Result<Self, String> {
        let ts = BenchmarkTimestamp::now();
        let commit_label = commit.unwrap_or("unknown");

        // Include seconds in the run id so back-to-back runs do not collide
        // when the same commit is profiled more than once in a minute.
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs() % 60)
            .unwrap_or(0);

        let run_id = format!(
            "{:04}-{:02}-{:02}T{:02}-{:02}-{:02}-{}",
            ts.year, ts.month, ts.day, ts.hour, ts.minute, seconds, commit_label
        );

        let root = profiles_root.join(&run_id);

        fs::create_dir_all(root.join("cases")).map_err(|e| {
            format!(
                "Failed to create profile run directory '{}': {}",
                root.display(),
                e
            )
        })?;

        Ok(Self { run_id, root })
    }

    /// Return the path for the root run-manifest.json.
    pub(crate) fn manifest_path(&self) -> PathBuf {
        self.root.join("run-manifest.json")
    }

    /// Return the path for the root index.md.
    pub(crate) fn index_path(&self) -> PathBuf {
        self.root.join("index.md")
    }

    /// Build case paths for a given case name.
    pub(crate) fn case_paths(&self, case_name: &str) -> ProfileCasePaths {
        let case_dir = self.root.join("cases").join(case_name);
        ProfileCasePaths {
            case_dir: case_dir.clone(),
            stdout_log: case_dir.join("stdout.log"),
            stderr_log: case_dir.join("stderr.log"),
            observations_json: case_dir.join("detailed-observations.json"),
            summary_md: case_dir.join("summary.md"),
            profile_json: case_dir.join("profile.json.gz"),
            hotspots_json: case_dir.join("hotspots.json"),
        }
    }
}

impl ProfileCasePaths {
    /// Create the case subdirectory on disk.
    pub(crate) fn create_dir(&self) -> Result<(), String> {
        fs::create_dir_all(&self.case_dir).map_err(|e| {
            format!(
                "Failed to create case directory '{}': {}",
                self.case_dir.display(),
                e
            )
        })
    }

    /// Write stdout content to stdout.log.
    pub(crate) fn write_stdout(&self, content: &str) -> Result<(), String> {
        fs::write(&self.stdout_log, content).map_err(|e| {
            format!(
                "Failed to write stdout.log '{}': {}",
                self.stdout_log.display(),
                e
            )
        })
    }

    /// Write stderr content to stderr.log.
    pub(crate) fn write_stderr(&self, content: &str) -> Result<(), String> {
        fs::write(&self.stderr_log, content).map_err(|e| {
            format!(
                "Failed to write stderr.log '{}': {}",
                self.stderr_log.display(),
                e
            )
        })
    }

    /// Write detailed-observations.json from a profile observation.
    pub(crate) fn write_observations_json(
        &self,
        observation: &ProfileObservation,
    ) -> Result<(), String> {
        let json = format_observations_json(observation);
        fs::write(&self.observations_json, json).map_err(|e| {
            format!(
                "Failed to write detailed-observations.json '{}': {}",
                self.observations_json.display(),
                e
            )
        })
    }
}

/// Write per-case hotspots.json from a hotspot extraction result.
///
/// WHAT: Serializes the hotspot extraction output as compact JSON and
/// writes it to the case directory.
///
/// WHY: `hotspots.json` is the machine-readable per-case hotspot data
/// that agents read to identify optimization targets. Using serde_json
/// ensures correct JSON escaping and formatting.
pub(crate) fn write_hotspots_json(
    case_paths: &ProfileCasePaths,
    result: &HotspotExtractionResult,
) -> Result<(), String> {
    let json = format_hotspots_json(result);
    fs::write(&case_paths.hotspots_json, json).map_err(|e| {
        format!(
            "Failed to write hotspots.json '{}': {}",
            case_paths.hotspots_json.display(),
            e
        )
    })
}

/// Write the root run-manifest.json containing all case manifests.
pub(crate) fn write_run_manifest(
    run_paths: &ProfileRunPaths,
    run_id: &str,
    commit: Option<&str>,
    filter: ProfileFilterMode,
    samply_rate_hz: Option<f64>,
    cases: &[ProfileCaseManifest],
) -> Result<(), String> {
    let json = format_run_manifest_json(run_id, commit, filter, samply_rate_hz, cases);
    fs::write(run_paths.manifest_path(), json).map_err(|e| {
        format!(
            "Failed to write run-manifest.json '{}': {}",
            run_paths.manifest_path().display(),
            e
        )
    })
}

/// Write the root index.md summarizing the profiling run.
pub(crate) fn write_index_md(
    run_paths: &ProfileRunPaths,
    run_id: &str,
    filter: ProfileFilterMode,
    cases: &[ProfileCaseManifest],
) -> Result<(), String> {
    let md = format_index_md(run_id, filter, cases);
    fs::write(run_paths.index_path(), md).map_err(|e| {
        format!(
            "Failed to write index.md '{}': {}",
            run_paths.index_path().display(),
            e
        )
    })
}

// ---------------------------------------------------------------------------
//  JSON formatting
// ---------------------------------------------------------------------------

/// Format detailed-observations.json for a single case.
///
/// Uses the manual JSON approach matching `bench_history.rs` so xtask
/// remains std-only until Phase 4 adds `serde_json`.
fn format_observations_json(observation: &ProfileObservation) -> String {
    let stage_timings_json = format_metric_array_json(&observation.observations.stage_timings);
    let counters_json = format_metric_array_json(&observation.observations.counters);

    // The command array includes the command as the first element, followed by args.
    let mut command_parts = vec![format!("\"{}\"", json_escape(&observation.command))];
    for arg in &observation.command_args {
        command_parts.push(format!("\"{}\"", json_escape(arg)));
    }
    let command_json = command_parts.join(",");

    format!(
        "{{\n  \"format_version\": {},\n  \"case\": \"{}\",\n  \"group\": \"{}\",\n  \"command\": [{}],\n  \"wall_ms\": {},\n  \"stage_timings\": {},\n  \"counters\": {}\n}}",
        FORMAT_VERSION,
        json_escape(&observation.case_name),
        json_escape(&observation.group_name),
        command_json,
        observation.wall_ms,
        stage_timings_json,
        counters_json,
    )
}

/// Format a slice of metrics as a JSON array of `{name, value}` objects.
fn format_metric_array_json(metrics: &[BenchmarkMetric]) -> String {
    if metrics.is_empty() {
        return "[]".to_string();
    }

    let items: Vec<String> = metrics
        .iter()
        .map(|metric| {
            format!(
                "    {{\"name\": \"{}\", \"value\": {}}}",
                json_escape(&metric.name),
                metric.value
            )
        })
        .collect();

    format!("[\n{}\n  ]", items.join(",\n"))
}

/// Format run-manifest.json as manual JSON.
fn format_run_manifest_json(
    run_id: &str,
    commit: Option<&str>,
    filter: ProfileFilterMode,
    samply_rate_hz: Option<f64>,
    cases: &[ProfileCaseManifest],
) -> String {
    let commit_json = match commit {
        Some(c) => format!("\"{}\"", json_escape(c)),
        None => "null".to_string(),
    };

    let samply_json = match samply_rate_hz {
        Some(rate) => format!("{}", rate),
        None => "null".to_string(),
    };

    let cases_json: Vec<String> = cases
        .iter()
        .map(|case| {
            let args_json = case
                .args
                .iter()
                .map(|a| format!("\"{}\"", json_escape(a)))
                .collect::<Vec<_>>()
                .join(",");

            format!(
                "    {{\n      \"case_name\": \"{}\",\n      \"group_name\": \"{}\",\n      \"command\": \"{}\",\n      \"args\": [{}],\n      \"observation_wall_ms\": {},\n      \"profile_path\": \"{}\",\n      \"stdout_path\": \"{}\",\n      \"stderr_path\": \"{}\",\n      \"summary_path\": \"{}\"\n    }}",
                json_escape(&case.case_name),
                json_escape(&case.group_name),
                json_escape(&case.command),
                args_json,
                case.observation_wall_ms,
                json_escape(&case.profile_path),
                json_escape(&case.stdout_path),
                json_escape(&case.stderr_path),
                json_escape(&case.summary_path),
            )
        })
        .collect();

    format!(
        "{{\n  \"format_version\": {},\n  \"run_id\": \"{}\",\n  \"timestamp\": \"{}\",\n  \"commit\": {},\n  \"filter\": \"{}\",\n  \"samply_rate_hz\": {},\n  \"cases\": [\n{}\n  ]\n}}",
        FORMAT_VERSION,
        json_escape(run_id),
        json_escape(&BenchmarkTimestamp::now().format_run_header()),
        commit_json,
        filter.display_label(),
        samply_json,
        cases_json.join(",\n"),
    )
}

/// Format hotspots.json for a single case using serde_json.
///
/// WHAT: Serializes the hotspot extraction output as compact JSON with
/// ranked functions, percentages, estimated milliseconds, owner buckets,
/// caller/callee edges, and warnings.
///
/// WHY: Using serde_json ensures correct JSON escaping and consistent
/// formatting. The output is designed to be compact enough for agent
/// consumption without post-processing.
fn format_hotspots_json(result: &HotspotExtractionResult) -> String {
    let functions_json: Vec<serde_json::Value> = result
        .functions
        .iter()
        .map(|func| {
            let callers_json = format_edges_json(&func.top_callers);
            let callees_json = format_edges_json(&func.top_callees);

            serde_json::json!({
                "name": func.name,
                "bucket": {
                    "label": func.bucket.label,
                    "suggested_paths": func.bucket.suggested_paths,
                },
                "inclusive_samples": func.inclusive_samples,
                "self_samples": func.self_samples,
                "inclusive_pct": round_2dp(func.inclusive_pct),
                "self_pct": round_2dp(func.self_pct),
                "estimated_inclusive_ms": round_2dp(func.estimated_inclusive_ms),
                "estimated_self_ms": round_2dp(func.estimated_self_ms),
                "top_callers": callers_json,
                "top_callees": callees_json,
            })
        })
        .collect();

    let output = serde_json::json!({
        "format_version": FORMAT_VERSION,
        "total_sample_count": result.total_sample_count,
        "total_sample_weight": round_2dp(result.total_sample_weight),
        "wall_time_ms": round_2dp(result.wall_time_ms),
        "hot_function_count": result.functions.len(),
        "functions": functions_json,
        "warnings": result.warnings,
    });

    serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
}

/// Format a slice of profile edges as serde_json values.
fn format_edges_json(edges: &[super::parse::ProfileEdge]) -> Vec<serde_json::Value> {
    edges
        .iter()
        .map(|edge| {
            serde_json::json!({
                "function_name": edge.function_name,
                "samples": round_2dp(edge.samples),
                "pct": round_2dp(edge.pct),
            })
        })
        .collect()
}

/// Round a floating-point value to 2 decimal places.
fn round_2dp(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

// ---------------------------------------------------------------------------
//  Markdown formatting
// ---------------------------------------------------------------------------

/// Format the root index.md for a profiling run.
fn format_index_md(
    run_id: &str,
    filter: ProfileFilterMode,
    cases: &[ProfileCaseManifest],
) -> String {
    let mut lines = Vec::new();

    lines.push(format!("# Profiling run: {}", run_id));
    lines.push(String::new());
    lines.push(format!("Filter: {}", filter.display_label()));
    lines.push(format!("Cases: {}", cases.len()));
    lines.push(String::new());

    if !cases.is_empty() {
        lines.push("## Cases".to_string());
        lines.push(String::new());
        for case in cases {
            lines.push(format!(
                "- **{}** — `{}` (~{:.0}ms)",
                case.case_name,
                format_command(&case.command, &case.args),
                case.observation_wall_ms,
            ));
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

/// Format a command with args for display.
fn format_command(command: &str, args: &[String]) -> String {
    let mut parts = vec![command.to_string()];
    parts.extend(args.iter().cloned());
    parts.join(" ")
}

// ---------------------------------------------------------------------------
//  Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;
