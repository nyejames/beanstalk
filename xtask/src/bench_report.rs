//! Local-only benchmark drilldown report.
//!
//! WHAT: Reads ignored JSONL benchmark history and prints a compact report for
//! optimization investigations.
//! WHY: Tracked summaries stay terse; this command gives developers local
//! per-case, stage, counter, and ratio evidence without writing any files.

use crate::bench_history::{LocalRunRecord, RUNS_JSONL_PATH, read_local_runs, to_case_results};
use crate::bench_system::{SystemIdentityMode, load_or_create_system};
use crate::bench_types::{
    BenchmarkCaseResult, BenchmarkComparison, BenchmarkMetric, BenchmarkStageMovement,
    BenchmarkSuiteKind, BenchmarkSystem, BenchmarkThresholds, calculate_stage_movement,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const SLOW_CASE_LIMIT: usize = 3;
const SLOW_CASE_STAGE_LIMIT: usize = 2;
const STAGE_MOVEMENT_LIMIT: usize = 5;
const COUNTER_MOVEMENT_LIMIT: usize = 3;
const RATIO_LIMIT: usize = 5;
const INVESTIGATION_LIMIT: usize = 3;
const MEANINGFUL_COUNTER_RATIO: f64 = 0.03;

const RATIO_CATALOG: &[RatioSpec] = &[
    RatioSpec::new(
        "file_prepare_ms/source_file_count",
        "file_prepare_ms",
        "source_file_count",
        "ms/file",
        Some("inspect tokenization, header parsing, string-table merge/remap"),
    ),
    RatioSpec::new(
        "file_prepare_ms/source_byte_count",
        "file_prepare_ms",
        "source_byte_count",
        "ms/byte",
        None,
    ),
    RatioSpec::new(
        "file_prepare_ms/token_count",
        "file_prepare_ms",
        "token_count",
        "ms/token",
        None,
    ),
    RatioSpec::new(
        "dependency_sort_ms/dependency_edge_count",
        "dependency_sort_ms",
        "dependency_edge_count",
        "ms/edge",
        Some("inspect duplicate edges or graph traversal"),
    ),
    RatioSpec::new(
        "ast_ms/ast_header_count",
        "ast_ms",
        "ast_header_count",
        "ms/header",
        None,
    ),
    RatioSpec::new(
        "ast_ms/type_compatibility_cache_lookups",
        "ast_ms",
        "type_compatibility_cache_lookups",
        "ms/lookup",
        None,
    ),
    RatioSpec::new(
        "ast_ms/type_compatibility_cache_misses",
        "ast_ms",
        "type_compatibility_cache_misses",
        "ms/miss",
        Some("inspect compatibility caching or repeated type checks"),
    ),
    RatioSpec::new(
        "hir_ms/hir_statement_count",
        "hir_ms",
        "hir_statement_count",
        "ms/statement",
        None,
    ),
    RatioSpec::new(
        "borrow_ms/borrow_conflict_check_count",
        "borrow_ms",
        "borrow_conflict_check_count",
        "ms/check",
        Some("inspect borrow state representation"),
    ),
    RatioSpec::new(
        "borrow_ms/borrow_statement_fact_count",
        "borrow_ms",
        "borrow_statement_fact_count",
        "ms/statement-fact",
        None,
    ),
    RatioSpec::new(
        "borrow_ms/borrow_value_fact_count",
        "borrow_ms",
        "borrow_value_fact_count",
        "ms/value-fact",
        None,
    ),
];

/// Run `bench-report` from the repository root.
pub fn run_benchmark_report() -> Result<(), String> {
    let runs = read_local_runs(Path::new(RUNS_JSONL_PATH))?;
    let system = load_or_create_system(SystemIdentityMode::ReadOnly)?;
    let report = calculate_benchmark_report(&runs, system.as_ref());

    println!("{}", format_benchmark_report(&report));

    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct BenchmarkReport {
    pub(crate) used_current_system: bool,
    pub(crate) suites: Vec<SuiteReport>,
}

#[derive(Debug, Clone)]
pub(crate) struct SuiteReport {
    pub(crate) suite_kind: BenchmarkSuiteKind,
    pub(crate) system_display: String,
    pub(crate) public_system_id: String,
    pub(crate) latest_timestamp: String,
    pub(crate) latest_commit: Option<String>,
    pub(crate) comparison: BenchmarkComparison,
    pub(crate) slowest_cases: Vec<SlowCaseReport>,
    pub(crate) stage_movements: Vec<BenchmarkStageMovement>,
    pub(crate) counter_movements: Vec<CounterMovement>,
    pub(crate) ratios: Vec<RatioReport>,
    pub(crate) investigation_hints: Vec<InvestigationHint>,
}

#[derive(Debug, Clone)]
pub(crate) struct SlowCaseReport {
    pub(crate) name: String,
    pub(crate) mean_ms: f64,
    pub(crate) stages: Vec<BenchmarkMetric>,
}

#[derive(Debug, Clone)]
pub(crate) struct CounterMovement {
    pub(crate) name: String,
    pub(crate) previous: f64,
    pub(crate) delta: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct RatioReport {
    pub(crate) name: &'static str,
    pub(crate) case_name: String,
    pub(crate) value: f64,
    pub(crate) unit: &'static str,
    pub(crate) hint: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub(crate) struct InvestigationHint {
    pub(crate) case_name: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Copy)]
struct RatioSpec {
    name: &'static str,
    numerator: &'static str,
    denominator: &'static str,
    unit: &'static str,
    hint: Option<&'static str>,
}

impl RatioSpec {
    const fn new(
        name: &'static str,
        numerator: &'static str,
        denominator: &'static str,
        unit: &'static str,
        hint: Option<&'static str>,
    ) -> Self {
        Self {
            name,
            numerator,
            denominator,
            unit,
            hint,
        }
    }
}

pub(crate) fn calculate_benchmark_report(
    runs: &[LocalRunRecord],
    current_system: Option<&BenchmarkSystem>,
) -> BenchmarkReport {
    let mut suites = Vec::new();

    for suite_kind in [
        BenchmarkSuiteKind::EndToEndCli,
        BenchmarkSuiteKind::FrontendPhases,
    ] {
        let Some(selection) = select_latest_run(runs, suite_kind, current_system) else {
            continue;
        };

        let suite = calculate_suite_report(suite_kind, selection);
        suites.push(suite);
    }

    BenchmarkReport {
        used_current_system: current_system.is_some(),
        suites,
    }
}

fn select_latest_run<'a>(
    runs: &'a [LocalRunRecord],
    suite_kind: BenchmarkSuiteKind,
    current_system: Option<&BenchmarkSystem>,
) -> Option<SelectedRun<'a>> {
    let persisted_suite_kind = suite_kind.persisted_name();

    let latest_index = runs.iter().enumerate().rev().find_map(|(index, run)| {
        if run.suite_kind != persisted_suite_kind {
            return None;
        }

        if let Some(system) = current_system
            && run.system_uuid != system.system_uuid
        {
            return None;
        }

        Some(index)
    })?;
    let latest = &runs[latest_index];

    let previous = runs[..latest_index].iter().rev().find(|run| {
        run.suite_kind == persisted_suite_kind && run.system_uuid == latest.system_uuid
    });

    Some(SelectedRun { latest, previous })
}

struct SelectedRun<'a> {
    latest: &'a LocalRunRecord,
    previous: Option<&'a LocalRunRecord>,
}

