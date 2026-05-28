//! Benchmark domain types - Core data structures for benchmark results
//!
//! This module provides named structs for benchmark measurements, statistics,
//! and comparisons. It replaces tuple-heavy APIs with explicit types that
//! document the meaning of each field.

/// Distinguishes the two benchmark suite kinds so local history and summaries
/// do not accidentally compare incompatible metrics.
///
/// WHAT: CLI subprocess wall-clock time vs in-process frontend stage time.
/// WHY: Prevents a frontend refactor from being compared against CLI spawn
///      overhead, and vice versa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkSuiteKind {
    /// End-to-end CLI benchmark measuring subprocess wall-clock time.
    EndToEndCli,
    /// In-process frontend benchmark measuring compiler stage timings.
    FrontendPhases,
}

impl BenchmarkSuiteKind {
    /// Parse a persisted suite kind from local JSONL records.
    pub fn from_persisted_name(name: &str) -> Option<Self> {
        match name {
            "end_to_end_cli" => Some(BenchmarkSuiteKind::EndToEndCli),
            "frontend_phases" => Some(BenchmarkSuiteKind::FrontendPhases),
            _ => None,
        }
    }

    /// Persistent string used in local JSONL records.
    pub fn persisted_name(&self) -> &'static str {
        match self {
            BenchmarkSuiteKind::EndToEndCli => "end_to_end_cli",
            BenchmarkSuiteKind::FrontendPhases => "frontend_phases",
        }
    }

    /// Human-readable display label used in summaries and terminal output.
    pub fn display_label(&self) -> &'static str {
        match self {
            BenchmarkSuiteKind::EndToEndCli => "End-to-end CLI",
            BenchmarkSuiteKind::FrontendPhases => "Frontend phases",
        }
    }

    /// Primary metric name for this suite kind.
    pub fn primary_metric_name(&self) -> &'static str {
        match self {
            BenchmarkSuiteKind::EndToEndCli => "wall_time_ms",
            BenchmarkSuiteKind::FrontendPhases => "frontend_total_ms",
        }
    }
}

/// A single benchmark case result after measured iterations.
#[derive(Debug, Clone)]
pub struct BenchmarkCaseResult {
    /// Name of the benchmark case.
    pub case_name: String,
    /// Public grouping used by summaries to give absolute context.
    pub group_name: String,
    /// The command executed (e.g., "check", "build").
    pub command: String,
    /// Arguments passed to the command.
    pub args: Vec<String>,
    /// Mean duration in milliseconds across measured iterations.
    pub mean_ms: f64,
    /// Median duration in milliseconds across measured iterations.
    pub median_ms: f64,
    /// Standard deviation in milliseconds across measured iterations.
    pub stddev_ms: f64,
    /// Local-only detailed timer and counter observations parsed from stdout.
    pub observations: BenchmarkCaseObservations,
}

/// One named timing or counter value captured from detailed compiler output.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BenchmarkMetric {
    pub name: String,
    pub value: f64,
}

/// Local-only detailed observations for one benchmark case.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BenchmarkCaseObservations {
    pub stage_timings: Vec<BenchmarkMetric>,
    pub counters: Vec<BenchmarkMetric>,
}

/// Aggregated statistics for one benchmark group.
///
/// Groups are deliberately simple summary buckets, not compiler-stage
/// categories. They make public benchmark output easier to compare without
/// committing per-case timing tables.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkGroupStats {
    /// Public group label.
    pub group_name: String,
    /// Number of cases in this group.
    pub case_count: usize,
    /// Average of per-case means in milliseconds.
    pub average_ms: f64,
}

/// Aggregated statistics for the entire benchmark suite.
///
/// WHAT: Summarises per-case means into a single average and case spread.
/// WHY: The spread is across heterogeneous benchmark cases, not statistical
/// measurement noise from repeated runs of the same case.
#[derive(Debug, Clone)]
pub struct SuiteStats {
    /// Average of per-case means in milliseconds.
    pub average_ms: f64,
    /// Standard deviation across per-case means in milliseconds.
    pub case_spread_ms: f64,
}

