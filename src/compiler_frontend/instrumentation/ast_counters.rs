//! Detailed AST build instrumentation.
//!
//! WHAT: tracks local-only AST churn counters for performance-sensitive parser, emitter, and
//! finalizer paths.
//! WHY: benchmark runs built with `benchmark_counters` need objective evidence for small timing
//! shifts, while normal compiler output must remain unchanged. Counter storage and logging are
//! gated by `benchmark_counters`, independent of `detailed_timers`; AST substage *timings*
//! remain gated by `detailed_timers`.

#[repr(usize)]
#[derive(Copy, Clone)]
// Counter variants are intentionally dormant in normal builds where storage is disabled.
#[cfg_attr(not(feature = "benchmark_counters"), allow(dead_code))]
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
    TemplateCompositionPasses,
    TemplateWrapperApplications,
    TemplateFoldLoopIterations,
    TemplateNormalizationNodesVisited,
    ModuleConstantNormalizationExpressionsVisited,
    TemplatesFoldedDuringFinalization,

    // TIR-native head-chain composition counters.
    TemplateTirHeadChainCompositionCalls,
    TemplateTirHeadChainCompositionHits,

    // TIR-native `$children(..)` wrapper application counters.
    TemplateTirChildWrapperCalls,
    TemplateTirChildWrapperHits,

    RuntimeTemplateHandoffsRefreshedForHir,
    RuntimeSlotApplicationPlansBuilt,
    RuntimeSlotSourcesPlanned,
    RuntimeSlotSitesPlanned,
    RuntimeSlotHandoffsMaterialized,
    RuntimeSlotHandoffOwnedNodesMaterialized,
    RuntimeTemplateHandoffsMaterialized,

    // Additional template churn pressure.
    TemplateNestedTemplateParses,
    TemplateBodyTokenVisits,
    TemplateTextBytesParsed,
    TemplateFoldOutputBytes,
    TemplateEstimatedFoldOutputBytes,
    TemplateFoldOutputEstimateMissBytes,
    TemplateFoldStringInternCalls,
    TemplateFoldExpressionCloneRequests,
    TemplateFoldExpressionOwnedRewrites,
    TemplateFoldBindingSubstitutions,

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

    // Template IR (TIR) store and converter pressure.
    TirTemplatesCreated,
    TirNodesCreated,
    TirTextNodesCreated,
    TirTextBytesRecorded,
    TirMaxDepth,
    TirWrapperSetsCreated,
    TirWrapperSetReuseHits,
    #[cfg(any(test, feature = "benchmark_counters"))]
    TirValidationNodesVisited,
    TirPreparationAttempts,
    TirPreparationNodesVisited,

    // TIR fold counters.
    TirFoldTemplatesFolded,
    TirFoldNodesVisited,
    TirFoldOutputBytes,
    TirFoldStringInternCalls,

    // Phase 1 TIR attribution counters.
    //
    // WHAT: fine-grained attribution for current-state materialization,
    // module-store view folds, fold-cache behavior, and full
    // `TemplateIrStore` clone sites, so Phase 2 can rank remaining clone and
    // current-state materialization costs.
    // WHY: the existing `ast_tir_templates_created` / `ast_tir_nodes_created`
    // counters measure broad TIR materialization volume, but they cannot
    // answer which current-state call sites or fold paths drove that volume.
    // These counters add call-site and path attribution without changing fold
    // behavior.
    /// Total current-state materialization calls that actually built a TIR
    /// tree (excludes the simple formatted-reference shortcut).

    /// TIR templates created by current-state materialization only.
    TirCurrentStateTemplatesCreated,

    /// TIR nodes created by current-state materialization only.
    TirCurrentStateNodesCreated,

    /// Current-state materialization called from the finalization fallback
    /// fold path in `template_helpers`.

    /// Current-state materialization called from `Template::fold_to_emission`.

    /// Current-state materialization called from TIR classification helpers.

    /// Current-state materialization called while refreshing a template's
    /// generic kind from TIR.

    /// Current-state materialization called while building runtime-template HIR
    /// handoffs during AST finalization.

    /// Recursive current-state child materialization performed by the
    /// materializer itself after a root caller has already entered. This counts
    /// child templates that still require a fresh current-state rebuild.

    /// Recursive current-state child materialization skipped because the child
    /// already owns an authoritative same-store formatted TIR root and no active
    /// runtime slot plan needs to be threaded through it.

    /// Recursive child formatted-root shortcut missed because the current TIR
    /// reference has not reached the formatted phase.

    /// Recursive child formatted-root shortcut missed because `ContentMirror`
    /// authority only admits narrow text/direct-dynamic shapes.

    /// Finalization view fold attempt that passed reference and store
    /// validation and reached view construction.
    TirFinalizationFoldAttempts,

    /// Finalization view fold completed (folded or classified
    /// non-renderable through the view path, without falling back).
    TirFinalizationFoldSuccesses,

    /// Total `fold_tir_view` entries (module-store view folds from any
    /// caller: finalization, doc fragments, HIR handoff).
    TirViewFoldsAttempted,

    /// `fold_tir_view` ran with neither an expression nor a slot overlay.
    TirViewFoldOverlayEmpty,

    /// `fold_tir_view` ran with an expression overlay but no slot overlay.
    TirViewFoldOverlayExpressionOnly,

    /// `fold_tir_view` ran with a slot overlay but no expression overlay.
    TirViewFoldOverlaySlotOnly,

    /// `fold_tir_view` ran with both an expression and a slot overlay.
    TirViewFoldOverlayExpressionAndSlot,

    /// `fold_tir_view` ran with a wrapper-context overlay present
    /// (orthogonal to the expression/slot shape).
    TirViewFoldWrapperContextPresent,

    /// `fold_tir_view` cache lookups that returned a previously cached emission.
    TirFoldCacheHits,

    /// `fold_tir_view` cache lookups that missed and recomputed the fold.
    TirFoldCacheMisses,

    /// Full `TemplateIrStore` clones in AST finalization
    /// (`template_helpers`): the module-store view fold clone and the
    /// fold-context snapshot clone.
    TirStoreCloneFinalization,

    /// Full `TemplateIrStore` clones during doc-fragment folding.
    TirStoreCloneDocFragments,

    /// Full `TemplateIrStore` clones during HIR runtime-template handoff
    /// folding.
    TirStoreCloneHirHandoff,
}
#[cfg(feature = "benchmark_counters")]
use crate::compiler_frontend::compiler_messages::compiler_dev_logging::log_benchmark_counter;