fn calculate_suite_report(
    suite_kind: BenchmarkSuiteKind,
    selection: SelectedRun<'_>,
) -> SuiteReport {
    let current_cases = to_case_results(selection.latest);
    let previous_cases = selection.previous.map(to_case_results);
    let comparison = BenchmarkComparison::new(&current_cases, previous_cases.as_deref());

    let slowest_cases = calculate_slowest_cases(&current_cases);
    let stage_movements = calculate_meaningful_stage_movements(&comparison);
    let counter_movements = calculate_counter_movements(&current_cases, previous_cases.as_deref());
    let ratios = calculate_ratios(&current_cases);
    let investigation_hints = calculate_investigation_hints(
        &comparison,
        &current_cases,
        &counter_movements,
        &ratios,
        previous_cases.as_deref(),
    );

    SuiteReport {
        suite_kind,
        system_display: selection.latest.display_name.clone(),
        public_system_id: selection.latest.public_system_id.clone(),
        latest_timestamp: selection.latest.timestamp.clone(),
        latest_commit: selection.latest.commit.clone(),
        comparison,
        slowest_cases,
        stage_movements,
        counter_movements,
        ratios,
        investigation_hints,
    }
}

fn calculate_slowest_cases(cases: &[BenchmarkCaseResult]) -> Vec<SlowCaseReport> {
    let mut sorted_cases: Vec<&BenchmarkCaseResult> = cases.iter().collect();
    sorted_cases.sort_by(|left, right| right.mean_ms.total_cmp(&left.mean_ms));

    sorted_cases
        .into_iter()
        .take(SLOW_CASE_LIMIT)
        .map(|case| {
            let mut stages = case.observations.stage_timings.clone();
            stages.sort_by(|left, right| right.value.total_cmp(&left.value));
            stages.truncate(SLOW_CASE_STAGE_LIMIT);

            SlowCaseReport {
                name: case.case_name.clone(),
                mean_ms: case.mean_ms,
                stages,
            }
        })
        .collect()
}

