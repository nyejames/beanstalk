//! Instrumentation regression tests.
//!
//! WHAT: protects the stable benchmark-counter path used by in-process frontend
//! benchmarks when detailed timer stdout is suppressed.
//! WHY: human-only counter output is not visible to the benchmark collector, so
//! AST counters must flow through the same stable machine path as frontend counters.

#[cfg(feature = "detailed_timers")]
use super::{AstCounter, add_ast_counter, log_ast_counters, reset_ast_counters};

#[cfg(feature = "detailed_timers")]
use crate::compiler_frontend::compiler_messages::compiler_dev_logging::{
    start_benchmark_collection, stop_and_collect_benchmark_observations,
};

#[cfg(feature = "detailed_timers")]
#[test]
fn ast_counters_record_stable_metrics_when_stdout_is_suppressed() {
    reset_ast_counters();
    start_benchmark_collection(true);

    add_ast_counter(AstCounter::ScopeContextsCreated, 3);
    add_ast_counter(AstCounter::TemplateRenderPlansBuilt, 2);
    log_ast_counters();

    let observations = stop_and_collect_benchmark_observations();

    assert_counter_value(&observations.counters, "ast_scope_contexts_created", 3.0);
    assert_counter_value(
        &observations.counters,
        "ast_template_render_plans_built",
        2.0,
    );
}

#[cfg(feature = "detailed_timers")]
fn assert_counter_value(
    counters: &[crate::compiler_frontend::compiler_messages::compiler_dev_logging::BenchmarkObservationMetric],
    name: &str,
    expected: f64,
) {
    let actual = counters
        .iter()
        .find(|counter| counter.name == name)
        .map(|counter| counter.value)
        .unwrap_or(-1.0);

    assert_eq!(actual, expected, "counter `{name}` did not match");
}
