//! Detailed AST build instrumentation.
//!
//! WHAT: tracks temporary churn counters for the AST pipeline refactor.
//! WHY: phase 0 needs objective measurements before structural changes start, while normal
//! compiler output must remain unchanged.

#[derive(Copy, Clone)]
pub(crate) enum AstCounter {
    ScopeContextsCreated,
    ScopeLocalDeclarationsClonedTotal,
    ConstantResolutionRounds,
    BoundedExpressionTokenCopies,
    BoundedExpressionTokensCopiedTotal,
    RuntimeRpnCloneCount,
    TemplateNormalizationNodesVisited,
    ModuleConstantNormalizationExpressionsVisited,
}

#[cfg(feature = "detailed_timers")]
mod detailed {
    use super::AstCounter;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SCOPE_CONTEXTS_CREATED: AtomicUsize = AtomicUsize::new(0);
    static SCOPE_LOCAL_DECLARATIONS_CLONED_TOTAL: AtomicUsize = AtomicUsize::new(0);
    static CONSTANT_RESOLUTION_ROUNDS: AtomicUsize = AtomicUsize::new(0);
    static BOUNDED_EXPRESSION_TOKEN_COPIES: AtomicUsize = AtomicUsize::new(0);
    static BOUNDED_EXPRESSION_TOKENS_COPIED_TOTAL: AtomicUsize = AtomicUsize::new(0);
    static RUNTIME_RPN_CLONE_COUNT: AtomicUsize = AtomicUsize::new(0);
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
        let constant_resolution_rounds = counter_value(AstCounter::ConstantResolutionRounds);
        let bounded_expression_token_copies =
            counter_value(AstCounter::BoundedExpressionTokenCopies);
        let bounded_expression_tokens_copied_total =
            counter_value(AstCounter::BoundedExpressionTokensCopiedTotal);
        let runtime_rpn_clone_count = counter_value(AstCounter::RuntimeRpnCloneCount);
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
            "  constant resolution rounds = ",
            Dark Green constant_resolution_rounds
        );
        saying::say!(
            "  bounded expression token copies = ",
            Dark Green bounded_expression_token_copies
        );
        saying::say!(
            "  bounded expression tokens copied total = ",
            Dark Green bounded_expression_tokens_copied_total
        );
        saying::say!("  runtime RPN clone count = ", Dark Green runtime_rpn_clone_count);
        saying::say!(
            "  template normalization nodes visited = ",
            Dark Green template_normalization_nodes_visited
        );
        saying::say!(
            "  module constant normalization expressions visited = ",
            Dark Green module_constant_normalization_expressions_visited
        );
    }

    fn all_counters() -> [AstCounter; 8] {
        [
            AstCounter::ScopeContextsCreated,
            AstCounter::ScopeLocalDeclarationsClonedTotal,
            AstCounter::ConstantResolutionRounds,
            AstCounter::BoundedExpressionTokenCopies,
            AstCounter::BoundedExpressionTokensCopiedTotal,
            AstCounter::RuntimeRpnCloneCount,
            AstCounter::TemplateNormalizationNodesVisited,
            AstCounter::ModuleConstantNormalizationExpressionsVisited,
        ]
    }

    fn atomic_counter(counter: AstCounter) -> &'static AtomicUsize {
        match counter {
            AstCounter::ScopeContextsCreated => &SCOPE_CONTEXTS_CREATED,
            AstCounter::ScopeLocalDeclarationsClonedTotal => &SCOPE_LOCAL_DECLARATIONS_CLONED_TOTAL,
            AstCounter::ConstantResolutionRounds => &CONSTANT_RESOLUTION_ROUNDS,
            AstCounter::BoundedExpressionTokenCopies => &BOUNDED_EXPRESSION_TOKEN_COPIES,
            AstCounter::BoundedExpressionTokensCopiedTotal => {
                &BOUNDED_EXPRESSION_TOKENS_COPIED_TOTAL
            }
            AstCounter::RuntimeRpnCloneCount => &RUNTIME_RPN_CLONE_COUNT,
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

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn reset_ast_counters() {}

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn increment_ast_counter(_counter: AstCounter) {}

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn add_ast_counter(_counter: AstCounter, _amount: usize) {}

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn log_ast_counters() {}