fn calculate_meaningful_stage_movements(
    comparison: &BenchmarkComparison,
) -> Vec<BenchmarkStageMovement> {
    calculate_stage_movement(comparison)
        .into_iter()
        .filter(|movement| {
            movement.total_delta_ms.abs() >= BenchmarkThresholds::DEFAULT.minimum_stage_delta_ms
        })
        .take(STAGE_MOVEMENT_LIMIT)
        .collect()
}

fn calculate_counter_movements(
    current_cases: &[BenchmarkCaseResult],
    previous_cases: Option<&[BenchmarkCaseResult]>,
) -> Vec<CounterMovement> {
    let Some(previous_cases) = previous_cases else {
        return Vec::new();
    };

    let current_totals = sum_counters(current_cases);
    let previous_totals = sum_counters(previous_cases);
    let mut names = BTreeSet::new();
    names.extend(current_totals.keys().cloned());
    names.extend(previous_totals.keys().cloned());

    let mut movements = Vec::new();
    for name in names {
        let current = current_totals.get(&name).copied().unwrap_or(0.0);
        let previous = previous_totals.get(&name).copied().unwrap_or(0.0);
        let delta = current - previous;

        if !is_meaningful_counter_movement(previous, delta) {
            continue;
        }

        movements.push(CounterMovement {
            name,
            previous,
            delta,
        });
    }

    movements.sort_by(|left, right| counter_score(right).total_cmp(&counter_score(left)));
    movements.truncate(COUNTER_MOVEMENT_LIMIT);
    movements
}

fn sum_counters(cases: &[BenchmarkCaseResult]) -> BTreeMap<String, f64> {
    let mut totals = BTreeMap::new();

    for case in cases {
        for counter in &case.observations.counters {
            let entry = totals.entry(counter.name.clone()).or_insert(0.0);
            *entry += counter.value;
        }
    }

    totals
}

fn is_meaningful_counter_movement(previous: f64, delta: f64) -> bool {
    if previous == 0.0 {
        return delta != 0.0;
    }

    (delta / previous).abs() >= MEANINGFUL_COUNTER_RATIO
}

fn counter_score(movement: &CounterMovement) -> f64 {
    if movement.previous == 0.0 {
        movement.delta.abs()
    } else {
        (movement.delta / movement.previous).abs()
    }
}

