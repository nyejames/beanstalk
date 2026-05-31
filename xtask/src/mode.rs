//! CLI mode parser for xtask
//!
//! WHAT: Parses the command-line mode string into a typed benchmark mode.
//! WHY: Keeps mode parsing testable and separate from main dispatch logic,
//!      and replaces raw string matching with a descriptive enum.

/// Distinguishes the supported xtask benchmark modes.
///
/// WHAT: Each variant represents a valid CLI mode the user can pass to xtask.
/// WHY: Using an enum prevents silent typos in dispatch code and makes the
///      set of supported modes explicit to readers and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkMode {
    /// Run the full benchmark suite and update local/public summaries.
    Bench,
    /// Run the full benchmark suite without writing benchmark history.
    BenchCheck,
    /// Read local benchmark history and print a drilldown report.
    BenchReport,
    /// Run the focused frontend benchmark suite and record.
    BenchFrontend,
    /// Run the focused frontend benchmark suite without writing history.
    BenchFrontendCheck,
}

impl BenchmarkMode {
    /// Parse a mode string into a typed mode.
    ///
    /// Returns `None` for unknown or unsupported mode strings.
    pub fn parse(mode_str: &str) -> Option<Self> {
        match mode_str {
            "bench" => Some(BenchmarkMode::Bench),
            "bench-check" => Some(BenchmarkMode::BenchCheck),
            "bench-report" => Some(BenchmarkMode::BenchReport),
            "bench-frontend" => Some(BenchmarkMode::BenchFrontend),
            "bench-frontend-check" => Some(BenchmarkMode::BenchFrontendCheck),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
