//! Detailed AST build instrumentation.
//!
//! WHAT: tracks local-only AST churn counters for performance-sensitive parser, emitter, and
//! finalizer paths.
//! WHY: benchmark runs need objective evidence for small timing shifts, while normal compiler
//! output must remain unchanged.

#[derive(Copy, Clone)]
pub(crate) enum AstCounter {
    // Scope-frame churn.
    ScopeContextsCreated,
    ScopeMaxFrameDepth,
    ScopeFrameLookupAncestorSteps,
    ScopeFrameRedeclarationAncestorChecks,
    ScopeLocalDeclarationsInserted,

    // Expression parser pressure.
    BoundedExpressionTokenWindows,
    BoundedExpressionTokenCopiesAvoided,

    // Template parsing, composition, planning, and folding pressure.
    TemplateAtomsParsed,
    TemplateCompositionPasses,
    TemplateWrapperApplications,
    TemplateRenderPlansBuilt,
    TemplateRenderPiecesBuilt,
    TemplateRenderPlanCloneCalls,
    TemplateRenderPiecesCloned,
    TemplateFoldPlanPiecesVisited,
    TemplateFoldFallbackPlanBuilds,
    TemplateFoldLoopIterations,
    TemplateNormalizationNodesVisited,
    ModuleConstantNormalizationExpressionsVisited,
    TemplatesFoldedDuringFinalization,
    RuntimeRenderPlansRebuilt,
    RuntimeSlotApplicationPlansBuilt,
    RuntimeSlotSourcesPlanned,
    RuntimeSlotSitesPlanned,

    // AST environment/type-resolution pressure.
    TypeResolutionCalls,
    VisibleTypeLookupAttempts,
    VisibleTypeAliasLookupAttempts,
    VisibleSourceTypeLookupAttempts,
    ReceiverCatalogHeadersScanned,
    ReceiverMethodsRegistered,
    DeclarationTableReplacements,
    PublicSurfaceValidationChecks,

    // Field/receiver lowering pressure.
    PostfixReceiverNodesCopied,
}

#[cfg(feature = "detailed_timers")]
use crate::compiler_frontend::compiler_messages::compiler_dev_logging::{
    detailed_timer_output_enabled, log_benchmark_counter,
};

#[cfg(feature = "detailed_timers")]
mod detailed {
    use super::AstCounter;
    use super::{detailed_timer_output_enabled, log_benchmark_counter};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SCOPE_CONTEXTS_CREATED: AtomicUsize = AtomicUsize::new(0);
    static SCOPE_MAX_FRAME_DEPTH: AtomicUsize = AtomicUsize::new(0);
    static SCOPE_FRAME_LOOKUP_ANCESTOR_STEPS: AtomicUsize = AtomicUsize::new(0);
    static SCOPE_FRAME_REDECLARATION_ANCESTOR_CHECKS: AtomicUsize = AtomicUsize::new(0);
    static SCOPE_LOCAL_DECLARATIONS_INSERTED: AtomicUsize = AtomicUsize::new(0);
    static BOUNDED_EXPRESSION_TOKEN_WINDOWS: AtomicUsize = AtomicUsize::new(0);
    static BOUNDED_EXPRESSION_TOKEN_COPIES_AVOIDED: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATE_ATOMS_PARSED: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATE_COMPOSITION_PASSES: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATE_WRAPPER_APPLICATIONS: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATE_RENDER_PLANS_BUILT: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATE_RENDER_PIECES_BUILT: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATE_RENDER_PLAN_CLONE_CALLS: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATE_RENDER_PIECES_CLONED: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATE_FOLD_PLAN_PIECES_VISITED: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATE_FOLD_FALLBACK_PLAN_BUILDS: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATE_FOLD_LOOP_ITERATIONS: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATE_NORMALIZATION_NODES_VISITED: AtomicUsize = AtomicUsize::new(0);
    static MODULE_CONSTANT_NORMALIZATION_EXPRESSIONS_VISITED: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATES_FOLDED_DURING_FINALIZATION: AtomicUsize = AtomicUsize::new(0);
    static RUNTIME_RENDER_PLANS_REBUILT: AtomicUsize = AtomicUsize::new(0);
    static RUNTIME_SLOT_APPLICATION_PLANS_BUILT: AtomicUsize = AtomicUsize::new(0);
    static RUNTIME_SLOT_SOURCES_PLANNED: AtomicUsize = AtomicUsize::new(0);
    static RUNTIME_SLOT_SITES_PLANNED: AtomicUsize = AtomicUsize::new(0);
    static TYPE_RESOLUTION_CALLS: AtomicUsize = AtomicUsize::new(0);
    static VISIBLE_TYPE_LOOKUP_ATTEMPTS: AtomicUsize = AtomicUsize::new(0);
    static VISIBLE_TYPE_ALIAS_LOOKUP_ATTEMPTS: AtomicUsize = AtomicUsize::new(0);
    static VISIBLE_SOURCE_TYPE_LOOKUP_ATTEMPTS: AtomicUsize = AtomicUsize::new(0);
    static RECEIVER_CATALOG_HEADERS_SCANNED: AtomicUsize = AtomicUsize::new(0);
    static RECEIVER_METHODS_REGISTERED: AtomicUsize = AtomicUsize::new(0);
    static DECLARATION_TABLE_REPLACEMENTS: AtomicUsize = AtomicUsize::new(0);
    static PUBLIC_SURFACE_VALIDATION_CHECKS: AtomicUsize = AtomicUsize::new(0);
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
        let print_human_counters = detailed_timer_output_enabled();

