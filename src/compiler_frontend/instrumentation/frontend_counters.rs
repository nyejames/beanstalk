//! Local-only frontend performance instrumentation.
//!
//! WHAT: exposes counters for clone-heavy, cache-sensitive, and remap-heavy frontend paths.
//! WHY: detailed benchmark runs need enough local evidence to interpret small
//! end-to-end timing changes, while normal compiler builds must not pay for or
//! print this diagnostic data.

/// Stable local benchmark counters grouped by the compiler stage that owns the work.
///
/// These counters are diagnostic evidence for benchmark reports. They deliberately stay in one
/// enum so the logging path has one current implementation and metric names remain stable.
#[derive(Clone, Copy)]
pub(crate) enum FrontendCounter {
    // Stage 0 and per-file preparation volume.
    ModuleCount,
    SourceFileCount,
    SourceByteCount,
    PreparedFileCount,
    TokenCount,
    HeaderCount,
    ImportCount,
    TopLevelDeclarationCount,

    // Dependency sorting volume.
    DependencyHeaderCount,
    DependencyEdgeCount,
    DependencyVisitCount,

    // AST construction and compile-time evaluation volume.
    AstHeaderCount,
    AstFunctionCount,
    AstStructCount,
    AstChoiceCount,
    AstConstantCount,
    AstTraitDeclarationCount,
    AstTraitConformanceCount,
    AstReceiverMethodCount,
    AstGenericTemplateCount,
    AstGenericInstanceCount,
    ConstantFoldAttemptCount,
    ConstantFoldSuccessCount,
    TemplateCount,
    ConstTemplateCount,
    RuntimeTemplateCount,

    // HIR and borrow-validation volume.
    HirBlockCount,
    HirStatementCount,
    HirFunctionCount,
    BorrowFunctionCount,
    BorrowBlockCount,
    BorrowConflictCheckCount,
    BorrowStateSnapshotCount,
    BorrowStatementFactCount,
    BorrowTerminatorFactCount,
    BorrowValueFactCount,

    // Implementation-pressure counters from shared frontend data structures.
    TypeEnvironmentFieldsForQueries,
    TypeEnvironmentFieldsReturned,
    TypeEnvironmentVariantsForQueries,
    TypeEnvironmentVariantsReturned,
    TypeEnvironmentSubstituteTypeIdCalls,
    TypeEnvironmentSubstitutionCacheLookups,
    TypeEnvironmentSubstitutionCacheHits,
    TypeEnvironmentSubstitutionCacheMisses,
    TypeCompatibilityCacheLookups,
    TypeCompatibilityCacheHits,
    TypeCompatibilityCacheMisses,
    StringTableFullClones,
    StringTableMergeFromSourceEntriesScanned,
    ModuleRemapStringIdsCalls,
}