impl SuiteStats {
    /// Compute suite stats from a list of per-case results.
    ///
    /// WHAT: Extracts per-case means and computes the suite average plus
    /// cross-case spread.
    /// WHY: Naming the spread accurately prevents summary code from treating
    /// unrelated benchmark variety as repeated-measurement uncertainty.
    pub fn from_case_results(cases: &[BenchmarkCaseResult]) -> Self {
        let means: Vec<f64> = cases.iter().map(|c| c.mean_ms).collect();
        let average_ms = calculate_mean(&means);
        let case_spread_ms = calculate_stddev(&means, average_ms);

        Self {
            average_ms,
            case_spread_ms,
        }
    }
}

/// Classification of benchmark change relative to a previous run
///
/// WHAT: Named interpretation of whether a run changed meaningfully.
/// WHY: Run-level classification must be derived from case classifications so
/// mixed faster/slower movement cannot collapse into a misleading no-change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkChangeKind {
    /// No previous overlapping benchmark cases exist
    Baseline,
    /// Previous comparison exists but no overlapping case exceeded its threshold
    NoMeasurableChange,
    /// At least one case improved and no cases regressed
    Faster,
    /// At least one case regressed and no cases improved
    Slower,
    /// At least one case improved and at least one case regressed
    Mixed,
}

/// Classification of a single overlapping benchmark case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkCaseChangeKind {
    /// The case delta stayed within its local measured-variation threshold.
    NoMeasurableChange,
    /// The current case mean is meaningfully lower than the previous mean.
    Faster,
    /// The current case mean is meaningfully higher than the previous mean.
    Slower,
}

/// Named rough-threshold configuration for benchmark comparisons.
///
/// WHAT: Defines the absolute and relative floors used to classify case
/// and stage movement as meaningful.
/// WHY: Prevents magic constants from drifting across the comparison and
/// display code, and makes the threshold policy explicit and testable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BenchmarkThresholds {
    pub minimum_case_delta_ms: f64,
    pub minimum_case_delta_ratio: f64,
    pub stddev_multiplier: f64,
    pub minimum_stage_delta_ms: f64,
    pub minimum_stage_delta_ratio: f64,
}

impl BenchmarkThresholds {
    /// Default thresholds calibrated for rough compiler-development sanity checks.
    ///
    /// WHAT: Catches obvious movement without over-reporting subprocess noise.
    /// WHY: These values were chosen after observing typical CLI and frontend
    /// benchmark variation on development hardware.
    pub const DEFAULT: Self = Self {
        minimum_case_delta_ms: 2.0,
        minimum_case_delta_ratio: 0.03,
        stddev_multiplier: 2.0,
        minimum_stage_delta_ms: 1.0,
        minimum_stage_delta_ratio: 0.05,
    };
}

/// Comparison between one current case and its previous counterpart.
#[derive(Debug, Clone)]
pub struct BenchmarkCaseComparison {
    pub case_name: String,
    pub group_name: String,
    pub previous_mean_ms: f64,
    pub current_mean_ms: f64,
    pub delta_ms: f64,
    pub threshold_ms: f64,
    pub change_kind: BenchmarkCaseChangeKind,
    pub observations: BenchmarkObservationComparison,
}

/// Comparison between one current stage timing and its previous counterpart.
///
/// WHAT: Retains both previous and current absolute values so local
/// diagnostics and future debug tooling can inspect them.
/// WHY: The public summary only uses `delta_ms`, but local JSONL history
/// and future per-stage drill-down will need the raw values.
#[derive(Debug, Clone)]
pub struct BenchmarkStageComparison {
    pub stage_name: String,
    pub previous_ms: f64,
    pub current_ms: f64,
    pub delta_ms: f64,
    pub change_kind: BenchmarkCaseChangeKind,
}

/// Comparison between current and previous observations for one case.
#[derive(Debug, Clone)]
pub struct BenchmarkObservationComparison {
    pub stage_comparisons: Vec<BenchmarkStageComparison>,
}

/// Aggregated stage movement across all overlapping cases in a comparison.
#[derive(Debug, Clone)]
pub struct BenchmarkStageMovement {
    pub stage_name: String,
    pub total_delta_ms: f64,
    pub case_count: usize,
    pub faster_count: usize,
    pub slower_count: usize,
}

