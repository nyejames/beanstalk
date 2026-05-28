//! Local-only frontend performance instrumentation.
//!
//! WHAT: exposes counters for clone-heavy, cache-sensitive, and remap-heavy frontend paths.
//! WHY: detailed benchmark runs need enough local evidence to interpret small
//! end-to-end timing changes, while normal compiler builds must not pay for or
//! print this diagnostic data.

#[derive(Clone, Copy)]
pub(crate) enum FrontendCounter {
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
    use crate::compiler_frontend::compiler_messages::compiler_dev_logging::detailed_timer_output_enabled;
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
        if !detailed_timer_output_enabled() {
            return;
        }

        saying::say!("Frontend/performance counters:");

        for counter in all_counters() {
            saying::say!(
                "  ",
                counter_label(counter),
                " = ",
                Dark Green counter_value(counter)
            );
        }
    }

    fn all_counters() -> [FrontendCounter; 14] {
        [
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