#[cfg(feature = "benchmark_counters")]
mod detailed {
    use super::AstCounter;
    use super::log_benchmark_counter;
    use std::cell::RefCell;

    const COUNTER_COUNT: usize = AstCounter::TirStoreCloneHirHandoff as usize + 1;

    thread_local! {
        /// Per-thread AST counter store.
        ///
        /// WHAT: each concurrently compiled module/task gets an isolated counter set
        /// so that reset/add/log cycles on one worker cannot corrupt another worker's
        /// snapshot.
        /// WHY: AST construction runs inside rayon worker threads; process-global
        /// atomics were reset by overlapping module builds, producing impossible
        /// detailed counter snapshots.
        static COUNTERS: RefCell<[usize; COUNTER_COUNT]> = const { RefCell::new([0; COUNTER_COUNT]) };
    }

    impl AstCounter {
        /// Stable dense index for this counter in the per-thread [`COUNTERS`] array.
        fn index(self) -> usize {
            self as usize
        }
    }

    pub(crate) fn reset_ast_counters() {
        COUNTERS.with(|counters| counters.borrow_mut().fill(0));
    }

    pub(crate) fn increment_ast_counter(counter: AstCounter) {
        add_ast_counter(counter, 1);
    }

    pub(crate) fn add_ast_counter(counter: AstCounter, amount: usize) {
        let index = counter.index();
        COUNTERS.with(|counters| counters.borrow_mut()[index] += amount);
    }

    pub(crate) fn record_ast_counter_max(counter: AstCounter, value: usize) {
        let index = counter.index();
        COUNTERS.with(|counters| {
            let mut array = counters.borrow_mut();
            if value > array[index] {
                array[index] = value;
            }
        });
    }

    pub(crate) fn log_ast_counters() {
        // The legacy per-counter human dump only prints in `BST_COUNTERS=full`.
        // Stable `BST_BENCH counter` lines (summary/full) are emitted inside
        // `log_benchmark_counter`, so `off` and `summary` stay quiet of per-line
        // prose here.
        let print_human_counters =
            crate::timing::current_counter_output_mode().emits_human_counter_prose();

        if print_human_counters {
            saying::say!("AST/churn counters:");
        }

        for &counter in all_counters() {
            let value = counter_value(counter);
            log_benchmark_counter(counter_metric_name(counter), value as f64);

            if print_human_counters {
                saying::say!("  ", counter_label(counter), " = ", Dark Green value);
            }
        }
    }

