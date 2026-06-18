//! Detailed AST build instrumentation.
//!
//! WHAT: tracks local-only AST churn counters for performance-sensitive parser, emitter, and
//! finalizer paths.
//! WHY: benchmark runs need objective evidence for small timing shifts, while normal compiler
//! output must remain unchanged.

#[derive(Copy, Clone)]
pub(crate) enum AstCounter {
    ScopeContextsCreated,
    ScopeMaxFrameDepth,
    ScopeFrameLookupAncestorSteps,
    ScopeFrameRedeclarationAncestorChecks,
    ScopeLocalDeclarationsInserted,
    BoundedExpressionTokenWindows,
    BoundedExpressionTokenCopiesAvoided,
    TemplateNormalizationNodesVisited,
    ModuleConstantNormalizationExpressionsVisited,
    TemplatesFoldedDuringFinalization,
    RuntimeRenderPlansRebuilt,
    PostfixReceiverNodesCopied,
}

#[cfg(feature = "detailed_timers")]
use crate::compiler_frontend::compiler_messages::compiler_dev_logging::detailed_timer_output_enabled;

#[cfg(feature = "detailed_timers")]
mod detailed {
    use super::AstCounter;
    use super::detailed_timer_output_enabled;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SCOPE_CONTEXTS_CREATED: AtomicUsize = AtomicUsize::new(0);
    static SCOPE_MAX_FRAME_DEPTH: AtomicUsize = AtomicUsize::new(0);
    static SCOPE_FRAME_LOOKUP_ANCESTOR_STEPS: AtomicUsize = AtomicUsize::new(0);
    static SCOPE_FRAME_REDECLARATION_ANCESTOR_CHECKS: AtomicUsize = AtomicUsize::new(0);
    static SCOPE_LOCAL_DECLARATIONS_INSERTED: AtomicUsize = AtomicUsize::new(0);
    static BOUNDED_EXPRESSION_TOKEN_WINDOWS: AtomicUsize = AtomicUsize::new(0);
    static BOUNDED_EXPRESSION_TOKEN_COPIES_AVOIDED: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATE_NORMALIZATION_NODES_VISITED: AtomicUsize = AtomicUsize::new(0);
    static MODULE_CONSTANT_NORMALIZATION_EXPRESSIONS_VISITED: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATES_FOLDED_DURING_FINALIZATION: AtomicUsize = AtomicUsize::new(0);
    static RUNTIME_RENDER_PLANS_REBUILT: AtomicUsize = AtomicUsize::new(0);
    static POSTFIX_RECEIVER_NODES_COPIED: AtomicUsize = AtomicUsize::new(0);

    pub(crate) fn reset_ast_counters() {
        for counter in all_counters() {
            atomic_counter(counter).store(0, Ordering::Relaxed);
        }
    }

    pub(crate) fn increment_ast_counter(counter: AstCounter) {
        add_ast_counter(counter, 1);
    }

    pub(crate) fn add_ast_counter(counter: AstCounter, amount: usize) {
        atomic_counter(counter).fetch_add(amount, Ordering::Relaxed);
    }

    pub(crate) fn record_ast_counter_max(counter: AstCounter, value: usize) {
        atomic_counter(counter).fetch_max(value, Ordering::Relaxed);
    }

