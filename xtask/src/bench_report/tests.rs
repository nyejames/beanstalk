use super::*;
use crate::bench_history::{LocalCaseRecord, LocalMetricRecord, LocalRunRecord};
use crate::bench_types::BenchmarkSystem;

#[test]
fn report_handles_no_local_history() {
    let report = calculate_benchmark_report(&[], None);
    let rendered = format_benchmark_report(&report);

    assert!(report.suites.is_empty());
    assert!(rendered.starts_with("Benchmark report: local data only"));
    assert!(rendered.contains("No local benchmark history found."));
}

#[test]
fn report_names_missing_current_system_history() {
    let system = test_system("SYSTEM-A");
    let report = calculate_benchmark_report(&[], Some(&system));
    let rendered = format_benchmark_report(&report);

    assert!(rendered.contains("No local benchmark history found for the current system."));
}

#[test]
fn report_includes_only_cli_history_when_only_cli_runs_exist() {
    let system = test_system("SYSTEM-A");
    let runs = vec![run_record(
        "2026-05-01T12:00",
        BenchmarkSuiteKind::EndToEndCli,
        "SYSTEM-A",
        vec![case_record("check_core", 42.0, vec![], vec![])],
    )];

    let report = calculate_benchmark_report(&runs, Some(&system));

    assert_eq!(report.suites.len(), 1);
    assert_eq!(report.suites[0].suite_kind, BenchmarkSuiteKind::EndToEndCli);
    assert_eq!(report.suites[0].slowest_cases[0].name, "check_core");
}

#[test]
fn report_includes_only_frontend_history_when_only_frontend_runs_exist() {
    let system = test_system("SYSTEM-A");
    let runs = vec![run_record(
        "2026-05-01T12:00",
        BenchmarkSuiteKind::FrontendPhases,
        "SYSTEM-A",
        vec![case_record(
            "docs",
            20.0,
            vec![metric("ast_ms", 12.0)],
            vec![],
        )],
    )];

    let report = calculate_benchmark_report(&runs, Some(&system));

    assert_eq!(report.suites.len(), 1);
    assert_eq!(
        report.suites[0].suite_kind,
        BenchmarkSuiteKind::FrontendPhases
    );
    assert_eq!(report.suites[0].slowest_cases[0].stages[0].name, "ast_ms");
}

#[test]
fn report_handles_missing_counters_from_old_records() {
    let system = test_system("SYSTEM-A");
    let previous = run_record(
        "2026-05-01T12:00",
        BenchmarkSuiteKind::FrontendPhases,
        "SYSTEM-A",
        vec![case_record(
            "docs",
            20.0,
            vec![metric("ast_ms", 10.0)],
            vec![],
        )],
    );
    let current = run_record(
        "2026-05-02T12:00",
        BenchmarkSuiteKind::FrontendPhases,
        "SYSTEM-A",
        vec![case_record(
            "docs",
            24.0,
            vec![metric("ast_ms", 13.0)],
            vec![metric("ast_header_count", 10.0)],
        )],
    );

    let report = calculate_benchmark_report(&[previous, current], Some(&system));
    let suite = &report.suites[0];

    assert_eq!(suite.stage_movements[0].stage_name, "ast_ms");
    assert!(
        suite
            .ratios
            .iter()
            .any(|ratio| { ratio.name == "ast_ms/ast_header_count" && ratio.case_name == "docs" })
    );
}