    fn all_counters() -> &'static [AstCounter] {
        &[
            AstCounter::ScopeContextsCreated,
            AstCounter::ScopeMaxFrameDepth,
            AstCounter::ScopeFrameLookupAncestorSteps,
            AstCounter::ScopeFrameRedeclarationAncestorChecks,
            AstCounter::ScopeLocalDeclarationsInserted,
            AstCounter::BoundedExpressionTokenWindows,
            AstCounter::BoundedExpressionTokenCopiesAvoided,
            AstCounter::TemplateCompositionPasses,
            AstCounter::TemplateWrapperApplications,
            AstCounter::TemplateFoldLoopIterations,
            AstCounter::TemplateNormalizationNodesVisited,
            AstCounter::ModuleConstantNormalizationExpressionsVisited,
            AstCounter::TemplatesFoldedDuringFinalization,
            AstCounter::TemplateTirHeadChainCompositionCalls,
            AstCounter::TemplateTirHeadChainCompositionHits,
            AstCounter::TemplateTirChildWrapperCalls,
            AstCounter::TemplateTirChildWrapperHits,
            AstCounter::RuntimeTemplateHandoffsRefreshedForHir,
            AstCounter::RuntimeSlotApplicationPlansBuilt,
            AstCounter::RuntimeSlotSourcesPlanned,
            AstCounter::RuntimeSlotSitesPlanned,
            AstCounter::RuntimeSlotHandoffsMaterialized,
            AstCounter::RuntimeSlotHandoffOwnedNodesMaterialized,
            AstCounter::RuntimeTemplateHandoffsMaterialized,
            AstCounter::TemplateNestedTemplateParses,
            AstCounter::TemplateBodyTokenVisits,
            AstCounter::TemplateTextBytesParsed,
            AstCounter::TemplateFoldOutputBytes,
            AstCounter::TemplateEstimatedFoldOutputBytes,
            AstCounter::TemplateFoldOutputEstimateMissBytes,
            AstCounter::TemplateFoldStringInternCalls,
            AstCounter::TemplateFoldExpressionCloneRequests,
            AstCounter::TemplateFoldExpressionOwnedRewrites,
            AstCounter::TemplateFoldBindingSubstitutions,
            AstCounter::TypeResolutionCalls,
            AstCounter::VisibleTypeLookupAttempts,
            AstCounter::VisibleTypeAliasLookupAttempts,
            AstCounter::VisibleSourceTypeLookupAttempts,
            AstCounter::ReceiverCatalogHeadersScanned,
            AstCounter::ReceiverMethodsRegistered,
            AstCounter::DeclarationTableReplacements,
            AstCounter::PublicSurfaceValidationChecks,
            AstCounter::PostfixReceiverNodesCopied,
            AstCounter::TirTemplatesCreated,
            AstCounter::TirNodesCreated,
            AstCounter::TirTextNodesCreated,
            AstCounter::TirTextBytesRecorded,
            AstCounter::TirMaxDepth,
            AstCounter::TirWrapperSetsCreated,
            AstCounter::TirWrapperSetReuseHits,
            #[cfg(any(test, feature = "benchmark_counters"))]
            AstCounter::TirValidationNodesVisited,
            AstCounter::TirPreparationAttempts,
            AstCounter::TirPreparationNodesVisited,
            AstCounter::TirFoldTemplatesFolded,
            AstCounter::TirFoldNodesVisited,
            AstCounter::TirFoldOutputBytes,
            AstCounter::TirFoldStringInternCalls,
            AstCounter::TirCurrentStateTemplatesCreated,
            AstCounter::TirCurrentStateNodesCreated,
            AstCounter::TirFinalizationFoldAttempts,
            AstCounter::TirFinalizationFoldSuccesses,
            AstCounter::TirViewFoldsAttempted,
            AstCounter::TirViewFoldOverlayEmpty,
            AstCounter::TirViewFoldOverlayExpressionOnly,
            AstCounter::TirViewFoldOverlaySlotOnly,
            AstCounter::TirViewFoldOverlayExpressionAndSlot,
            AstCounter::TirViewFoldWrapperContextPresent,
            AstCounter::TirFoldCacheHits,
            AstCounter::TirFoldCacheMisses,
            AstCounter::TirStoreCloneFinalization,
            AstCounter::TirStoreCloneDocFragments,
            AstCounter::TirStoreCloneHirHandoff,
        ]
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
            AstCounter::TemplateCompositionPasses => "template composition passes",
            AstCounter::TemplateWrapperApplications => "template wrapper applications",
            AstCounter::TemplateFoldLoopIterations => "template fold loop iterations",
            AstCounter::TemplateNormalizationNodesVisited => "template normalization nodes visited",
            AstCounter::ModuleConstantNormalizationExpressionsVisited => {
                "module constant normalization expressions visited"
            }
            AstCounter::TemplatesFoldedDuringFinalization => "templates folded during finalization",