/// Aggregated comparison counts for a current benchmark group.
#[derive(Debug, Clone)]
pub struct BenchmarkGroupComparison {
    pub group_name: String,
    pub previous_average_ms: Option<f64>,
    pub current_average_ms: f64,
    pub delta_ms: Option<f64>,
    pub faster_count: usize,
    pub slower_count: usize,
    pub unchanged_count: usize,
}

/// Comparison between a current benchmark run and a previous one
///
/// WHAT: Computes per-case deltas, group counts, and the run-level
/// classification between two runs.
/// WHY: The displayed run result should report mixed movement honestly instead
/// of using spread across unrelated cases as a noise threshold.
#[derive(Debug, Clone)]
pub struct BenchmarkComparison {
    /// Overall mean change in milliseconds, or None if no previous run
    pub overall_mean_delta_ms: Option<f64>,
    /// Named classification of the change.
    pub change_kind: BenchmarkChangeKind,
    /// Number of current cases that had a previous case with the same name.
    pub compared_case_count: usize,
    /// Number of current cases.
    pub current_case_count: usize,
    /// Number of previous cases.
    pub previous_case_count: usize,
    /// Number of overlapping cases classified as faster.
    pub faster_case_count: usize,
    /// Number of overlapping cases classified as slower.
    pub slower_case_count: usize,
    /// Number of overlapping cases classified as unchanged.
    pub unchanged_case_count: usize,
    /// True when cases were added or removed between the two runs.
    pub case_set_changed: bool,
    /// Per-case comparisons for overlapping cases.
    pub cases: Vec<BenchmarkCaseComparison>,
    /// Per-current-group comparison counts and average movement.
    pub groups: Vec<BenchmarkGroupComparison>,
}

impl BenchmarkComparison {
    /// Compare current case results against an optional previous set.
    ///
    /// WHAT: Finds overlapping cases by name, classifies each case against its
    /// own measured-variation threshold, then derives run and group summaries.
    /// WHY: Per-case classification catches mixed movement and single-case
    /// regressions that a suite-level average can hide.
    ///
    /// If there are no overlapping cases with the previous run, or if no
    /// previous run is provided, the comparison reports baseline.
    pub fn new(current: &[BenchmarkCaseResult], previous: Option<&[BenchmarkCaseResult]>) -> Self {
        Self::new_with_thresholds(current, previous, &BenchmarkThresholds::DEFAULT)
    }

    /// Compare current case results with explicit threshold policy.
    ///
    /// WHAT: Lets tests and future xtask-only callers exercise threshold
    /// behavior without changing the normal benchmark command defaults.
    /// WHY: Threshold tuning is part of the benchmark domain, so exact policy
    /// tests should not depend on hidden constants.
    pub fn new_with_thresholds(
        current: &[BenchmarkCaseResult],
        previous: Option<&[BenchmarkCaseResult]>,
        thresholds: &BenchmarkThresholds,
    ) -> Self {
        let Some(previous_cases) = previous else {
            return Self::baseline(current.len(), 0, false, current, None);
        };

        let cases = compare_cases(current, previous_cases, thresholds);
        if cases.is_empty() {
            let case_set_changed = !current.is_empty() || !previous_cases.is_empty();
            return Self::baseline(
                current.len(),
                previous_cases.len(),
                case_set_changed,
                current,
                Some(previous_cases),
            );
        }
        debug_assert!(cases.iter().all(|case| {
            !case.case_name.is_empty()
                && case.previous_mean_ms.is_finite()
                && case.current_mean_ms.is_finite()
        }));

        let faster_case_count = cases
            .iter()
            .filter(|case| case.change_kind == BenchmarkCaseChangeKind::Faster)
            .count();
        let slower_case_count = cases
            .iter()
            .filter(|case| case.change_kind == BenchmarkCaseChangeKind::Slower)
            .count();
        let unchanged_case_count = cases
            .iter()
            .filter(|case| case.change_kind == BenchmarkCaseChangeKind::NoMeasurableChange)
            .count();

        let change_kind = classify_run_change(faster_case_count, slower_case_count);
        let deltas: Vec<f64> = cases.iter().map(|case| case.delta_ms).collect();
        let overall_mean_delta_ms = Some(calculate_mean(&deltas));
        let compared_case_count = cases.len();
        let current_case_count = current.len();
        let previous_case_count = previous_cases.len();
        let case_set_changed =
            compared_case_count != current_case_count || compared_case_count != previous_case_count;
        let groups = compare_groups(current, previous_cases, &cases);

        let comparison = Self {
            overall_mean_delta_ms,
            change_kind,
            compared_case_count,
            current_case_count,
            previous_case_count,
            faster_case_count,
            slower_case_count,
            unchanged_case_count,
            case_set_changed,
            cases,
            groups,
        };

        comparison.debug_assert_consistent();
        comparison
    }