fn calculate_ratios(cases: &[BenchmarkCaseResult]) -> Vec<RatioReport> {
    let mut ratios = Vec::new();

    for case in cases {
        for spec in RATIO_CATALOG {
            let Some(numerator) = metric_value(&case.observations.stage_timings, spec.numerator)
            else {
                continue;
            };
            let Some(denominator) = metric_value(&case.observations.counters, spec.denominator)
            else {
                continue;
            };

            if denominator == 0.0 {
                continue;
            }

            ratios.push(RatioReport {
                name: spec.name,
                case_name: case.case_name.clone(),
                value: numerator / denominator,
                unit: spec.unit,
                hint: spec.hint,
            });
        }
    }

    ratios.sort_by(|left, right| right.value.total_cmp(&left.value));
    ratios.truncate(RATIO_LIMIT);
    ratios
}

fn metric_value(metrics: &[BenchmarkMetric], name: &str) -> Option<f64> {
    metrics
        .iter()
        .find(|metric| metric.name == name)
        .map(|metric| metric.value)
}

fn calculate_investigation_hints(
    comparison: &BenchmarkComparison,
    current_cases: &[BenchmarkCaseResult],
    counter_movements: &[CounterMovement],
    ratios: &[RatioReport],
    previous_cases: Option<&[BenchmarkCaseResult]>,
) -> Vec<InvestigationHint> {
    let mut hints = Vec::new();
    let mut seen = BTreeSet::new();

    for ratio in ratios {
        let Some(message) = ratio.hint else {
            continue;
        };

        push_hint(
            &mut hints,
            &mut seen,
            ratio.case_name.clone(),
            format!("high {}; {}", ratio.name, message),
        );

        if hints.len() >= INVESTIGATION_LIMIT {
            return hints;
        }
    }

    if let Some(previous_cases) = previous_cases {
        for case in &comparison.cases {
            if case.delta_ms <= case.threshold_ms {
                continue;
            }

            if case_counters_are_stable(case, current_cases, previous_cases, counter_movements) {
                push_hint(
                    &mut hints,
                    &mut seen,
                    case.case_name.clone(),
                    "timing grew while counters stayed near previous values; run CPU profiling before refactoring"
                        .to_string(),
                );
            }

            if hints.len() >= INVESTIGATION_LIMIT {
                break;
            }
        }
    }

    hints
}

fn push_hint(
    hints: &mut Vec<InvestigationHint>,
    seen: &mut BTreeSet<String>,
    case_name: String,
    message: String,
) {
    let key = format!("{case_name}\n{message}");
    if seen.insert(key) {
        hints.push(InvestigationHint { case_name, message });
    }
}

fn case_counters_are_stable(
    case: &crate::bench_types::BenchmarkCaseComparison,
    current_cases: &[BenchmarkCaseResult],
    previous_cases: &[BenchmarkCaseResult],
    counter_movements: &[CounterMovement],
) -> bool {
    let Some(previous_case) = previous_cases
        .iter()
        .find(|previous| previous.case_name == case.case_name)
    else {
        return false;
    };
    let Some(current_case) = current_cases
        .iter()
        .find(|current| current.case_name == case.case_name)
    else {
        return false;
    };

    let previous_total = previous_case
        .observations
        .counters
        .iter()
        .map(|counter| counter.value)
        .sum::<f64>();
    let current_total = current_case
        .observations
        .counters
        .iter()
        .map(|counter| counter.value)
        .sum::<f64>();

    if previous_total == 0.0 && current_total == 0.0 {
        return false;
    }

    if counter_movements.is_empty() {
        return true;
    }

    if previous_total == 0.0 {
        return current_total == 0.0;
    }

    ((current_total - previous_total) / previous_total).abs() < MEANINGFUL_COUNTER_RATIO
}