#[cfg(feature = "detailed_timers")]
mod detailed {
    use super::FrontendCounter;
    use crate::compiler_frontend::compiler_messages::compiler_dev_logging::{
        detailed_timer_output_enabled, log_benchmark_counter,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TYPE_ENVIRONMENT_FIELDS_FOR_QUERIES: AtomicUsize = AtomicUsize::new(0);
    static TYPE_ENVIRONMENT_FIELDS_RETURNED: AtomicUsize = AtomicUsize::new(0);
    static TYPE_ENVIRONMENT_VARIANTS_FOR_QUERIES: AtomicUsize = AtomicUsize::new(0);
    static TYPE_ENVIRONMENT_VARIANTS_RETURNED: AtomicUsize = AtomicUsize::new(0);
    static TYPE_ENVIRONMENT_SUBSTITUTE_TYPE_ID_CALLS: AtomicUsize = AtomicUsize::new(0);
    static TYPE_ENVIRONMENT_SUBSTITUTION_CACHE_LOOKUPS: AtomicUsize = AtomicUsize::new(0);
    static TYPE_ENVIRONMENT_SUBSTITUTION_CACHE_HITS: AtomicUsize = AtomicUsize::new(0);
    static TYPE_ENVIRONMENT_SUBSTITUTION_CACHE_MISSES: AtomicUsize = AtomicUsize::new(0);
    static TYPE_COMPATIBILITY_CACHE_LOOKUPS: AtomicUsize = AtomicUsize::new(0);
    static TYPE_COMPATIBILITY_CACHE_HITS: AtomicUsize = AtomicUsize::new(0);
    static TYPE_COMPATIBILITY_CACHE_MISSES: AtomicUsize = AtomicUsize::new(0);
    static STRING_TABLE_FULL_CLONES: AtomicUsize = AtomicUsize::new(0);
    static STRING_TABLE_MERGE_FROM_SOURCE_ENTRIES_SCANNED: AtomicUsize = AtomicUsize::new(0);
    static MODULE_REMAP_STRING_IDS_CALLS: AtomicUsize = AtomicUsize::new(0);
    static MODULE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static SOURCE_FILE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static SOURCE_BYTE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static PREPARED_FILE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static TOKEN_COUNT: AtomicUsize = AtomicUsize::new(0);
    static HEADER_COUNT: AtomicUsize = AtomicUsize::new(0);
    static IMPORT_COUNT: AtomicUsize = AtomicUsize::new(0);
    static TOP_LEVEL_DECLARATION_COUNT: AtomicUsize = AtomicUsize::new(0);
    static DEPENDENCY_HEADER_COUNT: AtomicUsize = AtomicUsize::new(0);
    static DEPENDENCY_EDGE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static DEPENDENCY_VISIT_COUNT: AtomicUsize = AtomicUsize::new(0);
    static AST_HEADER_COUNT: AtomicUsize = AtomicUsize::new(0);
    static AST_FUNCTION_COUNT: AtomicUsize = AtomicUsize::new(0);
    static AST_STRUCT_COUNT: AtomicUsize = AtomicUsize::new(0);
    static AST_CHOICE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static AST_CONSTANT_COUNT: AtomicUsize = AtomicUsize::new(0);
    static AST_TRAIT_DECLARATION_COUNT: AtomicUsize = AtomicUsize::new(0);
    static AST_TRAIT_CONFORMANCE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static AST_RECEIVER_METHOD_COUNT: AtomicUsize = AtomicUsize::new(0);
    static AST_GENERIC_TEMPLATE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static AST_GENERIC_INSTANCE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static CONSTANT_FOLD_ATTEMPT_COUNT: AtomicUsize = AtomicUsize::new(0);
    static CONSTANT_FOLD_SUCCESS_COUNT: AtomicUsize = AtomicUsize::new(0);
    static TEMPLATE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static CONST_TEMPLATE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static RUNTIME_TEMPLATE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static HIR_BLOCK_COUNT: AtomicUsize = AtomicUsize::new(0);
    static HIR_STATEMENT_COUNT: AtomicUsize = AtomicUsize::new(0);
    static HIR_FUNCTION_COUNT: AtomicUsize = AtomicUsize::new(0);
    static BORROW_FUNCTION_COUNT: AtomicUsize = AtomicUsize::new(0);
    static BORROW_BLOCK_COUNT: AtomicUsize = AtomicUsize::new(0);
    static BORROW_CONFLICT_CHECK_COUNT: AtomicUsize = AtomicUsize::new(0);
    static BORROW_STATE_SNAPSHOT_COUNT: AtomicUsize = AtomicUsize::new(0);
    static BORROW_STATEMENT_FACT_COUNT: AtomicUsize = AtomicUsize::new(0);
    static BORROW_TERMINATOR_FACT_COUNT: AtomicUsize = AtomicUsize::new(0);
    static BORROW_VALUE_FACT_COUNT: AtomicUsize = AtomicUsize::new(0);

    pub(crate) fn reset_frontend_counters() {
        for counter in all_counters() {
            atomic_counter(counter).store(0, Ordering::Relaxed);
        }
    }

    pub(crate) fn increment_frontend_counter(counter: FrontendCounter) {
        add_frontend_counter(counter, 1);
    }

    pub(crate) fn add_frontend_counter(counter: FrontendCounter, amount: usize) {
        atomic_counter(counter).fetch_add(amount, Ordering::Relaxed);
    }

    pub(crate) fn log_frontend_counters() {
        let print_human_counters = detailed_timer_output_enabled();

        if print_human_counters {
            saying::say!("Frontend/performance counters:");
        }

        for counter in all_counters() {
            let value = counter_value(counter);
            log_benchmark_counter(counter_metric_name(counter), value as f64);

            if print_human_counters {
                saying::say!("  ", counter_label(counter), " = ", Dark Green value);
            }
        }
    }

    fn all_counters() -> [FrontendCounter; 50] {
        [
            FrontendCounter::ModuleCount,
            FrontendCounter::SourceFileCount,
            FrontendCounter::SourceByteCount,
            FrontendCounter::PreparedFileCount,
            FrontendCounter::TokenCount,
            FrontendCounter::HeaderCount,
            FrontendCounter::ImportCount,
            FrontendCounter::TopLevelDeclarationCount,
            FrontendCounter::DependencyHeaderCount,
            FrontendCounter::DependencyEdgeCount,
            FrontendCounter::DependencyVisitCount,
            FrontendCounter::AstHeaderCount,
            FrontendCounter::AstFunctionCount,
            FrontendCounter::AstStructCount,
            FrontendCounter::AstChoiceCount,
            FrontendCounter::AstConstantCount,
            FrontendCounter::AstTraitDeclarationCount,
            FrontendCounter::AstTraitConformanceCount,
            FrontendCounter::AstReceiverMethodCount,
            FrontendCounter::AstGenericTemplateCount,
            FrontendCounter::AstGenericInstanceCount,
            FrontendCounter::ConstantFoldAttemptCount,
            FrontendCounter::ConstantFoldSuccessCount,
            FrontendCounter::TemplateCount,
            FrontendCounter::ConstTemplateCount,
            FrontendCounter::RuntimeTemplateCount,
            FrontendCounter::HirBlockCount,
            FrontendCounter::HirStatementCount,
            FrontendCounter::HirFunctionCount,
            FrontendCounter::BorrowFunctionCount,
            FrontendCounter::BorrowBlockCount,
            FrontendCounter::BorrowConflictCheckCount,
            FrontendCounter::BorrowStateSnapshotCount,
            FrontendCounter::BorrowStatementFactCount,
            FrontendCounter::BorrowTerminatorFactCount,
            FrontendCounter::BorrowValueFactCount,
            FrontendCounter::TypeEnvironmentFieldsForQueries,
            FrontendCounter::TypeEnvironmentFieldsReturned,
            FrontendCounter::TypeEnvironmentVariantsForQueries,
            FrontendCounter::TypeEnvironmentVariantsReturned,
            FrontendCounter::TypeEnvironmentSubstituteTypeIdCalls,
            FrontendCounter::TypeEnvironmentSubstitutionCacheLookups,
            FrontendCounter::TypeEnvironmentSubstitutionCacheHits,
            FrontendCounter::TypeEnvironmentSubstitutionCacheMisses,
            FrontendCounter::TypeCompatibilityCacheLookups,
            FrontendCounter::TypeCompatibilityCacheHits,
            FrontendCounter::TypeCompatibilityCacheMisses,
            FrontendCounter::StringTableFullClones,
            FrontendCounter::StringTableMergeFromSourceEntriesScanned,
            FrontendCounter::ModuleRemapStringIdsCalls,
        ]
    }

    fn atomic_counter(counter: FrontendCounter) -> &'static AtomicUsize {
        match counter {
            FrontendCounter::ModuleCount => &MODULE_COUNT,

            FrontendCounter::SourceFileCount => &SOURCE_FILE_COUNT,

            FrontendCounter::SourceByteCount => &SOURCE_BYTE_COUNT,

            FrontendCounter::PreparedFileCount => &PREPARED_FILE_COUNT,

            FrontendCounter::TokenCount => &TOKEN_COUNT,

            FrontendCounter::HeaderCount => &HEADER_COUNT,

            FrontendCounter::ImportCount => &IMPORT_COUNT,

            FrontendCounter::TopLevelDeclarationCount => &TOP_LEVEL_DECLARATION_COUNT,

            FrontendCounter::DependencyHeaderCount => &DEPENDENCY_HEADER_COUNT,

            FrontendCounter::DependencyEdgeCount => &DEPENDENCY_EDGE_COUNT,

            FrontendCounter::DependencyVisitCount => &DEPENDENCY_VISIT_COUNT,

            FrontendCounter::AstHeaderCount => &AST_HEADER_COUNT,

            FrontendCounter::AstFunctionCount => &AST_FUNCTION_COUNT,

            FrontendCounter::AstStructCount => &AST_STRUCT_COUNT,

            FrontendCounter::AstChoiceCount => &AST_CHOICE_COUNT,

            FrontendCounter::AstConstantCount => &AST_CONSTANT_COUNT,

            FrontendCounter::AstTraitDeclarationCount => &AST_TRAIT_DECLARATION_COUNT,

            FrontendCounter::AstTraitConformanceCount => &AST_TRAIT_CONFORMANCE_COUNT,

            FrontendCounter::AstReceiverMethodCount => &AST_RECEIVER_METHOD_COUNT,

            FrontendCounter::AstGenericTemplateCount => &AST_GENERIC_TEMPLATE_COUNT,

            FrontendCounter::AstGenericInstanceCount => &AST_GENERIC_INSTANCE_COUNT,

            FrontendCounter::ConstantFoldAttemptCount => &CONSTANT_FOLD_ATTEMPT_COUNT,

            FrontendCounter::ConstantFoldSuccessCount => &CONSTANT_FOLD_SUCCESS_COUNT,

            FrontendCounter::TemplateCount => &TEMPLATE_COUNT,

            FrontendCounter::ConstTemplateCount => &CONST_TEMPLATE_COUNT,

            FrontendCounter::RuntimeTemplateCount => &RUNTIME_TEMPLATE_COUNT,

            FrontendCounter::HirBlockCount => &HIR_BLOCK_COUNT,

            FrontendCounter::HirStatementCount => &HIR_STATEMENT_COUNT,

            FrontendCounter::HirFunctionCount => &HIR_FUNCTION_COUNT,

            FrontendCounter::BorrowFunctionCount => &BORROW_FUNCTION_COUNT,

            FrontendCounter::BorrowBlockCount => &BORROW_BLOCK_COUNT,

            FrontendCounter::BorrowConflictCheckCount => &BORROW_CONFLICT_CHECK_COUNT,

            FrontendCounter::BorrowStateSnapshotCount => &BORROW_STATE_SNAPSHOT_COUNT,

            FrontendCounter::BorrowStatementFactCount => &BORROW_STATEMENT_FACT_COUNT,

            FrontendCounter::BorrowTerminatorFactCount => &BORROW_TERMINATOR_FACT_COUNT,

            FrontendCounter::BorrowValueFactCount => &BORROW_VALUE_FACT_COUNT,

            FrontendCounter::TypeEnvironmentFieldsForQueries => {
                &TYPE_ENVIRONMENT_FIELDS_FOR_QUERIES
            }

            FrontendCounter::TypeEnvironmentFieldsReturned => &TYPE_ENVIRONMENT_FIELDS_RETURNED,

            FrontendCounter::TypeEnvironmentVariantsForQueries => {
                &TYPE_ENVIRONMENT_VARIANTS_FOR_QUERIES
            }

            FrontendCounter::TypeEnvironmentVariantsReturned => &TYPE_ENVIRONMENT_VARIANTS_RETURNED,

            FrontendCounter::TypeEnvironmentSubstituteTypeIdCalls => {
                &TYPE_ENVIRONMENT_SUBSTITUTE_TYPE_ID_CALLS
            }

            FrontendCounter::TypeEnvironmentSubstitutionCacheLookups => {
                &TYPE_ENVIRONMENT_SUBSTITUTION_CACHE_LOOKUPS
            }

            FrontendCounter::TypeEnvironmentSubstitutionCacheHits => {
                &TYPE_ENVIRONMENT_SUBSTITUTION_CACHE_HITS
            }

            FrontendCounter::TypeEnvironmentSubstitutionCacheMisses => {
                &TYPE_ENVIRONMENT_SUBSTITUTION_CACHE_MISSES
            }

            FrontendCounter::TypeCompatibilityCacheLookups => &TYPE_COMPATIBILITY_CACHE_LOOKUPS,

            FrontendCounter::TypeCompatibilityCacheHits => &TYPE_COMPATIBILITY_CACHE_HITS,

            FrontendCounter::TypeCompatibilityCacheMisses => &TYPE_COMPATIBILITY_CACHE_MISSES,

            FrontendCounter::StringTableFullClones => &STRING_TABLE_FULL_CLONES,

            FrontendCounter::StringTableMergeFromSourceEntriesScanned => {
                &STRING_TABLE_MERGE_FROM_SOURCE_ENTRIES_SCANNED
            }

            FrontendCounter::ModuleRemapStringIdsCalls => &MODULE_REMAP_STRING_IDS_CALLS,
        }
    }