    fn baseline(
        current_case_count: usize,
        previous_case_count: usize,
        case_set_changed: bool,
        current: &[BenchmarkCaseResult],
        previous: Option<&[BenchmarkCaseResult]>,
    ) -> Self {
        let groups = previous
            .map(|previous_cases| compare_groups(current, previous_cases, &[]))
            .unwrap_or_else(|| baseline_groups(current));

        let comparison = Self {
            overall_mean_delta_ms: None,
            change_kind: BenchmarkChangeKind::Baseline,
            compared_case_count: 0,
            current_case_count,
            previous_case_count,
            faster_case_count: 0,
            slower_case_count: 0,
            unchanged_case_count: 0,
            case_set_changed,
            cases: Vec::new(),
            groups,
        };

        comparison.debug_assert_consistent();
        comparison
    }

    /// Check comparison aggregates while keeping detailed fields live.
    ///
    /// WHAT: Validates that the public summary counts still agree with the
    /// retained per-case and per-group detail.
    /// WHY: The benchmark model intentionally stores detail beyond the terse
    /// monthly summary so future diagnostics can inspect cases without
    /// reparsing history.
    fn debug_assert_consistent(&self) {
        let case_count = self.cases.len();
        let classified_case_count =
            self.faster_case_count + self.slower_case_count + self.unchanged_case_count;

        debug_assert_eq!(case_count, self.compared_case_count);
        debug_assert_eq!(classified_case_count, self.compared_case_count);

        for case in &self.cases {
            debug_assert!(!case.case_name.trim().is_empty());
            debug_assert!(!case.group_name.trim().is_empty());
            debug_assert!(case.previous_mean_ms.is_finite());
            debug_assert!(case.current_mean_ms.is_finite());
            debug_assert!(case.delta_ms.is_finite());
            debug_assert!(case.threshold_ms >= 0.0 && case.threshold_ms.is_finite());

            for stage in &case.observations.stage_comparisons {
                debug_assert!(!stage.stage_name.trim().is_empty());
                debug_assert!(stage.previous_ms.is_finite());
                debug_assert!(stage.current_ms.is_finite());
                debug_assert!(stage.delta_ms.is_finite());
            }
        }

        let grouped_case_count: usize = self
            .groups
            .iter()
            .map(|group| {
                debug_assert!(!group.group_name.trim().is_empty());
                debug_assert!(group.current_average_ms.is_finite());

                if let Some(previous_average_ms) = group.previous_average_ms {
                    debug_assert!(previous_average_ms.is_finite());
                }

                if let Some(delta_ms) = group.delta_ms {
                    debug_assert!(delta_ms.is_finite());
                }

                group.faster_count + group.slower_count + group.unchanged_count
            })
            .sum();

        debug_assert_eq!(grouped_case_count, self.compared_case_count);
    }