    pub(crate) fn log_ast_counters() {
        if !detailed_timer_output_enabled() {
            return;
        }

        let scope_contexts_created = counter_value(AstCounter::ScopeContextsCreated);
        let scope_max_frame_depth = counter_value(AstCounter::ScopeMaxFrameDepth);
        let scope_frame_lookup_ancestor_steps =
            counter_value(AstCounter::ScopeFrameLookupAncestorSteps);
        let scope_frame_redeclaration_ancestor_checks =
            counter_value(AstCounter::ScopeFrameRedeclarationAncestorChecks);
        let scope_local_declarations_inserted =
            counter_value(AstCounter::ScopeLocalDeclarationsInserted);
        let bounded_expression_token_windows =
            counter_value(AstCounter::BoundedExpressionTokenWindows);
        let bounded_expression_token_copies_avoided =
            counter_value(AstCounter::BoundedExpressionTokenCopiesAvoided);
        let template_normalization_nodes_visited =
            counter_value(AstCounter::TemplateNormalizationNodesVisited);
        let module_constant_normalization_expressions_visited =
            counter_value(AstCounter::ModuleConstantNormalizationExpressionsVisited);
        let templates_folded_during_finalization =
            counter_value(AstCounter::TemplatesFoldedDuringFinalization);
        let runtime_render_plans_rebuilt = counter_value(AstCounter::RuntimeRenderPlansRebuilt);
        let postfix_receiver_nodes_copied = counter_value(AstCounter::PostfixReceiverNodesCopied);

        saying::say!("AST/churn counters:");
        saying::say!(
            "  scope contexts created = ",
            Dark Green scope_contexts_created
        );
        saying::say!(
            "  scope max frame depth = ",
            Dark Green scope_max_frame_depth
        );
        saying::say!(
            "  scope frame lookup ancestor steps = ",
            Dark Green scope_frame_lookup_ancestor_steps
        );
        saying::say!(
            "  scope frame redeclaration ancestor checks = ",
            Dark Green scope_frame_redeclaration_ancestor_checks
        );
        saying::say!(
            "  scope local declarations inserted = ",
            Dark Green scope_local_declarations_inserted
        );
        saying::say!(
            "  bounded expression token windows = ",
            Dark Green bounded_expression_token_windows
        );
        saying::say!(
            "  bounded expression token copies avoided = ",
            Dark Green bounded_expression_token_copies_avoided
        );
        saying::say!(
            "  template normalization nodes visited = ",
            Dark Green template_normalization_nodes_visited
        );
        saying::say!(
            "  module constant normalization expressions visited = ",
            Dark Green module_constant_normalization_expressions_visited
        );
        saying::say!(
            "  templates folded during finalization = ",
            Dark Green templates_folded_during_finalization
        );
        saying::say!(
            "  runtime render plans rebuilt = ",
            Dark Green runtime_render_plans_rebuilt
        );
        saying::say!(
            "  postfix receiver nodes copied = ",
            Dark Green postfix_receiver_nodes_copied
        );
    }

    fn all_counters() -> [AstCounter; 12] {
        [
            AstCounter::ScopeContextsCreated,
            AstCounter::ScopeMaxFrameDepth,
            AstCounter::ScopeFrameLookupAncestorSteps,
            AstCounter::ScopeFrameRedeclarationAncestorChecks,
            AstCounter::ScopeLocalDeclarationsInserted,
            AstCounter::BoundedExpressionTokenWindows,
            AstCounter::BoundedExpressionTokenCopiesAvoided,
            AstCounter::TemplateNormalizationNodesVisited,
            AstCounter::ModuleConstantNormalizationExpressionsVisited,
            AstCounter::TemplatesFoldedDuringFinalization,
            AstCounter::RuntimeRenderPlansRebuilt,
            AstCounter::PostfixReceiverNodesCopied,
        ]
    }

    fn atomic_counter(counter: AstCounter) -> &'static AtomicUsize {
        match counter {
            AstCounter::ScopeContextsCreated => &SCOPE_CONTEXTS_CREATED,

            AstCounter::ScopeMaxFrameDepth => &SCOPE_MAX_FRAME_DEPTH,

            AstCounter::ScopeFrameLookupAncestorSteps => &SCOPE_FRAME_LOOKUP_ANCESTOR_STEPS,

            AstCounter::ScopeFrameRedeclarationAncestorChecks => {
                &SCOPE_FRAME_REDECLARATION_ANCESTOR_CHECKS
            }

            AstCounter::ScopeLocalDeclarationsInserted => &SCOPE_LOCAL_DECLARATIONS_INSERTED,

            AstCounter::BoundedExpressionTokenWindows => &BOUNDED_EXPRESSION_TOKEN_WINDOWS,

            AstCounter::BoundedExpressionTokenCopiesAvoided => {
                &BOUNDED_EXPRESSION_TOKEN_COPIES_AVOIDED
            }

            AstCounter::TemplateNormalizationNodesVisited => &TEMPLATE_NORMALIZATION_NODES_VISITED,

            AstCounter::ModuleConstantNormalizationExpressionsVisited => {
                &MODULE_CONSTANT_NORMALIZATION_EXPRESSIONS_VISITED
            }

            AstCounter::TemplatesFoldedDuringFinalization => &TEMPLATES_FOLDED_DURING_FINALIZATION,

            AstCounter::RuntimeRenderPlansRebuilt => &RUNTIME_RENDER_PLANS_REBUILT,

            AstCounter::PostfixReceiverNodesCopied => &POSTFIX_RECEIVER_NODES_COPIED,
        }
    }

    fn counter_value(counter: AstCounter) -> usize {
        atomic_counter(counter).load(Ordering::Relaxed)
    }
}

#[cfg(feature = "detailed_timers")]
pub(crate) use detailed::{
    add_ast_counter, increment_ast_counter, log_ast_counters, record_ast_counter_max,
    reset_ast_counters,
};

// Stubs when detailed timers are disabled.
#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn reset_ast_counters() {}

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn increment_ast_counter(_counter: AstCounter) {}

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn add_ast_counter(_counter: AstCounter, _amount: usize) {}

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn record_ast_counter_max(_counter: AstCounter, _value: usize) {}

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn log_ast_counters() {}