    fn counter_label(counter: FrontendCounter) -> &'static str {
        match counter {
            FrontendCounter::ModuleCount => "module count",

            FrontendCounter::SourceFileCount => "source file count",

            FrontendCounter::SourceByteCount => "source byte count",

            FrontendCounter::PreparedFileCount => "prepared file count",

            FrontendCounter::TokenCount => "token count",

            FrontendCounter::HeaderCount => "header count",

            FrontendCounter::ImportCount => "import count",

            FrontendCounter::TopLevelDeclarationCount => "top-level declaration count",

            FrontendCounter::DependencyHeaderCount => "dependency header count",

            FrontendCounter::DependencyEdgeCount => "dependency edge count",

            FrontendCounter::DependencyVisitCount => "dependency visit count",

            FrontendCounter::AstHeaderCount => "AST/header count",

            FrontendCounter::AstFunctionCount => "AST/function count",

            FrontendCounter::AstStructCount => "AST/struct count",

            FrontendCounter::AstChoiceCount => "AST/choice count",

            FrontendCounter::AstConstantCount => "AST/constant count",

            FrontendCounter::AstTraitDeclarationCount => "AST/trait declaration count",

            FrontendCounter::AstTraitConformanceCount => "AST/trait conformance count",

            FrontendCounter::AstReceiverMethodCount => "AST/receiver method count",

            FrontendCounter::AstGenericTemplateCount => "AST/generic template count",

            FrontendCounter::AstGenericInstanceCount => "AST/generic instance count",

            FrontendCounter::ConstantFoldAttemptCount => "constant fold attempt count",

            FrontendCounter::ConstantFoldSuccessCount => "constant fold success count",

            FrontendCounter::TemplateCount => "template count",

            FrontendCounter::ConstTemplateCount => "const template count",

            FrontendCounter::RuntimeTemplateCount => "runtime template count",

            FrontendCounter::HirBlockCount => "HIR/block count",

            FrontendCounter::HirStatementCount => "HIR/statement count",

            FrontendCounter::HirFunctionCount => "HIR/function count",

            FrontendCounter::BorrowFunctionCount => "borrow/function count",

            FrontendCounter::BorrowBlockCount => "borrow/block count",

            FrontendCounter::BorrowConflictCheckCount => "borrow/conflict check count",

            FrontendCounter::BorrowStateSnapshotCount => "borrow/state snapshot count",

            FrontendCounter::BorrowStatementFactCount => "borrow/statement fact count",

            FrontendCounter::BorrowTerminatorFactCount => "borrow/terminator fact count",

            FrontendCounter::BorrowValueFactCount => "borrow/value fact count",

            FrontendCounter::TypeEnvironmentFieldsForQueries => {
                "TypeEnvironment/fields_for queries"
            }

            FrontendCounter::TypeEnvironmentFieldsReturned => {
                "TypeEnvironment/fields_for borrowed fields returned"
            }

            FrontendCounter::TypeEnvironmentVariantsForQueries => {
                "TypeEnvironment/variants_for queries"
            }

            FrontendCounter::TypeEnvironmentVariantsReturned => {
                "TypeEnvironment/variants_for borrowed variants returned"
            }

            FrontendCounter::TypeEnvironmentSubstituteTypeIdCalls => {
                "TypeEnvironment/substitute_type_id calls"
            }

            FrontendCounter::TypeEnvironmentSubstitutionCacheLookups => {
                "TypeEnvironment/substitution cache lookups"
            }

            FrontendCounter::TypeEnvironmentSubstitutionCacheHits => {
                "TypeEnvironment/substitution cache hits"
            }

            FrontendCounter::TypeEnvironmentSubstitutionCacheMisses => {
                "TypeEnvironment/substitution cache misses"
            }

            FrontendCounter::TypeCompatibilityCacheLookups => "TypeCompatibilityCache/lookups",

            FrontendCounter::TypeCompatibilityCacheHits => "TypeCompatibilityCache/hits",

            FrontendCounter::TypeCompatibilityCacheMisses => "TypeCompatibilityCache/misses",

            FrontendCounter::StringTableFullClones => "StringTable/full clone count",

            FrontendCounter::StringTableMergeFromSourceEntriesScanned => {
                "StringTable/merge_from source entries scanned"
            }

            FrontendCounter::ModuleRemapStringIdsCalls => "Module/remap_string_ids count",
        }
    }

    fn counter_metric_name(counter: FrontendCounter) -> &'static str {
        match counter {
            FrontendCounter::ModuleCount => "module_count",

            FrontendCounter::SourceFileCount => "source_file_count",

            FrontendCounter::SourceByteCount => "source_byte_count",

            FrontendCounter::PreparedFileCount => "prepared_file_count",

            FrontendCounter::TokenCount => "token_count",

            FrontendCounter::HeaderCount => "header_count",

            FrontendCounter::ImportCount => "import_count",

            FrontendCounter::TopLevelDeclarationCount => "top_level_declaration_count",

            FrontendCounter::DependencyHeaderCount => "dependency_header_count",

            FrontendCounter::DependencyEdgeCount => "dependency_edge_count",

            FrontendCounter::DependencyVisitCount => "dependency_visit_count",

            FrontendCounter::AstHeaderCount => "ast_header_count",

            FrontendCounter::AstFunctionCount => "ast_function_count",

            FrontendCounter::AstStructCount => "ast_struct_count",

            FrontendCounter::AstChoiceCount => "ast_choice_count",

            FrontendCounter::AstConstantCount => "ast_constant_count",

            FrontendCounter::AstTraitDeclarationCount => "ast_trait_declaration_count",

            FrontendCounter::AstTraitConformanceCount => "ast_trait_conformance_count",

            FrontendCounter::AstReceiverMethodCount => "ast_receiver_method_count",

            FrontendCounter::AstGenericTemplateCount => "ast_generic_template_count",

            FrontendCounter::AstGenericInstanceCount => "ast_generic_instance_count",

            FrontendCounter::ConstantFoldAttemptCount => "constant_fold_attempt_count",

            FrontendCounter::ConstantFoldSuccessCount => "constant_fold_success_count",

            FrontendCounter::TemplateCount => "template_count",

            FrontendCounter::ConstTemplateCount => "const_template_count",

            FrontendCounter::RuntimeTemplateCount => "runtime_template_count",

            FrontendCounter::HirBlockCount => "hir_block_count",

            FrontendCounter::HirStatementCount => "hir_statement_count",

            FrontendCounter::HirFunctionCount => "hir_function_count",

            FrontendCounter::BorrowFunctionCount => "borrow_function_count",

            FrontendCounter::BorrowBlockCount => "borrow_block_count",

            FrontendCounter::BorrowConflictCheckCount => "borrow_conflict_check_count",

            FrontendCounter::BorrowStateSnapshotCount => "borrow_state_snapshot_count",

            FrontendCounter::BorrowStatementFactCount => "borrow_statement_fact_count",

            FrontendCounter::BorrowTerminatorFactCount => "borrow_terminator_fact_count",

            FrontendCounter::BorrowValueFactCount => "borrow_value_fact_count",

            FrontendCounter::TypeEnvironmentFieldsForQueries => {
                "type_environment_fields_for_queries"
            }

            FrontendCounter::TypeEnvironmentFieldsReturned => "type_environment_fields_returned",

            FrontendCounter::TypeEnvironmentVariantsForQueries => {
                "type_environment_variants_for_queries"
            }

            FrontendCounter::TypeEnvironmentVariantsReturned => {
                "type_environment_variants_returned"
            }

            FrontendCounter::TypeEnvironmentSubstituteTypeIdCalls => {
                "type_environment_substitute_type_id_calls"
            }

            FrontendCounter::TypeEnvironmentSubstitutionCacheLookups => {
                "type_environment_substitution_cache_lookups"
            }

            FrontendCounter::TypeEnvironmentSubstitutionCacheHits => {
                "type_environment_substitution_cache_hits"
            }

            FrontendCounter::TypeEnvironmentSubstitutionCacheMisses => {
                "type_environment_substitution_cache_misses"
            }

            FrontendCounter::TypeCompatibilityCacheLookups => "type_compatibility_cache_lookups",

            FrontendCounter::TypeCompatibilityCacheHits => "type_compatibility_cache_hits",

            FrontendCounter::TypeCompatibilityCacheMisses => "type_compatibility_cache_misses",

            FrontendCounter::StringTableFullClones => "string_table_full_clones",

            FrontendCounter::StringTableMergeFromSourceEntriesScanned => {
                "string_table_merge_source_entries_scanned"
            }

            FrontendCounter::ModuleRemapStringIdsCalls => "module_remap_string_ids_calls",
        }
    }

    fn counter_value(counter: FrontendCounter) -> usize {
        atomic_counter(counter).load(Ordering::Relaxed)
    }
}

#[cfg(feature = "detailed_timers")]
pub(crate) use detailed::{
    add_frontend_counter, increment_frontend_counter, log_frontend_counters,
    reset_frontend_counters,
};

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn reset_frontend_counters() {}

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn increment_frontend_counter(_counter: FrontendCounter) {}

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn add_frontend_counter(_counter: FrontendCounter, _amount: usize) {}

#[cfg(not(feature = "detailed_timers"))]
pub(crate) fn log_frontend_counters() {}