    /// Format the run-entry summary line for display in monthly summaries.
    ///
    /// Returns:
    /// - "**baseline**; N cases" for the first run on a system.
    /// - "no measurable change: avg +0ms; N/N cases" when all shared cases
    ///   stayed within their thresholds.
    /// - terse faster/slower/mixed/case-set-changed lines otherwise.
    pub fn format_run_change_line(&self) -> String {
        if self.case_set_changed {
            return self.format_case_set_changed_line();
        }

        match self.change_kind {
            BenchmarkChangeKind::Baseline => {
                format!("**baseline**; {} cases", self.current_case_count)
            }
            BenchmarkChangeKind::NoMeasurableChange => {
                format!(
                    "no measurable change: avg {}; {}/{} cases",
                    format_signed_ms(self.overall_mean_delta_ms.unwrap_or(0.0)),
                    self.compared_case_count,
                    self.current_case_count
                )
            }
            BenchmarkChangeKind::Faster | BenchmarkChangeKind::Slower => {
                format!(
                    "**{} avg**; {} faster, {} slower; {}/{} cases",
                    format_signed_ms(self.overall_mean_delta_ms.unwrap_or(0.0)),
                    self.faster_case_count,
                    self.slower_case_count,
                    self.compared_case_count,
                    self.current_case_count
                )
            }
            BenchmarkChangeKind::Mixed => format!(
                "mixed: avg {}; {} faster, {} slower; {}/{} cases",
                format_signed_ms(self.overall_mean_delta_ms.unwrap_or(0.0)),
                self.faster_case_count,
                self.slower_case_count,
                self.compared_case_count,
                self.current_case_count
            ),
        }
    }

    fn format_case_set_changed_line(&self) -> String {
        if let Some(delta) = self.overall_mean_delta_ms {
            let shared_denominator = self.current_case_count.max(self.previous_case_count);

            format!(
                "case set changed: avg {} on {}/{} shared cases; {} slower, {} faster",
                format_signed_ms(delta),
                self.compared_case_count,
                shared_denominator,
                self.slower_case_count,
                self.faster_case_count
            )
        } else {
            format!(
                "case set changed: no shared cases; {} current, {} previous",
                self.current_case_count, self.previous_case_count
            )
        }
    }
}

fn compare_cases(
    current: &[BenchmarkCaseResult],
    previous: &[BenchmarkCaseResult],
    thresholds: &BenchmarkThresholds,
) -> Vec<BenchmarkCaseComparison> {
    let mut cases = Vec::new();

    for current_case in current {
        let Some(previous_case) = previous
            .iter()
            .find(|case| case.case_name == current_case.case_name)
        else {
            continue;
        };

        let delta_ms = current_case.mean_ms - previous_case.mean_ms;
        let threshold_ms = case_threshold_ms(current_case, previous_case, thresholds);
        let change_kind = classify_case_change(delta_ms, threshold_ms);
        let observations = compare_observations(
            &current_case.observations,
            &previous_case.observations,
            thresholds,
        );

        cases.push(BenchmarkCaseComparison {
            case_name: current_case.case_name.clone(),
            group_name: current_case.group_name.clone(),
            previous_mean_ms: previous_case.mean_ms,
            current_mean_ms: current_case.mean_ms,
            delta_ms,
            threshold_ms,
            change_kind,
            observations,
        });
    }

    cases
}

fn case_threshold_ms(
    current: &BenchmarkCaseResult,
    previous: &BenchmarkCaseResult,
    thresholds: &BenchmarkThresholds,
) -> f64 {
    let combined_stddev = (current.stddev_ms.powi(2) + previous.stddev_ms.powi(2)).sqrt();
    let stddev_component = combined_stddev * thresholds.stddev_multiplier;
    let ratio_component = previous.mean_ms * thresholds.minimum_case_delta_ratio;

    thresholds
        .minimum_case_delta_ms
        .max(ratio_component)
        .max(stddev_component)
}

fn classify_case_change(delta_ms: f64, threshold_ms: f64) -> BenchmarkCaseChangeKind {
    if delta_ms.abs() <= threshold_ms {
        BenchmarkCaseChangeKind::NoMeasurableChange
    } else if delta_ms < -threshold_ms {
        BenchmarkCaseChangeKind::Faster
    } else {
        BenchmarkCaseChangeKind::Slower
    }
}

fn classify_run_change(faster_count: usize, slower_count: usize) -> BenchmarkChangeKind {
    match (faster_count > 0, slower_count > 0) {
        (true, true) => BenchmarkChangeKind::Mixed,
        (true, false) => BenchmarkChangeKind::Faster,
        (false, true) => BenchmarkChangeKind::Slower,
        (false, false) => BenchmarkChangeKind::NoMeasurableChange,
    }
}

fn baseline_groups(current: &[BenchmarkCaseResult]) -> Vec<BenchmarkGroupComparison> {
    calculate_group_stats(current)
        .into_iter()
        .map(|group| BenchmarkGroupComparison {
            group_name: group.group_name,
            previous_average_ms: None,
            current_average_ms: group.average_ms,
            delta_ms: None,
            faster_count: 0,
            slower_count: 0,
            unchanged_count: 0,
        })
        .collect()
}

