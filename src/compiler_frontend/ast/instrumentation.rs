//! Detailed AST build instrumentation.
//!
//! WHAT: tracks temporary churn counters for the AST pipeline refactor.
//! WHY: phase 0 needs objective measurements before structural changes start, while normal
//! compiler output must remain unchanged.

#[derive(Copy, Clone)]
pub(crate) enum AstCounter {
    ScopeContextsCreated,
    ScopeLocalDeclarationsClonedTotal,
    BoundedExpressionTokenWindows,
    BoundedExpressionTokenCopiesAvoided,
    RuntimeRpnUnchangedFolds,
    TemplateNormalizationNodesVisited,
    ModuleConstantNormalizationExpressionsVisited,
}

#[cfg(feature = "detailed_timers")]
mod detailed {
    use super::AstCounter;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SCOPE_CONTEXTS_CREATED: AtomicUsize = AtomicUsize::new(0);
    static SCOPE_LOCAL_DECLARATIONS_CLONED_TOTAL: AtomicUsize = AtomicUsize::new(0);
    static BOUNDED_EXPRESSION_TOKEN_WINDOWS: AtomicUsize = AtomicUsize::new(0);
    static BOUNDED_EXPRESSION_TOKEN_COPIES_AVOIDED: AtomicUsize = AtomicUsize::new(0);
    static RUNTIME_RPN_UNCHANGED_FOLDS: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATE_NORMALIZATION_NODES_VISITED: AtomicUsize = AtomicUsize::new(0);
    static MODULE_CONSTANT_NORMALIZATION_EXPRESSIONS_VISITED: AtomicUsize = AtomicUsize::new(0);

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

    pub(crate) fn log_ast_counters() {
        let scope_contexts_created = counter_value(AstCounter::ScopeContextsCreated);
        let scope_local_declarations_cloned_total =
            counter_value(AstCounter::ScopeLocalDeclarationsClonedTotal);
        let bounded_expression_token_windows =
            counter_value(AstCounter::BoundedExpressionTokenWindows);
        let bounded_expression_token_copies_avoided =
            counter_value(AstCounter::BoundedExpressionTokenCopiesAvoided);
        let runtime_rpn_unchanged_folds = counter_value(AstCounter::RuntimeRpnUnchangedFolds);
        let template_normalization_nodes_visited =
            counter_value(AstCounter::TemplateNormalizationNodesVisited);
        let module_constant_normalization_expressions_visited =
            counter_value(AstCounter::ModuleConstantNormalizationExpressionsVisited);

        saying::say!("AST/churn counters:");
        saying::say!(
            "  scope contexts created = ",
            Dark Green scope_contexts_created
        );
        saying::say!(
            "  scope local declarations cloned total = ",
            Dark Green scope_local_declarations_cloned_total
        );
        saying::say!(
            "  bounded expression token windows = ",
            Dark Green bounded_expression_token_windows
        );
        saying::say!(
            "  bounded expression token copies avoided = ",
            Dark Green bounded_expression_token_copies_avoided
        );
        saying::say!("  runtime RPN unchanged folds = ", Dark Green runtime_rpn_unchanged_folds);
        saying::say!(
            "  template normalization nodes visited = ",
            Dark Green template_normalization_nodes_visited
        );
        saying::say!(
            "  module constant normalization expressions visited = ",
            Dark Green module_constant_normalization_expressions_visited
        );
    }

    fn all_counters() -> [AstCounter; 7] {
        [
            AstCounter::ScopeContextsCreated,
            AstCounter::ScopeLocalDeclarationsClonedTotal,
            AstCounter::BoundedExpressionTokenWindows,
            AstCounter::BoundedExpressionTokenCopiesAvoided,
            AstCounter::RuntimeRpnUnchangedFolds,
            AstCounter::TemplateNormalizationNodesVisited,
            AstCounter::ModuleConstantNormalizationExpressionsVisited,
        ]
    }

    fn atomic_counter(counter: AstCounter) -> &'static AtomicUsize {
        match counter {
            AstCounter::ScopeContextsCreated => &SCOPE_CONTEXTS_CREATED,

            AstCounter::ScopeLocalDeclarationsClonedTotal => &SCOPE_LOCAL_DECLARATIONS_CLONED_TOTAL,

            AstCounter::BoundedExpressionTokenWindows => &BOUNDED_EXPRESSION_TOKEN_WINDOWS,

            AstCounter::BoundedExpressionTokenCopiesAvoided => {
                &BOUNDED_EXPRESSION_TOKEN_COPIES_AVOIDED
            }

            AstCounter::RuntimeRpnUnchangedFolds => &RUNTIME_RPN_UNCHANGED_FOLDS,

            AstCounter::TemplateNormalizationNodesVisited => &TEMPLATE_NORMALIZATION_NODES_VISITED,

            AstCounter::ModuleConstantNormalizationExpressionsVisited => {
                &MODULE_CONSTANT_NORMALIZATION_EXPRESSIONS_VISITED
            }
        }
    }

    fn counter_value(counter: AstCounter) -> usize {
        atomic_counter(counter).load(Ordering::Relaxed)
    }
}

#[cfg(feature = "detailed_timers")]
pub(crate) use detailed::{
    add_ast_counter, increment_ast_counter, log_ast_counters, reset_ast_counters,
};

// Stubs when detailed timers are disabled.
#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn reset_ast_counters() {}

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn increment_ast_counter(_counter: AstCounter) {}

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn add_ast_counter(_counter: AstCounter, _amount: usize) {}

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn log_ast_counters() {}