            AstCounter::TemplateTirHeadChainCompositionCalls => "TIR head-chain composition calls",
            AstCounter::TemplateTirHeadChainCompositionHits => "TIR head-chain composition hits",
            AstCounter::TemplateTirChildWrapperCalls => "TIR child wrapper calls",
            AstCounter::TemplateTirChildWrapperHits => "TIR child wrapper hits",

            AstCounter::RuntimeTemplateHandoffsRefreshedForHir => {
                "runtime template handoffs refreshed for HIR"
            }
            AstCounter::RuntimeSlotApplicationPlansBuilt => "runtime slot application plans built",
            AstCounter::RuntimeSlotSourcesPlanned => "runtime slot sources planned",
            AstCounter::RuntimeSlotSitesPlanned => "runtime slot sites planned",
            AstCounter::RuntimeSlotHandoffsMaterialized => "runtime slot handoffs materialized",
            AstCounter::RuntimeSlotHandoffOwnedNodesMaterialized => {
                "runtime slot handoff owned nodes materialized"
            }
            AstCounter::RuntimeTemplateHandoffsMaterialized => {
                "runtime template handoffs materialized"
            }
            AstCounter::TemplateNestedTemplateParses => "nested template parses",
            AstCounter::TemplateBodyTokenVisits => "template body token visits",
            AstCounter::TemplateTextBytesParsed => "template text bytes parsed",
            AstCounter::TemplateFoldOutputBytes => "template fold output bytes",
            AstCounter::TemplateEstimatedFoldOutputBytes => "template estimated fold output bytes",
            AstCounter::TemplateFoldOutputEstimateMissBytes => {
                "template fold output estimate miss bytes"
            }
            AstCounter::TemplateFoldStringInternCalls => "template fold string-intern calls",
            AstCounter::TemplateFoldExpressionCloneRequests => {
                "template fold expression clone requests"
            }
            AstCounter::TemplateFoldExpressionOwnedRewrites => {
                "template fold expression owned rewrites"
            }
            AstCounter::TemplateFoldBindingSubstitutions => "template fold binding substitutions",

            AstCounter::TypeResolutionCalls => "type-resolution calls",
            AstCounter::VisibleTypeLookupAttempts => "visible type lookup attempts",
            AstCounter::VisibleTypeAliasLookupAttempts => "visible type-alias lookup attempts",
            AstCounter::VisibleSourceTypeLookupAttempts => "visible source type lookup attempts",
            AstCounter::ReceiverCatalogHeadersScanned => "receiver catalog headers scanned",
            AstCounter::ReceiverMethodsRegistered => "receiver methods registered",
            AstCounter::DeclarationTableReplacements => "declaration table replacements",
            AstCounter::PublicSurfaceValidationChecks => "public-surface validation checks",
            AstCounter::PostfixReceiverNodesCopied => "postfix receiver nodes copied",

            AstCounter::TirTemplatesCreated => "TIR templates created",
            AstCounter::TirNodesCreated => "TIR nodes created",
            AstCounter::TirTextNodesCreated => "TIR text nodes created",
            AstCounter::TirTextBytesRecorded => "TIR text bytes recorded",
            AstCounter::TirMaxDepth => "TIR max depth",
            AstCounter::TirWrapperSetsCreated => "TIR wrapper sets created",
            AstCounter::TirWrapperSetReuseHits => "TIR wrapper set reuse hits",
            #[cfg(any(test, feature = "benchmark_counters"))]
            AstCounter::TirValidationNodesVisited => "TIR validation nodes visited",
            AstCounter::TirPreparationAttempts => "TIR preparation attempts",
            AstCounter::TirPreparationNodesVisited => "TIR preparation nodes visited",

            AstCounter::TirFoldTemplatesFolded => "TIR fold templates folded",
            AstCounter::TirFoldNodesVisited => "TIR fold nodes visited",
            AstCounter::TirFoldOutputBytes => "TIR fold output bytes",
            AstCounter::TirFoldStringInternCalls => "TIR fold string-intern calls",

