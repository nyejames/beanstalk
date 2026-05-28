use super::*;

#[test]
fn parses_stage_timings_and_counters_from_detailed_stdout() {
    let stdout = concat!(
        "Tokenized in: \u{1b}[32m335.834µs\u{1b}[0m\n",
        "Headers Parsed in: \u{1b}[32m1.25ms\u{1b}[0m\n",
        "AST/build environment completed in: \u{1b}[32m0.002s\u{1b}[0m\n",
        "AST/churn counters:\u{1b}[0m\n",
        "  scope contexts created = \u{1b}[2m\u{1b}[32m602\u{1b}[0m\n",
        "  postfix receiver nodes copied = \u{1b}[2m\u{1b}[32m41\u{1b}[0m\n",
        "AST created in: \u{1b}[32m51.731083ms\u{1b}[0m\n",
        "Frontend/performance counters:\n",
        "  StringTable/full clone count = \u{1b}[2m\u{1b}[32m8\u{1b}[0m\n",
    );

    let observations = parse_stdout_observations(stdout);

    assert_eq!(observations.stage_timings.len(), 4);
    assert_metric_value(&observations.stage_timings, "tokenize_ms", 0.335834);
    assert_metric_value(&observations.stage_timings, "headers_ms", 1.25);
    assert_metric_value(&observations.stage_timings, "ast_build_environment_ms", 2.0);
    assert_metric_value(&observations.stage_timings, "ast_ms", 51.731083);

    assert_metric_value(&observations.counters, "scope contexts created", 602.0);
    assert_metric_value(
        &observations.counters,
        "postfix receiver nodes copied",
        41.0,
    );
    assert_metric_value(&observations.counters, "StringTable/full clone count", 8.0);
}

#[test]
fn averages_metrics_by_name_across_iterations() {
    let observations = vec![
        BenchmarkCaseObservations {
            stage_timings: vec![BenchmarkMetric {
                name: "ast_ms".to_string(),
                value: 10.0,
            }],
            counters: vec![BenchmarkMetric {
                name: "counter".to_string(),
                value: 2.0,
            }],
        },
        BenchmarkCaseObservations {
            stage_timings: vec![BenchmarkMetric {
                name: "ast_ms".to_string(),
                value: 14.0,
            }],
            counters: vec![BenchmarkMetric {
                name: "counter".to_string(),
                value: 6.0,
            }],
        },
    ];

    let averaged = average_observations(&observations);

    assert_metric_value(&averaged.stage_timings, "ast_ms", 12.0);
    assert_metric_value(&averaged.counters, "counter", 4.0);
}

#[test]
fn parses_stable_benchmark_timing_lines() {
    let stdout = concat!(
        "BST_BENCH timing file_prepare_ms=12.5ms\n",
        "BST_BENCH timing ast_ms=51.731083ms\n",
        "BST_BENCH timing hir_ms=8ms\n",
    );

    let observations = parse_stdout_observations(stdout);

    assert_eq!(observations.stage_timings.len(), 3);
    assert_metric_value(&observations.stage_timings, "file_prepare_ms", 12.5);
    assert_metric_value(&observations.stage_timings, "ast_ms", 51.731083);
    assert_metric_value(&observations.stage_timings, "hir_ms", 8.0);
}

#[test]
fn ignores_unknown_lines() {
    let stdout = concat!(
        "Some random compiler output\n",
        "BST_BENCH timing ast_ms=10ms\n",
        "Another random line\n",
    );

    let observations = parse_stdout_observations(stdout);

    assert_eq!(observations.stage_timings.len(), 1);
    assert_metric_value(&observations.stage_timings, "ast_ms", 10.0);
}

#[test]
fn ignores_malformed_stable_benchmark_timing_lines() {
    let stdout = concat!(
        "BST_BENCH timing =10ms\n",
        "BST_BENCH timing ast_ms=10\n",
        "BST_BENCH timing hir_ms=7ms\n",
    );

    let observations = parse_stdout_observations(stdout);

    assert_eq!(observations.stage_timings.len(), 1);
    assert_metric_value(&observations.stage_timings, "hir_ms", 7.0);
}

#[test]
fn sums_duplicate_stable_metrics_within_one_command_output() {
    let stdout = concat!(
        "BST_BENCH timing ast_ms=2ms\n",
        "BST_BENCH timing ast_ms=3ms\n",
    );

    let observations = parse_stdout_observations(stdout);

    assert_metric_value(&observations.stage_timings, "ast_ms", 5.0);
}

#[test]
fn stable_lines_take_precedence_over_legacy_human_lines() {
    let stdout = concat!(
        "BST_BENCH timing ast_ms=10ms\n",
        "AST created in: 10ms\n",
        "HIR generated in: 5ms\n",
    );

    let observations = parse_stdout_observations(stdout);

    // Stable line should be used, legacy human line for the same metric ignored.
    assert_metric_value(&observations.stage_timings, "ast_ms", 10.0);
    // Legacy line for a metric without a stable counterpart should still parse.
    assert_metric_value(&observations.stage_timings, "hir_ms", 5.0);
}

#[test]
fn sums_duplicate_metrics_within_one_command_output() {
    let stdout = concat!(
        "AST created in: 2ms\n",
        "AST created in: 3ms\n",
        "AST/churn counters:\n",
        "  scope contexts created = 4\n",
        "AST/churn counters:\n",
        "  scope contexts created = 6\n",
    );

    let observations = parse_stdout_observations(stdout);

    assert_metric_value(&observations.stage_timings, "ast_ms", 5.0);
    assert_metric_value(&observations.counters, "scope contexts created", 10.0);
}

fn assert_metric_value(metrics: &[BenchmarkMetric], name: &str, expected: f64) {
    let metric = metrics
        .iter()
        .find(|metric| metric.name == name)
        .unwrap_or_else(|| panic!("missing metric {name}"));

    assert!(
        (metric.value - expected).abs() < 0.000001,
        "expected {name} to be {expected}, got {}",
        metric.value
    );
}
