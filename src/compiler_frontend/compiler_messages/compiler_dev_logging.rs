//! Developer-oriented logging and benchmark observation helpers.
//!
//! WHAT: provides feature-gated macros (`token_log!`, `timer_log!`, `hir_log!`) and benchmark
//!       snapshot types for debugging compiler internals without affecting release builds.
//! WHY: keeping developer instrumentation behind feature flags keeps normal builds deterministic,
//!      quiet, and free of debug output overhead.

#[cfg(feature = "detailed_timers")]
use std::time::Duration;

#[cfg(feature = "detailed_timers")]
#[derive(Debug, Clone, Default)]
pub(crate) struct BenchmarkObservationSnapshot {
    pub(crate) timings: Vec<BenchmarkObservationMetric>,
    pub(crate) counters: Vec<BenchmarkObservationMetric>,
}

#[cfg(feature = "detailed_timers")]
#[derive(Debug, Clone)]
pub(crate) struct BenchmarkObservationMetric {
    pub(crate) name: String,
    pub(crate) value: f64,
}

// TOKEN LOGGING MACROS
#[macro_export]
#[cfg(feature = "show_tokens")]
macro_rules! token_log {
    ($($arg:tt)*) => {
        saying::say!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "show_tokens"))]
macro_rules! token_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// Extra timer logging
#[macro_export]
#[cfg(feature = "detailed_timers")]
macro_rules! timer_log {
    ($time:expr, $msg:expr) => {{
        if $crate::compiler_frontend::compiler_messages::compiler_dev_logging::detailed_timer_output_enabled() {
            saying::say!($msg, Green #$time.elapsed());
        }
    }};
}

#[macro_export]
#[cfg(not(feature = "detailed_timers"))]
macro_rules! timer_log {
    ($time:expr, $msg:expr) => {
        // Nothing
    };
}

/// Benchmark-aware timer macro: prints the existing human message, then emits a stable
/// machine-readable `BST_BENCH timing` line for benchmark observation parsing.
///
/// WHAT: wraps one stage/timer so it produces both developer-readable colored output and
/// a grep-friendly metric that survives prose refactors.
/// WHY: only benchmark-significant stages should use this; tiny substage timers should
/// keep using `timer_log!` so local history is not flooded with unstable micro-events.
#[macro_export]
#[cfg(feature = "detailed_timers")]
macro_rules! benchmark_timer_log {
    ($time:expr, $metric_name:expr, $human_msg:expr) => {{
        let elapsed = $time.elapsed();
        if $crate::compiler_frontend::compiler_messages::compiler_dev_logging::detailed_timer_output_enabled() {
            saying::say!($human_msg, Green #elapsed);
        }
        $crate::compiler_frontend::compiler_messages::compiler_dev_logging::log_benchmark_timing(
            $metric_name,
            elapsed,
        );
    }};
}

#[macro_export]
#[cfg(not(feature = "detailed_timers"))]
macro_rules! benchmark_timer_log {
    ($time:expr, $metric_name:expr, $human_msg:expr) => {
        // Nothing
    };
}

#[cfg(feature = "detailed_timers")]
pub fn log_aggregated_duration(label: &str, duration: Duration) {
    if detailed_timer_output_enabled() {
        saying::say!(label, Green #duration);
    }
}

/// Emit one stable, machine-readable benchmark observation line.
///
/// WHAT: prints a plain `BST_BENCH timing <metric>=<millis>ms` line that the benchmark
/// observation parser can grep without depending on human prose.
/// WHY: separating the stable metric line from colored human output lets compiler
/// logging change its prose without silently breaking performance attribution.
#[cfg(feature = "detailed_timers")]
pub fn log_benchmark_timing(metric_name: &str, duration: Duration) {
    if metric_name.trim().is_empty() {
        return;
    }

    let millis = duration.as_secs_f64() * 1000.0;
    if detailed_timer_output_enabled() {
        saying::say!("BST_BENCH timing ", metric_name, "=", #millis, "ms");
    }
    benchmark_collector::record_timing(metric_name, millis);
}

/// Emit one stable, machine-readable benchmark counter line.
///
/// WHAT: prints and records `BST_BENCH counter <metric>=<value>` observations
/// using the same collection scope as stage timings.
/// WHY: counters need a stable machine path for local benchmark history while
/// human counter prose remains optional display text.
#[cfg(feature = "detailed_timers")]
pub fn log_benchmark_counter(metric_name: &str, value: f64) {
    if metric_name.trim().is_empty() || !value.is_finite() {
        return;
    }

    if detailed_timer_output_enabled() {
        saying::say!("BST_BENCH counter ", metric_name, "=", #value);
    }
    benchmark_collector::record_counter(metric_name, value);
}

#[cfg(feature = "detailed_timers")]
pub fn detailed_timer_output_enabled() -> bool {
    benchmark_collector::output_enabled()
}

// -------------------------
//  In-Memory Benchmark Collector
// -------------------------

/// Thread-safe in-memory collector for benchmark observations.
///
/// WHAT: captures stable benchmark metric values during an active collection scope
/// so that in-process benchmark APIs can read timings and counters directly
/// instead of parsing stdout.
/// WHY: subprocess-free frontend benchmarks need programmatic access to the same
/// metrics that CLI benchmarks extract from stable `BST_BENCH` lines.
#[cfg(feature = "detailed_timers")]
mod benchmark_collector {
    use super::{BenchmarkObservationMetric, BenchmarkObservationSnapshot};
    use std::sync::Mutex;

    struct ActiveBenchmarkCollection {
        timings: Vec<BenchmarkObservationMetric>,
        counters: Vec<BenchmarkObservationMetric>,
        suppress_output: bool,
    }

    static ACTIVE_COLLECTOR: Mutex<Option<ActiveBenchmarkCollection>> = Mutex::new(None);

    /// Start a new collection scope, discarding any previous in-flight data.
    pub fn start_collection(suppress_output: bool) {
        if let Ok(mut guard) = ACTIVE_COLLECTOR.lock() {
            *guard = Some(ActiveBenchmarkCollection {
                timings: Vec::new(),
                counters: Vec::new(),
                suppress_output,
            });
        }
    }

    /// Record one timing if a collection scope is currently active.
    pub fn record_timing(name: &str, millis: f64) {
        if let Ok(mut guard) = ACTIVE_COLLECTOR.lock()
            && let Some(collection) = guard.as_mut()
        {
            collection.timings.push(BenchmarkObservationMetric {
                name: name.to_string(),
                value: millis,
            });
        }
    }

    /// Record one counter if a collection scope is currently active.
    pub fn record_counter(name: &str, value: f64) {
        if let Ok(mut guard) = ACTIVE_COLLECTOR.lock()
            && let Some(collection) = guard.as_mut()
        {
            collection.counters.push(BenchmarkObservationMetric {
                name: name.to_string(),
                value,
            });
        }
    }

    /// Whether detailed timer text should currently be printed.
    pub fn output_enabled() -> bool {
        match ACTIVE_COLLECTOR.lock() {
            Ok(guard) => match guard.as_ref() {
                Some(collection) => !collection.suppress_output,
                None => true,
            },
            Err(_) => true,
        }
    }

    /// Stop the current collection scope and return all captured observations.
    ///
    /// Returns an empty snapshot if no scope was active or if the lock was poisoned.
    pub fn stop_and_collect() -> BenchmarkObservationSnapshot {
        if let Ok(mut guard) = ACTIVE_COLLECTOR.lock() {
            guard
                .take()
                .map_or_else(BenchmarkObservationSnapshot::default, |collection| {
                    BenchmarkObservationSnapshot {
                        timings: collection.timings,
                        counters: collection.counters,
                    }
                })
        } else {
            BenchmarkObservationSnapshot::default()
        }
    }
}

#[cfg(feature = "detailed_timers")]
pub use benchmark_collector::{
    start_collection as start_benchmark_collection,
    stop_and_collect as stop_and_collect_benchmark_observations,
};

// Headers Logging
#[macro_export]
#[cfg(feature = "show_headers")]
macro_rules! header_log {
    ($($arg:tt)*) => {
        saying::say!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "show_headers"))]
macro_rules! header_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// AST LOGGING MACROS
#[macro_export]
#[cfg(feature = "show_ast")]
macro_rules! ast_log {
    ($($arg:tt)*) => {
        saying::say!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "show_ast"))]
macro_rules! ast_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// EVAL LOGGING MACROS
#[macro_export]
#[cfg(feature = "show_eval")]
macro_rules! eval_log {
    ($($arg:tt)*) => {
        saying::say!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "show_eval"))]
macro_rules! eval_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// CODEGEN LOGGING MACROS
#[macro_export]
#[cfg(feature = "show_codegen")]
macro_rules! codegen_log {
    ($($arg:tt)*) => {
        saying::say!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "show_codegen"))]
macro_rules! codegen_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// HIR LOGGING MACROS
#[macro_export]
#[cfg(feature = "show_hir")]
macro_rules! hir_log {
    ($($arg:tt)*) => {
        saying::say!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "show_hir"))]
macro_rules! hir_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// BORROW CHECKER LOGGING MACROS
#[macro_export]
#[cfg(feature = "show_borrow_checker")]
macro_rules! borrow_log {
    ($($arg:tt)*) => {
        saying::say!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "show_borrow_checker"))]
macro_rules! borrow_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}
