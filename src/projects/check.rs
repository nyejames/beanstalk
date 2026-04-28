//! Frontend-only `check` command orchestration.
//!
//! WHAT: compiles input through Stage 0 + frontend validation (including borrow checking)
//! without running backend lowering or writing output artifacts.
//! WHY: users and tooling need a fast diagnostic pass that validates source correctness while
//! remaining backend-agnostic.

use crate::build_system::build::{
    BuildBootstrap, ProjectBuilder, bootstrap_project_build, collect_frontend_warnings,
};
use crate::build_system::create_project_modules::compile_project_frontend;
use crate::compiler_frontend::basic_utility_functions::check_if_valid_path;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::display_messages::{
    print_compiler_messages, print_terse_compiler_messages,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;
use saying::say;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CheckOptions {
    pub terse: bool,
}

struct CheckOutcome {
    messages: CompilerMessages,
    duration: Duration,
}

pub fn run_check(path: &str, options: CheckOptions) {
    let outcome = execute_check(path);
    let error_count = outcome.messages.errors.len();
    let warning_count = outcome.messages.warnings.len();

    if options.terse {
        print_terse_compiler_messages(&outcome.messages);
        println!(
            "{}",
            format_terse_summary_line(outcome.duration, error_count, warning_count)
        );
        return;
    }

    if error_count == 0 && warning_count == 0 {
        say!(Dark White "---------------------");
        say!(success_message(outcome.duration));
        say!(Bold Green "No errors or warnings");
    } else {
        print_compiler_messages(outcome.messages);
    }
}

fn execute_check(path: &str) -> CheckOutcome {
    let start = Instant::now();
    let normalized_path = normalize_entry_path(path);

    let mut path_string_table = StringTable::new();
    let valid_path = match check_if_valid_path(normalized_path, &mut path_string_table) {
        Ok(path) => path,
        Err(error) => {
            return CheckOutcome {
                messages: CompilerMessages::from_error(error, path_string_table),
                duration: start.elapsed(),
            };
        }
    };

    let project_builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let BuildBootstrap {
        mut config,
        style_directives,
        mut string_table,
    } = match bootstrap_project_build(&project_builder, valid_path) {
        Ok(bootstrap) => bootstrap,
        Err(messages) => {
            return CheckOutcome {
                messages,
                duration: start.elapsed(),
            };
        }
    };

    let libraries = project_builder.backend.libraries();
    let messages = match compile_project_frontend(
        &mut config,
        &[],
        &style_directives,
        &libraries,
        &mut string_table,
    ) {
        Ok(modules) => CompilerMessages {
            errors: Vec::new(),
            warnings: collect_frontend_warnings(&modules),
            string_table,
        },
        Err(messages) => messages,
    };

    CheckOutcome {
        messages,
        duration: start.elapsed(),
    }
}

fn normalize_entry_path(path: &str) -> &str {
    if path.trim().is_empty() { "." } else { path }
}

fn format_terse_summary_line(
    duration: Duration,
    error_count: usize,
    warning_count: usize,
) -> String {
    if error_count == 0 && warning_count == 0 {
        return format!("{}. No errors or warnings.", success_message(duration));
    }

    format!("errors={error_count}, warnings={warning_count}.")
}

fn format_duration(duration: Duration) -> String {
    format!("{duration:?}")
}

fn success_message(duration: Duration) -> String {
    format!("Done in {}", format_duration(duration))
}

#[cfg(test)]
#[path = "tests/check_tests.rs"]
mod tests;