fn compare_groups(
    current: &[BenchmarkCaseResult],
    previous: &[BenchmarkCaseResult],
    compared_cases: &[BenchmarkCaseComparison],
) -> Vec<BenchmarkGroupComparison> {
    let current_groups = calculate_group_stats(current);
    let previous_groups = calculate_group_stats(previous);

    current_groups
        .into_iter()
        .map(|current_group| {
            let previous_average_ms = previous_groups
                .iter()
                .find(|group| group.group_name == current_group.group_name)
                .map(|group| group.average_ms);
            let delta_ms = previous_average_ms
                .map(|previous_average| current_group.average_ms - previous_average);
            let faster_count = count_group_cases(
                compared_cases,
                &current_group.group_name,
                BenchmarkCaseChangeKind::Faster,
            );
            let slower_count = count_group_cases(
                compared_cases,
                &current_group.group_name,
                BenchmarkCaseChangeKind::Slower,
            );
            let unchanged_count = count_group_cases(
                compared_cases,
                &current_group.group_name,
                BenchmarkCaseChangeKind::NoMeasurableChange,
            );

            BenchmarkGroupComparison {
                group_name: current_group.group_name,
                previous_average_ms,
                current_average_ms: current_group.average_ms,
                delta_ms,
                faster_count,
                slower_count,
                unchanged_count,
            }
        })
        .collect()
}

fn count_group_cases(
    cases: &[BenchmarkCaseComparison],
    group_name: &str,
    change_kind: BenchmarkCaseChangeKind,
) -> usize {
    cases
        .iter()
        .filter(|case| case.group_name == group_name && case.change_kind == change_kind)
        .count()
}

fn format_signed_ms(value: f64) -> String {
    let rounded = value.round() as i64;
    if rounded > 0 {
        format!("+{}ms", rounded)
    } else {
        format!("{}ms", rounded)
    }
}

/// Describes the local system that ran the benchmark
///
/// WHAT: Privacy-safe identity for a single machine/clone
/// WHY: Allows per-system tracking without exposing machine-derived identifiers
#[derive(Debug, Clone)]
pub struct BenchmarkSystem {
    /// Stable private UUID for this clone (local-only)
    pub system_uuid: String,
    /// Short public hex identifier shown in summaries
    pub public_system_id: String,
    /// Human-readable display name (e.g., "macOS M1")
    pub display_name: String,
}

/// A complete recorded benchmark run
///
/// WHAT: All data from one full benchmark execution
/// WHY: Stored in local raw history and used to generate summaries
#[derive(Debug, Clone)]
pub struct BenchmarkRun {
    /// Timestamp when the run started
    pub timestamp: crate::bench_time::BenchmarkTimestamp,
    /// Short git commit hash, if available
    pub commit: Option<String>,
    /// System that performed the run
    pub system: BenchmarkSystem,
    /// Which benchmark suite kind this run belongs to
    pub suite_kind: BenchmarkSuiteKind,
    /// Per-case results
    pub cases: Vec<BenchmarkCaseResult>,
    /// Aggregated statistics per public benchmark group.
    pub groups: Vec<BenchmarkGroupStats>,
    /// Aggregated suite statistics
    pub suite: SuiteStats,
    /// Number of warmup runs performed before measurement
    pub warmup_runs: usize,
    /// Number of measured iterations used for each case
    pub measured_iterations: usize,
}