        if print_human_counters {
            saying::say!("AST/churn counters:");
        }

        for counter in all_counters() {
            let value = counter_value(counter);
            log_benchmark_counter(counter_metric_name(counter), value as f64);

            if print_human_counters {
                saying::say!("  ", counter_label(counter), " = ", Dark Green value);
            }
        }
    }

    fn all_counters() -> [AstCounter; 33] {
        [
            AstCounter::ScopeContextsCreated,
            AstCounter::ScopeMaxFrameDepth,
            AstCounter::ScopeFrameLookupAncestorSteps,
            AstCounter::ScopeFrameRedeclarationAncestorChecks,
            AstCounter::ScopeLocalDeclarationsInserted,
            AstCounter::BoundedExpressionTokenWindows,
            AstCounter::BoundedExpressionTokenCopiesAvoided,
            AstCounter::TemplateAtomsParsed,
            AstCounter::TemplateCompositionPasses,
            AstCounter::TemplateWrapperApplications,
            AstCounter::TemplateRenderPlansBuilt,
            AstCounter::TemplateRenderPiecesBuilt,
            AstCounter::TemplateRenderPlanCloneCalls,
            AstCounter::TemplateRenderPiecesCloned,
            AstCounter::TemplateFoldPlanPiecesVisited,
            AstCounter::TemplateFoldFallbackPlanBuilds,
            AstCounter::TemplateFoldLoopIterations,
            AstCounter::TemplateNormalizationNodesVisited,
            AstCounter::ModuleConstantNormalizationExpressionsVisited,
            AstCounter::TemplatesFoldedDuringFinalization,
            AstCounter::RuntimeRenderPlansRebuilt,
            AstCounter::RuntimeSlotApplicationPlansBuilt,
            AstCounter::RuntimeSlotSourcesPlanned,
            AstCounter::RuntimeSlotSitesPlanned,
            AstCounter::TypeResolutionCalls,
            AstCounter::VisibleTypeLookupAttempts,
            AstCounter::VisibleTypeAliasLookupAttempts,
            AstCounter::VisibleSourceTypeLookupAttempts,
            AstCounter::ReceiverCatalogHeadersScanned,
            AstCounter::ReceiverMethodsRegistered,
            AstCounter::DeclarationTableReplacements,
            AstCounter::PublicSurfaceValidationChecks,
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

            AstCounter::TemplateAtomsParsed => &TEMPLATE_ATOMS_PARSED,

            AstCounter::TemplateCompositionPasses => &TEMPLATE_COMPOSITION_PASSES,

            AstCounter::TemplateWrapperApplications => &TEMPLATE_WRAPPER_APPLICATIONS,

            AstCounter::TemplateRenderPlansBuilt => &TEMPLATE_RENDER_PLANS_BUILT,

            AstCounter::TemplateRenderPiecesBuilt => &TEMPLATE_RENDER_PIECES_BUILT,

            AstCounter::TemplateRenderPlanCloneCalls => &TEMPLATE_RENDER_PLAN_CLONE_CALLS,

            AstCounter::TemplateRenderPiecesCloned => &TEMPLATE_RENDER_PIECES_CLONED,

            AstCounter::TemplateFoldPlanPiecesVisited => &TEMPLATE_FOLD_PLAN_PIECES_VISITED,

            AstCounter::TemplateFoldFallbackPlanBuilds => &TEMPLATE_FOLD_FALLBACK_PLAN_BUILDS,

            AstCounter::TemplateFoldLoopIterations => &TEMPLATE_FOLD_LOOP_ITERATIONS,

            AstCounter::TemplateNormalizationNodesVisited => &TEMPLATE_NORMALIZATION_NODES_VISITED,

            AstCounter::ModuleConstantNormalizationExpressionsVisited => {
                &MODULE_CONSTANT_NORMALIZATION_EXPRESSIONS_VISITED
            }

            AstCounter::TemplatesFoldedDuringFinalization => &TEMPLATES_FOLDED_DURING_FINALIZATION,

            AstCounter::RuntimeRenderPlansRebuilt => &RUNTIME_RENDER_PLANS_REBUILT,

            AstCounter::RuntimeSlotApplicationPlansBuilt => &RUNTIME_SLOT_APPLICATION_PLANS_BUILT,

            AstCounter::RuntimeSlotSourcesPlanned => &RUNTIME_SLOT_SOURCES_PLANNED,

            AstCounter::RuntimeSlotSitesPlanned => &RUNTIME_SLOT_SITES_PLANNED,

            AstCounter::TypeResolutionCalls => &TYPE_RESOLUTION_CALLS,

            AstCounter::VisibleTypeLookupAttempts => &VISIBLE_TYPE_LOOKUP_ATTEMPTS,

            AstCounter::VisibleTypeAliasLookupAttempts => &VISIBLE_TYPE_ALIAS_LOOKUP_ATTEMPTS,

            AstCounter::VisibleSourceTypeLookupAttempts => &VISIBLE_SOURCE_TYPE_LOOKUP_ATTEMPTS,

            AstCounter::ReceiverCatalogHeadersScanned => &RECEIVER_CATALOG_HEADERS_SCANNED,

            AstCounter::ReceiverMethodsRegistered => &RECEIVER_METHODS_REGISTERED,

            AstCounter::DeclarationTableReplacements => &DECLARATION_TABLE_REPLACEMENTS,

            AstCounter::PublicSurfaceValidationChecks => &PUBLIC_SURFACE_VALIDATION_CHECKS,

            AstCounter::PostfixReceiverNodesCopied => &POSTFIX_RECEIVER_NODES_COPIED,
        }
    }

    fn counter_label(counter: AstCounter) -> &'static str {
        match counter {
            AstCounter::ScopeContextsCreated => "scope contexts created",
            AstCounter::ScopeMaxFrameDepth => "scope max frame depth",
            AstCounter::ScopeFrameLookupAncestorSteps => "scope frame lookup ancestor steps",
            AstCounter::ScopeFrameRedeclarationAncestorChecks => {
                "scope frame redeclaration ancestor checks"
            }
            AstCounter::ScopeLocalDeclarationsInserted => "scope local declarations inserted",
            AstCounter::BoundedExpressionTokenWindows => "bounded expression token windows",
            AstCounter::BoundedExpressionTokenCopiesAvoided => {
                "bounded expression token copies avoided"
            }
            AstCounter::TemplateAtomsParsed => "template atoms parsed",
            AstCounter::TemplateCompositionPasses => "template composition passes",
            AstCounter::TemplateWrapperApplications => "template wrapper applications",
            AstCounter::TemplateRenderPlansBuilt => "template render plans built",
            AstCounter::TemplateRenderPiecesBuilt => "template render pieces built",
            AstCounter::TemplateRenderPlanCloneCalls => "template render-plan clone calls",
            AstCounter::TemplateRenderPiecesCloned => "template render pieces cloned",
            AstCounter::TemplateFoldPlanPiecesVisited => "template fold plan pieces visited",
            AstCounter::TemplateFoldFallbackPlanBuilds => "template fold fallback plan builds",
            AstCounter::TemplateFoldLoopIterations => "template fold loop iterations",
            AstCounter::TemplateNormalizationNodesVisited => "template normalization nodes visited",
            AstCounter::ModuleConstantNormalizationExpressionsVisited => {
                "module constant normalization expressions visited"
            }
            AstCounter::TemplatesFoldedDuringFinalization => "templates folded during finalization",
            AstCounter::RuntimeRenderPlansRebuilt => "runtime render plans rebuilt",
            AstCounter::RuntimeSlotApplicationPlansBuilt => "runtime slot application plans built",
            AstCounter::RuntimeSlotSourcesPlanned => "runtime slot sources planned",
            AstCounter::RuntimeSlotSitesPlanned => "runtime slot sites planned",
            AstCounter::TypeResolutionCalls => "type-resolution calls",
            AstCounter::VisibleTypeLookupAttempts => "visible type lookup attempts",
            AstCounter::VisibleTypeAliasLookupAttempts => "visible type-alias lookup attempts",
            AstCounter::VisibleSourceTypeLookupAttempts => "visible source type lookup attempts",
            AstCounter::ReceiverCatalogHeadersScanned => "receiver catalog headers scanned",
            AstCounter::ReceiverMethodsRegistered => "receiver methods registered",
            AstCounter::DeclarationTableReplacements => "declaration table replacements",
            AstCounter::PublicSurfaceValidationChecks => "public-surface validation checks",
            AstCounter::PostfixReceiverNodesCopied => "postfix receiver nodes copied",
        }
    }

    fn counter_metric_name(counter: AstCounter) -> &'static str {
        match counter {
            AstCounter::ScopeContextsCreated => "ast_scope_contexts_created",
            AstCounter::ScopeMaxFrameDepth => "ast_scope_max_frame_depth",
            AstCounter::ScopeFrameLookupAncestorSteps => "ast_scope_frame_lookup_ancestor_steps",
            AstCounter::ScopeFrameRedeclarationAncestorChecks => {
                "ast_scope_frame_redeclaration_ancestor_checks"
            }
            AstCounter::ScopeLocalDeclarationsInserted => "ast_scope_local_declarations_inserted",
            AstCounter::BoundedExpressionTokenWindows => "ast_bounded_expression_token_windows",
            AstCounter::BoundedExpressionTokenCopiesAvoided => {
                "ast_bounded_expression_token_copies_avoided"
            }
            AstCounter::TemplateAtomsParsed => "ast_template_atoms_parsed",
            AstCounter::TemplateCompositionPasses => "ast_template_composition_passes",
            AstCounter::TemplateWrapperApplications => "ast_template_wrapper_applications",
            AstCounter::TemplateRenderPlansBuilt => "ast_template_render_plans_built",
            AstCounter::TemplateRenderPiecesBuilt => "ast_template_render_pieces_built",
            AstCounter::TemplateRenderPlanCloneCalls => "ast_template_render_plan_clone_calls",
            AstCounter::TemplateRenderPiecesCloned => "ast_template_render_pieces_cloned",
            AstCounter::TemplateFoldPlanPiecesVisited => "ast_template_fold_plan_pieces_visited",
            AstCounter::TemplateFoldFallbackPlanBuilds => "ast_template_fold_fallback_plan_builds",
            AstCounter::TemplateFoldLoopIterations => "ast_template_fold_loop_iterations",
            AstCounter::TemplateNormalizationNodesVisited => {
                "ast_template_normalization_nodes_visited"
            }
            AstCounter::ModuleConstantNormalizationExpressionsVisited => {
                "ast_module_constant_normalization_expressions_visited"
            }
            AstCounter::TemplatesFoldedDuringFinalization => {
                "ast_templates_folded_during_finalization"
            }
            AstCounter::RuntimeRenderPlansRebuilt => "ast_runtime_render_plans_rebuilt",
            AstCounter::RuntimeSlotApplicationPlansBuilt => {
                "ast_runtime_slot_application_plans_built"
            }
            AstCounter::RuntimeSlotSourcesPlanned => "ast_runtime_slot_sources_planned",
            AstCounter::RuntimeSlotSitesPlanned => "ast_runtime_slot_sites_planned",
            AstCounter::TypeResolutionCalls => "ast_type_resolution_calls",
            AstCounter::VisibleTypeLookupAttempts => "ast_visible_type_lookup_attempts",
            AstCounter::VisibleTypeAliasLookupAttempts => "ast_visible_type_alias_lookup_attempts",
            AstCounter::VisibleSourceTypeLookupAttempts => {
                "ast_visible_source_type_lookup_attempts"
            }
            AstCounter::ReceiverCatalogHeadersScanned => "ast_receiver_catalog_headers_scanned",
            AstCounter::ReceiverMethodsRegistered => "ast_receiver_methods_registered",
            AstCounter::DeclarationTableReplacements => "ast_declaration_table_replacements",
            AstCounter::PublicSurfaceValidationChecks => "ast_public_surface_validation_checks",
            AstCounter::PostfixReceiverNodesCopied => "ast_postfix_receiver_nodes_copied",
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
