#[cfg(feature = "detailed_timers")]
use std::time::Duration;

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
    let millis = duration.as_secs_f64() * 1000.0;
    if detailed_timer_output_enabled() {
        saying::say!("BST_BENCH timing ", metric_name, "=", #millis, "ms");
    }
    benchmark_collector::record_timing(metric_name, millis);
}

#[cfg(feature = "detailed_timers")]
pub fn detailed_timer_output_enabled() -> bool {
    benchmark_collector::output_enabled()
}

// -------------------------
//  In-Memory Benchmark Collector
// -------------------------

/// Thread-safe in-memory collector for benchmark timings.
///
/// WHAT: captures stable benchmark metric values during an active collection scope
/// so that in-process benchmark APIs can read stage timings directly instead of
/// parsing stdout.
/// WHY: subprocess-free frontend benchmarks need programmatic access to the same
/// metrics that CLI benchmarks extract from `BST_BENCH timing` lines.
#[cfg(feature = "detailed_timers")]
mod benchmark_collector {
    use std::sync::Mutex;

    struct ActiveBenchmarkCollection {
        timings: Vec<(String, f64)>,
        suppress_output: bool,
    }

    static ACTIVE_COLLECTOR: Mutex<Option<ActiveBenchmarkCollection>> = Mutex::new(None);

    /// Start a new collection scope, discarding any previous in-flight data.
    pub fn start_collection(suppress_output: bool) {
        if let Ok(mut guard) = ACTIVE_COLLECTOR.lock() {
            *guard = Some(ActiveBenchmarkCollection {
                timings: Vec::new(),
                suppress_output,
            });
        }
    }

    /// Record one timing if a collection scope is currently active.
    pub fn record_timing(name: &str, millis: f64) {
        if let Ok(mut guard) = ACTIVE_COLLECTOR.lock()
            && let Some(collection) = guard.as_mut()
        {
            collection.timings.push((name.to_string(), millis));
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

    /// Stop the current collection scope and return all captured timings.
    ///
    /// Returns an empty vector if no scope was active or if the lock was poisoned.
    pub fn stop_and_collect() -> Vec<(String, f64)> {
        if let Ok(mut guard) = ACTIVE_COLLECTOR.lock() {
            guard
                .take()
                .map(|collection| collection.timings)
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }
}

#[cfg(feature = "detailed_timers")]
pub use benchmark_collector::{
    start_collection as start_benchmark_collection,
    stop_and_collect as stop_and_collect_benchmark_timings,
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