/// Calculate mean of a slice of values
pub fn calculate_mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Calculate median of a slice of values.
///
/// WHAT: Sorts a local copy and returns the middle value, or the average of
/// the two middle values for even-sized inputs.
/// WHY: Benchmark summaries will keep mean as the primary public average, but
/// median is useful local raw data for judging noisy subprocess measurements.
pub fn calculate_median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mut sorted_values = values.to_vec();
    sorted_values.sort_by(|left, right| left.total_cmp(right));

    let middle = sorted_values.len() / 2;
    if sorted_values.len() % 2 == 1 {
        sorted_values[middle]
    } else {
        let left = sorted_values[middle - 1];
        let right = sorted_values[middle];
        (left + right) / 2.0
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

/// Calculate average timing per benchmark group.
///
/// WHAT: Groups case means by their public group label and returns stable
/// summary records.
/// WHY: Later summary phases need absolute group context without duplicating
/// grouping and ordering logic in render code.
pub fn calculate_group_stats(cases: &[BenchmarkCaseResult]) -> Vec<BenchmarkGroupStats> {
    let mut groups: Vec<BenchmarkGroupStatsBuilder> = Vec::new();

    for case in cases {
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.group_name == case.group_name)
        {
            group.case_means.push(case.mean_ms);
        } else {
            groups.push(BenchmarkGroupStatsBuilder {
                group_name: case.group_name.clone(),
                case_means: vec![case.mean_ms],
            });
        }
    }

    let mut stats: Vec<BenchmarkGroupStats> = groups
        .into_iter()
        .map(|group| {
            let average_ms = calculate_mean(&group.case_means);

            BenchmarkGroupStats {
                group_name: group.group_name,
                case_count: group.case_means.len(),
                average_ms,
            }
        })
        .collect();

    stats.sort_by(|left, right| {
        group_sort_key(&left.group_name).cmp(&group_sort_key(&right.group_name))
    });

    stats
}

struct BenchmarkGroupStatsBuilder {
    group_name: String,
    case_means: Vec<f64>,
}

fn group_sort_key(group_name: &str) -> (usize, &str) {
    match group_name {
        "core" => (0, group_name),
        "docs" => (1, group_name),
        "stress" => (2, group_name),
        "module" => (3, group_name),
        "borrow" => (4, group_name),
        _ => (usize::MAX, group_name),
    }
}

/// Compare current and previous observations for overlapping stage timings.
///
/// WHAT: Finds stage metrics present in both current and previous observations,
/// calculates deltas, classifies them against rough thresholds, and sorts by
/// absolute delta descending.
/// WHY: Stage attribution helps identify which compiler phases changed.
pub fn compare_observations(
    current: &BenchmarkCaseObservations,
    previous: &BenchmarkCaseObservations,
    thresholds: &BenchmarkThresholds,
) -> BenchmarkObservationComparison {
    let mut stage_comparisons = Vec::new();

    for current_metric in &current.stage_timings {
        let Some(previous_metric) = previous
            .stage_timings
            .iter()
            .find(|metric| metric.name == current_metric.name)
        else {
            continue;
        };

        let delta_ms = current_metric.value - previous_metric.value;
        let threshold_ms =
            stage_threshold_ms(previous_metric.value, current_metric.value, thresholds);
        let change_kind = classify_stage_change(delta_ms, threshold_ms);

        stage_comparisons.push(BenchmarkStageComparison {
            stage_name: current_metric.name.clone(),
            previous_ms: previous_metric.value,
            current_ms: current_metric.value,
            delta_ms,
            change_kind,
        });
    }

    stage_comparisons.sort_by(|left, right| right.delta_ms.abs().total_cmp(&left.delta_ms.abs()));

    BenchmarkObservationComparison { stage_comparisons }
}

/// Rough threshold for whether a stage delta is meaningful.
///
/// WHAT: Uses the configured larger of an absolute floor or a percentage of
/// the previous stage time.
/// WHY: Tiny fast stages need an absolute jitter guard; slower stages need a
/// percentage guard to avoid over-reporting stable small movement.
fn stage_threshold_ms(previous_ms: f64, _current_ms: f64, thresholds: &BenchmarkThresholds) -> f64 {
    let ratio_threshold = previous_ms * thresholds.minimum_stage_delta_ratio;
    ratio_threshold.max(thresholds.minimum_stage_delta_ms)
}

fn classify_stage_change(delta_ms: f64, threshold_ms: f64) -> BenchmarkCaseChangeKind {
    if delta_ms < -threshold_ms {
        BenchmarkCaseChangeKind::Faster
    } else if delta_ms > threshold_ms {
        BenchmarkCaseChangeKind::Slower
    } else {
        BenchmarkCaseChangeKind::NoMeasurableChange
    }
}