#[test]
fn report_formats_counter_movements() {
    let system = test_system("SYSTEM-A");
    let previous = run_record(
        "2026-05-01T12:00",
        BenchmarkSuiteKind::FrontendPhases,
        "SYSTEM-A",
        vec![case_record(
            "docs",
            20.0,
            vec![],
            vec![metric("token_count", 100.0)],
        )],
    );
    let current = run_record(
        "2026-05-02T12:00",
        BenchmarkSuiteKind::FrontendPhases,
        "SYSTEM-A",
        vec![case_record(
            "docs",
            20.0,
            vec![],
            vec![
                metric("token_count", 110.0),
                metric("source_file_count", 3.0),
            ],
        )],
    );

    let report = calculate_benchmark_report(&[previous, current], Some(&system));
    let suite = &report.suites[0];

    let token_movement = suite
        .counter_movements
        .iter()
        .find(|movement| movement.name == "token_count")
        .expect("token_count should move by percentage");
    assert_eq!(format_counter_delta(token_movement), "+10%");

    let new_counter_movement = suite
        .counter_movements
        .iter()
        .find(|movement| movement.name == "source_file_count")
        .expect("new counter should move by absolute delta");
    assert_eq!(format_counter_delta(new_counter_movement), "+3");
}

#[test]
fn report_skips_zero_denominator_ratios() {
    let system = test_system("SYSTEM-A");
    let runs = vec![run_record(
        "2026-05-01T12:00",
        BenchmarkSuiteKind::FrontendPhases,
        "SYSTEM-A",
        vec![case_record(
            "docs",
            20.0,
            vec![metric("file_prepare_ms", 8.0)],
            vec![metric("source_file_count", 0.0)],
        )],
    )];

    let report = calculate_benchmark_report(&runs, Some(&system));

    assert!(report.suites[0].ratios.is_empty());
}

#[test]
fn report_uses_latest_per_suite_without_system_identity() {
    let runs = vec![
        run_record(
            "2026-05-01T12:00",
            BenchmarkSuiteKind::EndToEndCli,
            "SYSTEM-A",
            vec![case_record("old", 50.0, vec![], vec![])],
        ),
        run_record(
            "2026-05-02T12:00",
            BenchmarkSuiteKind::EndToEndCli,
            "SYSTEM-B",
            vec![case_record("new", 30.0, vec![], vec![])],
        ),
    ];

    let report = calculate_benchmark_report(&runs, None);
    let rendered = format_benchmark_report(&report);

    assert_eq!(report.suites.len(), 1);
    assert_eq!(report.suites[0].slowest_cases[0].name, "new");
    assert!(rendered.contains("No local system identity found"));
}

fn test_system(system_uuid: &str) -> BenchmarkSystem {
    BenchmarkSystem {
        system_uuid: system_uuid.to_string(),
        public_system_id: "ABC123".to_string(),
        display_name: "Test System".to_string(),
    }
}

fn run_record(
    timestamp: &str,
    suite_kind: BenchmarkSuiteKind,
    system_uuid: &str,
    cases: Vec<LocalCaseRecord>,
) -> LocalRunRecord {
    LocalRunRecord {
        format_version: 4,
        timestamp: timestamp.to_string(),
        month_key: "2026-05".to_string(),
        commit: Some("abc1234".to_string()),
        system_uuid: system_uuid.to_string(),
        public_system_id: "ABC123".to_string(),
        display_name: "Test System".to_string(),
        warmup_runs: 1,
        measured_iterations: 10,
        suite_kind: suite_kind.persisted_name().to_string(),
        primary_metric_name: suite_kind.primary_metric_name().to_string(),
        suite_average_ms: 0.0,
        suite_case_spread_ms: 0.0,
        groups: vec![],
        cases,
    }
}

fn case_record(
    name: &str,
    mean_ms: f64,
    stage_timings: Vec<LocalMetricRecord>,
    counters: Vec<LocalMetricRecord>,
) -> LocalCaseRecord {
    LocalCaseRecord {
        name: name.to_string(),
        group_name: "test".to_string(),
        command: "check".to_string(),
        args: vec![name.to_string()],
        mean_ms,
        median_ms: mean_ms,
        stddev_ms: 0.0,
        stage_timings,
        counters,
    }
}

fn metric(name: &str, value: f64) -> LocalMetricRecord {
    LocalMetricRecord {
        name: name.to_string(),
        value,
    }
}
