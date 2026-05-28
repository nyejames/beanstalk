//! Frontend benchmark implementation.
//!
//! WHAT: measures the compiler frontend pipeline (Stage 0 through borrow
//! validation) for a single entry path, collecting total time and per-stage
//! timings when `detailed_timers` is enabled.
//! WHY: avoids subprocess noise while reusing the exact same setup path as
//! `bean check`.

use std::path::PathBuf;
use std::time::Instant;

use crate::build_system::build::{BuildBootstrap, ProjectBuilder, bootstrap_project_build};
use crate::build_system::create_project_modules::compile_project_frontend;
use crate::build_system::path_validation::check_if_valid_path;
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::display_messages::format_terse_compiler_messages;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;

/// Build profile selector for frontend benchmarks.
///
/// WHAT: a narrow, public copy of the internal `FrontendBuildProfile` so the
/// benchmark API does not expose private compiler types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendBenchmarkBuildProfile {
    Dev,
    Release,
}

/// Input options for a single frontend benchmark run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendBenchmarkOptions {
    pub entry_path: PathBuf,
    pub build_profile: FrontendBenchmarkBuildProfile,
}

/// Report produced by a successful frontend benchmark run.
#[derive(Debug, Clone)]
pub struct FrontendBenchmarkReport {
    pub total_ms: f64,
    pub stages: Vec<FrontendBenchmarkStage>,
    pub counters: Vec<FrontendBenchmarkCounter>,
}

/// One named stage timing captured during frontend compilation.
#[derive(Debug, Clone)]
pub struct FrontendBenchmarkStage {
    pub name: String,
    pub duration_ms: f64,
}

/// One named counter value captured during frontend compilation.
#[derive(Debug, Clone)]
pub struct FrontendBenchmarkCounter {
    pub name: String,
    pub value: f64,
}

/// Error returned when a frontend benchmark fails.
///
/// The message is pre-rendered into a terse, multi-line string suitable for
/// direct display by xtask or other tooling.
#[derive(Debug, Clone)]
pub struct FrontendBenchmarkError {
    pub message: String,
}

impl std::fmt::Display for FrontendBenchmarkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for FrontendBenchmarkError {}

/// Run one frontend benchmark for the given entry path.
///
/// WHAT: validates the path, bootstraps an HTML project build, compiles through
/// the frontend pipeline, and returns total plus per-stage timings.
/// WHY: this is the narrow dev-tooling entry point that keeps benchmark
/// orchestration out of the compiler frontend while reusing production setup.
///
/// Stage timings are only populated when the `detailed_timers` feature is
/// enabled and a collection scope is active during compilation.
pub fn run_frontend_benchmark(
    options: FrontendBenchmarkOptions,
) -> Result<FrontendBenchmarkReport, FrontendBenchmarkError> {
    let start = Instant::now();

    let path = options
        .entry_path
        .to_str()
        .ok_or_else(|| FrontendBenchmarkError {
            message: format!(
                "Frontend benchmark path is not valid UTF-8: {}",
                options.entry_path.display()
            ),
        })?;
    let normalized = if path.trim().is_empty() { "." } else { path };

    let mut path_string_table = StringTable::new();
    let valid_path = match check_if_valid_path(normalized, &mut path_string_table) {
        Ok(path) => path,
        Err(error) => {
            let messages = CompilerMessages::from_error(error, path_string_table);

            return Err(FrontendBenchmarkError {
                message: format_compiler_messages(&messages),
            });
        }
    };

    let project_builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let BuildBootstrap {
        mut config,
        style_directives,
        mut string_table,
        mut libraries,
    } = match bootstrap_project_build(&project_builder, valid_path) {
        Ok(bootstrap) => bootstrap,
        Err(messages) => {
            return Err(FrontendBenchmarkError {
                message: format_compiler_messages(&messages),
            });
        }
    };

    let flags = match options.build_profile {
        FrontendBenchmarkBuildProfile::Release => vec![Flag::Release],
        FrontendBenchmarkBuildProfile::Dev => vec![],
    };

    #[cfg(feature = "detailed_timers")]
    crate::compiler_frontend::compiler_messages::compiler_dev_logging::start_benchmark_collection(
        true,
    );

    let messages = match compile_project_frontend(
        &mut config,
        &flags,
        &style_directives,
        &mut libraries,
        &mut string_table,
    ) {
        Ok(_modules) => CompilerMessages::empty(string_table),
        Err(messages) => messages,
    };

    #[cfg(feature = "detailed_timers")]
    let raw_timings = crate::compiler_frontend::compiler_messages::compiler_dev_logging::stop_and_collect_benchmark_timings();

    #[cfg(not(feature = "detailed_timers"))]
    let raw_timings: Vec<(String, f64)> = Vec::new();

    let total_ms = start.elapsed().as_secs_f64() * 1000.0;

    if messages.error_count() > 0 {
        return Err(FrontendBenchmarkError {
            message: format_compiler_messages(&messages),
        });
    }

    let stages = raw_timings
        .into_iter()
        .map(|(name, duration_ms)| FrontendBenchmarkStage { name, duration_ms })
        .collect();

    let counters = Vec::new();

    Ok(FrontendBenchmarkReport {
        total_ms,
        stages,
        counters,
    })
}

fn format_compiler_messages(messages: &CompilerMessages) -> String {
    let mut lines = format_terse_compiler_messages(messages);

    if lines.is_empty() {
        lines.push(format!("{} error(s) found", messages.error_count()));
    }

    lines.join("\n")
}