/// Aggregate stage movement across all overlapping cases in a comparison.
///
/// WHAT: Sums per-case stage deltas and counts faster/slower classifications.
/// WHY: Run-level stage attribution makes it obvious whether a change
/// affected AST, headers, HIR, or borrow checking.
pub fn calculate_stage_movement(comparison: &BenchmarkComparison) -> Vec<BenchmarkStageMovement> {
    let mut movement_by_stage: std::collections::BTreeMap<String, BenchmarkStageMovement> =
        std::collections::BTreeMap::new();

    for case in &comparison.cases {
        for stage in &case.observations.stage_comparisons {
            let movement = movement_by_stage.entry(stage.stage_name.clone()).or_insert(
                BenchmarkStageMovement {
                    stage_name: stage.stage_name.clone(),
                    total_delta_ms: 0.0,
                    case_count: 0,
                    faster_count: 0,
                    slower_count: 0,
                },
            );

            movement.total_delta_ms += stage.delta_ms;
            movement.case_count += 1;
            match stage.change_kind {
                BenchmarkCaseChangeKind::Faster => movement.faster_count += 1,
                BenchmarkCaseChangeKind::Slower => movement.slower_count += 1,
                BenchmarkCaseChangeKind::NoMeasurableChange => {}
            }
        }
    }

    let mut movements: Vec<BenchmarkStageMovement> = movement_by_stage.into_values().collect();
    movements.sort_by(|left, right| {
        right
            .total_delta_ms
            .abs()
            .total_cmp(&left.total_delta_ms.abs())
    });

    movements
}

/// Convert a raw stage metric name to a short friendly label.
pub fn friendly_stage_label(stage_name: &str) -> &str {
    match stage_name {
        "tokenize_ms" => "tokenize",
        "headers_ms" => "headers",
        "file_prepare_ms" => "file prep",
        "dependency_sort_ms" => "sort",
        "ast_ms" => "ast",
        "ast_build_environment_ms" => "ast env",
        "ast_emit_nodes_ms" => "ast emit",
        "ast_finalize_ms" => "ast finalize",
        "hir_ms" => "hir",
        "borrow_ms" => "borrow",
        _ => stage_name,
    }
}

/// Format a stage movement line for terminal or summary output.
///
/// Returns `None` when there are no meaningful stage movers.
pub fn format_stage_movement_line(
    movements: &[BenchmarkStageMovement],
    thresholds: &BenchmarkThresholds,
) -> Option<String> {
    let meaningful: Vec<&BenchmarkStageMovement> = movements
        .iter()
        .filter(|movement| movement.total_delta_ms.abs() >= thresholds.minimum_stage_delta_ms)
        .take(3)
        .collect();

    if meaningful.is_empty() {
        return None;
    }

    let parts: Vec<String> = meaningful
        .iter()
        .map(|movement| {
            format!(
                "{} {}",
                friendly_stage_label(&movement.stage_name),
                format_signed_ms(movement.total_delta_ms)
            )
        })
        .collect();

    Some(format!("Stage movement: {}", parts.join(", ")))
}

/// Format the top current stages by absolute time for baseline runs.
///
/// Returns `None` when no stage data exists in the current cases.
pub fn format_top_current_stages(cases: &[BenchmarkCaseResult]) -> Option<String> {
    let mut sums_by_name: std::collections::BTreeMap<String, (f64, usize)> =
        std::collections::BTreeMap::new();

    for case in cases {
        for metric in &case.observations.stage_timings {
            let entry = sums_by_name.entry(metric.name.clone()).or_insert((0.0, 0));
            entry.0 += metric.value;
            entry.1 += 1;
        }
    }

    if sums_by_name.is_empty() {
        return None;
    }

    let mut stages: Vec<(String, f64)> = sums_by_name
        .into_iter()
        .map(|(name, (sum, count))| (name, if count == 0 { 0.0 } else { sum / count as f64 }))
        .collect();

    stages.sort_by(|left, right| right.1.total_cmp(&left.1));

    let parts: Vec<String> = stages
        .iter()
        .take(3)
        .map(|(name, value)| format!("{} ~{}ms", friendly_stage_label(name), value.round() as i64))
        .collect();

    Some(format!("Top stages: {}", parts.join(", ")))
}

#[cfg(test)]
mod tests;