pub(crate) fn format_benchmark_report(report: &BenchmarkReport) -> String {
    let mut output = String::from("Benchmark report: local data only\n");

    if !report.used_current_system {
        output.push_str("\nNo local system identity found; showing latest local run per suite.\n");
    }

    if report.suites.is_empty() {
        if report.used_current_system {
            output.push_str("\nNo local benchmark history found for the current system.\n");
        } else {
            output.push_str("\nNo local benchmark history found.\n");
        }
        return output;
    }

    for suite in &report.suites {
        output.push('\n');
        output.push_str(&format!(
            "{} / {} ({})\n",
            suite.suite_kind.display_label(),
            suite.system_display,
            suite.public_system_id
        ));
        output.push_str(&format!(
            "Latest: {}, commit {}\n",
            suite.latest_timestamp,
            suite.latest_commit.as_deref().unwrap_or("unknown")
        ));
        output.push_str(&format!(
            "Change: {}\n",
            suite.comparison.format_run_change_line().replace("**", "")
        ));

        append_slowest_cases(&mut output, suite);
        append_stage_movement(&mut output, suite);
        append_counter_movement(&mut output, suite);
        append_ratios(&mut output, suite);
        append_investigation_hints(&mut output, suite);
    }

    output
}

fn append_slowest_cases(output: &mut String, suite: &SuiteReport) {
    output.push_str("\nSlowest cases:\n");
    if suite.slowest_cases.is_empty() {
        output.push_str("  none\n");
        return;
    }

    for case in &suite.slowest_cases {
        let stage_text = case
            .stages
            .iter()
            .map(|stage| format!("{} ~{}ms", stage.name, stage.value.round() as i64))
            .collect::<Vec<_>>()
            .join(", ");

        if stage_text.is_empty() {
            output.push_str(&format!(
                "  {:<28} ~{}ms\n",
                case.name,
                case.mean_ms.round() as i64
            ));
        } else {
            output.push_str(&format!(
                "  {:<28} ~{}ms  {}\n",
                case.name,
                case.mean_ms.round() as i64,
                stage_text
            ));
        }
    }
}

fn append_stage_movement(output: &mut String, suite: &SuiteReport) {
    output.push_str("\nStage movement:\n");
    if suite.stage_movements.is_empty() {
        output.push_str("  none\n");
        return;
    }

    for movement in &suite.stage_movements {
        output.push_str(&format!(
            "  {:<32} {} across {} cases\n",
            movement.stage_name,
            format_signed_ms(movement.total_delta_ms),
            movement.case_count
        ));
    }
}

fn append_counter_movement(output: &mut String, suite: &SuiteReport) {
    output.push_str("\nCounter movement:\n");
    if suite.counter_movements.is_empty() {
        output.push_str("  none\n");
        return;
    }

    for movement in &suite.counter_movements {
        output.push_str(&format!(
            "  {:<32} {}\n",
            movement.name,
            format_counter_delta(movement)
        ));
    }
}

fn append_ratios(output: &mut String, suite: &SuiteReport) {
    output.push_str("\nRatios:\n");
    if suite.ratios.is_empty() {
        output.push_str("  none\n");
        return;
    }

    for ratio in &suite.ratios {
        output.push_str(&format!(
            "  {:<40} {:<24} {}{}\n",
            ratio.name,
            ratio.case_name,
            format_ratio_value(ratio.value),
            ratio.unit
        ));
    }
}

fn append_investigation_hints(output: &mut String, suite: &SuiteReport) {
    output.push_str("\nNext investigation candidates:\n");
    if suite.investigation_hints.is_empty() {
        output.push_str("  none\n");
        return;
    }

    for hint in &suite.investigation_hints {
        output.push_str(&format!("  {}: {}\n", hint.case_name, hint.message));
    }
}

fn format_counter_delta(movement: &CounterMovement) -> String {
    if movement.previous == 0.0 {
        return format_signed_number(movement.delta);
    }

    let percent = (movement.delta / movement.previous) * 100.0;
    format!("{}%", format_signed_number(percent))
}

fn format_signed_ms(value: f64) -> String {
    format!("{}ms", format_signed_number(value.round()))
}

fn format_signed_number(value: f64) -> String {
    let rounded = value.round() as i64;
    if rounded > 0 {
        format!("+{rounded}")
    } else {
        rounded.to_string()
    }
}

fn format_ratio_value(value: f64) -> String {
    if value >= 10.0 {
        format!("{value:.1}")
    } else if value >= 1.0 {
        format!("{value:.2}")
    } else {
        format!("{value:.4}")
    }
}

#[cfg(test)]
mod tests;