            AstCounter::TirCurrentStateTemplatesCreated => "TIR current-state templates created",
            AstCounter::TirCurrentStateNodesCreated => "TIR current-state nodes created",
            AstCounter::TirFinalizationFoldAttempts => "finalization fold attempts",
            AstCounter::TirFinalizationFoldSuccesses => "finalization fold successes",
            AstCounter::TirViewFoldsAttempted => "TIR view folds attempted",
            AstCounter::TirViewFoldOverlayEmpty => "TIR view fold overlay: empty",
            AstCounter::TirViewFoldOverlayExpressionOnly => {
                "TIR view fold overlay: expression-only"
            }
            AstCounter::TirViewFoldOverlaySlotOnly => "TIR view fold overlay: slot-only",
            AstCounter::TirViewFoldOverlayExpressionAndSlot => {
                "TIR view fold overlay: expression+slot"
            }
            AstCounter::TirViewFoldWrapperContextPresent => "TIR view fold wrapper-context present",
            AstCounter::TirFoldCacheHits => "TIR fold cache hits",
            AstCounter::TirFoldCacheMisses => "TIR fold cache misses",
            AstCounter::TirStoreCloneFinalization => "TIR store clone: finalization",
            AstCounter::TirStoreCloneDocFragments => "TIR store clone: doc fragments",
            AstCounter::TirStoreCloneHirHandoff => "TIR store clone: HIR handoff",
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
            AstCounter::TemplateCompositionPasses => "ast_template_composition_passes",
            AstCounter::TemplateWrapperApplications => "ast_template_wrapper_applications",
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

            AstCounter::TemplateTirHeadChainCompositionCalls => {
                "ast_template_tir_head_chain_composition_calls"
            }
            AstCounter::TemplateTirHeadChainCompositionHits => {
                "ast_template_tir_head_chain_composition_hits"
            }
            AstCounter::TemplateTirChildWrapperCalls => "ast_template_tir_child_wrapper_calls",
            AstCounter::TemplateTirChildWrapperHits => "ast_template_tir_child_wrapper_hits",

            AstCounter::RuntimeTemplateHandoffsRefreshedForHir => {
                "ast_runtime_template_handoffs_refreshed_for_hir"
            }
            AstCounter::RuntimeSlotApplicationPlansBuilt => {
                "ast_runtime_slot_application_plans_built"
            }
            AstCounter::RuntimeSlotSourcesPlanned => "ast_runtime_slot_sources_planned",
            AstCounter::RuntimeSlotSitesPlanned => "ast_runtime_slot_sites_planned",
            AstCounter::RuntimeSlotHandoffsMaterialized => "ast_runtime_slot_handoffs_materialized",
            AstCounter::RuntimeSlotHandoffOwnedNodesMaterialized => {
                "ast_runtime_slot_handoff_owned_nodes_materialized"
            }
            AstCounter::RuntimeTemplateHandoffsMaterialized => {
                "ast_runtime_template_handoffs_materialized"
            }
            AstCounter::TemplateNestedTemplateParses => "ast_template_nested_template_parses",
            AstCounter::TemplateBodyTokenVisits => "ast_template_body_token_visits",
            AstCounter::TemplateTextBytesParsed => "ast_template_text_bytes_parsed",
            AstCounter::TemplateFoldOutputBytes => "ast_template_fold_output_bytes",
            AstCounter::TemplateEstimatedFoldOutputBytes => {
                "ast_template_estimated_fold_output_bytes"
            }
            AstCounter::TemplateFoldOutputEstimateMissBytes => {
                "ast_template_fold_output_estimate_miss_bytes"
            }
            AstCounter::TemplateFoldStringInternCalls => "ast_template_fold_string_intern_calls",
            AstCounter::TemplateFoldExpressionCloneRequests => {
                "ast_template_fold_expression_clone_requests"
            }
            AstCounter::TemplateFoldExpressionOwnedRewrites => {
                "ast_template_fold_expression_owned_rewrites"
            }
            AstCounter::TemplateFoldBindingSubstitutions => {
                "ast_template_fold_binding_substitutions"
            }

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

