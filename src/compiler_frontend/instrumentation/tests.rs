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

    // Template churn counters must also flow through the stable benchmark
    // metric path without panics.
    add_ast_counter(AstCounter::TemplateNestedTemplateParses, 7);
    add_ast_counter(AstCounter::TemplateBodyTokenVisits, 11);
    add_ast_counter(AstCounter::TemplateTextBytesParsed, 13);
    add_ast_counter(AstCounter::TemplateContentEstimatedAtomCapacity, 15);
    add_ast_counter(AstCounter::TemplateFoldOutputBytes, 17);
    add_ast_counter(AstCounter::TemplateEstimatedFoldOutputBytes, 18);
    add_ast_counter(AstCounter::TemplateFoldOutputEstimateMissBytes, 16);
    add_ast_counter(AstCounter::TemplateFoldStringInternCalls, 19);
    add_ast_counter(AstCounter::TemplateFoldExpressionCloneRequests, 23);
    add_ast_counter(AstCounter::TemplateFoldExpressionOwnedRewrites, 24);
    add_ast_counter(AstCounter::TemplateFoldBindingSubstitutions, 29);
    add_ast_counter(AstCounter::TemplateContentClonesForRenderUnits, 31);
    add_ast_counter(AstCounter::TemplateContentRebuildsAfterFormatting, 37);
    add_ast_counter(AstCounter::TemplateWrapperVectorClones, 41);
    add_ast_counter(AstCounter::TemplateAggregatePlanBuilds, 43);

    log_ast_counters();

    let observations = stop_and_collect_benchmark_observations();

    assert_counter_value(&observations.counters, "ast_scope_contexts_created", 3.0);
    assert_counter_value(
        &observations.counters,
        "ast_template_render_plans_built",
        2.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_nested_template_parses",
        7.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_body_token_visits",
        11.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_text_bytes_parsed",
        13.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_content_estimated_atom_capacity",
        15.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_fold_output_bytes",
        17.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_estimated_fold_output_bytes",
        18.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_fold_output_estimate_miss_bytes",
        16.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_fold_string_intern_calls",
        19.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_fold_expression_clone_requests",
        23.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_fold_expression_owned_rewrites",
        24.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_fold_binding_substitutions",
        29.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_content_clones_for_render_units",
        31.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_content_rebuilds_after_formatting",
        37.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_wrapper_vector_clones",
        41.0,
    );
    assert_counter_value(
        &observations.counters,
        "ast_template_aggregate_plan_builds",
        43.0,
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