            AstCounter::TirTemplatesCreated => "ast_tir_templates_created",
            AstCounter::TirNodesCreated => "ast_tir_nodes_created",
            AstCounter::TirTextNodesCreated => "ast_tir_text_nodes_created",
            AstCounter::TirTextBytesRecorded => "ast_tir_text_bytes_recorded",
            AstCounter::TirMaxDepth => "ast_tir_max_depth",
            AstCounter::TirWrapperSetsCreated => "ast_tir_wrapper_sets_created",
            AstCounter::TirWrapperSetReuseHits => "ast_tir_wrapper_set_reuse_hits",
            #[cfg(any(test, feature = "benchmark_counters"))]
            AstCounter::TirValidationNodesVisited => "ast_tir_validation_nodes_visited",
            AstCounter::TirPreparationAttempts => "ast_tir_preparation_attempts",
            AstCounter::TirPreparationNodesVisited => "ast_tir_preparation_nodes_visited",

            AstCounter::TirFoldTemplatesFolded => "ast_tir_fold_templates_folded",
            AstCounter::TirFoldNodesVisited => "ast_tir_fold_nodes_visited",
            AstCounter::TirFoldOutputBytes => "ast_tir_fold_output_bytes",
            AstCounter::TirFoldStringInternCalls => "ast_tir_fold_string_intern_calls",

            AstCounter::TirCurrentStateTemplatesCreated => {
                "ast_tir_current_state_templates_created"
            }
            AstCounter::TirCurrentStateNodesCreated => "ast_tir_current_state_nodes_created",
            AstCounter::TirFinalizationFoldAttempts => "ast_tir_finalization_fold_attempts",
            AstCounter::TirFinalizationFoldSuccesses => "ast_tir_finalization_fold_successes",
            AstCounter::TirViewFoldsAttempted => "ast_tir_view_folds_attempted",
            AstCounter::TirViewFoldOverlayEmpty => "ast_tir_view_fold_overlay_empty",
            AstCounter::TirViewFoldOverlayExpressionOnly => {
                "ast_tir_view_fold_overlay_expression_only"
            }
            AstCounter::TirViewFoldOverlaySlotOnly => "ast_tir_view_fold_overlay_slot_only",
            AstCounter::TirViewFoldOverlayExpressionAndSlot => {
                "ast_tir_view_fold_overlay_expression_and_slot"
            }
            AstCounter::TirViewFoldWrapperContextPresent => {
                "ast_tir_view_fold_wrapper_context_present"
            }
            AstCounter::TirFoldCacheHits => "ast_tir_fold_cache_hits",
            AstCounter::TirFoldCacheMisses => "ast_tir_fold_cache_misses",
            AstCounter::TirStoreCloneFinalization => "ast_tir_store_clone_finalization",
            AstCounter::TirStoreCloneDocFragments => "ast_tir_store_clone_doc_fragments",
            AstCounter::TirStoreCloneHirHandoff => "ast_tir_store_clone_hir_handoff",
        }
    }

    fn counter_value(counter: AstCounter) -> usize {
        let index = counter.index();
        COUNTERS.with(|counters| counters.borrow()[index])
    }

    /// Test-only readback for per-thread AST counter values.
    ///
    /// WHAT: lets unit tests assert that a specific production path incremented
    ///       the expected counter without relying on stdout or the benchmark
    ///       collector, which would need cross-test serialization.
    /// WHY: the public instrumentation API is intentionally write-only so normal
    ///      compiler code cannot read stale counter state.
    #[cfg(test)]
    pub(crate) fn test_read_ast_counter(counter: AstCounter) -> usize {
        counter_value(counter)
    }
}

#[cfg(feature = "benchmark_counters")]
pub(crate) use detailed::{
    add_ast_counter, increment_ast_counter, log_ast_counters, record_ast_counter_max,
    reset_ast_counters,
};

#[cfg(all(test, feature = "benchmark_counters"))]
pub(crate) use detailed::test_read_ast_counter;

// Stubs when detailed timers are disabled.
#[cfg(not(feature = "benchmark_counters"))]
pub(crate) fn reset_ast_counters() {}

#[cfg(not(feature = "benchmark_counters"))]
pub(crate) fn increment_ast_counter(_counter: AstCounter) {}

#[cfg(not(feature = "benchmark_counters"))]
pub(crate) fn add_ast_counter(_counter: AstCounter, _amount: usize) {}

#[cfg(not(feature = "benchmark_counters"))]
pub(crate) fn record_ast_counter_max(_counter: AstCounter, _value: usize) {}

#[cfg(not(feature = "benchmark_counters"))]
pub(crate) fn log_ast_counters() {}
