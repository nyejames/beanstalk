//! Test runner for validating core Beanstalk compiler_frontend functionality
use crate::build_system::build::build_project;
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_messages::compiler_errors::error_type_to_str;
use crate::compiler_frontend::compiler_messages::compiler_warnings::print_formatted_warning;
use crate::compiler_frontend::display_messages::print_formatted_error;
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;
use saying::say;
use std::fs;
use std::path::Path;

const INTEGRATION_TESTS_PATH: &str = "tests/cases";
const SEPARATOR_LINE_LENGTH: usize = 37;

/// This module provides a focused test suite that validates the essential
/// compiler_frontend operations without getting bogged down in implementation details.
///
/// Run all test cases from the tests/cases directory
pub fn run_all_test_cases(show_warnings: bool) {
    println!("Running all Beanstalk test cases...\n");
    let timer = std::time::Instant::now();

    let test_cases_dir = Path::new(INTEGRATION_TESTS_PATH);
    let success_dir = test_cases_dir.join("success");
    let failure_dir = test_cases_dir.join("failure");

    // Flags set for all the integration tests
    let flags = vec![Flag::DisableTimers, Flag::DisableWarnings];

    let mut total_tests = 0;
    let mut passed_tests = 0;
    let mut failed_tests = 0;
    let mut expected_failures = 0;
    let mut unexpected_successes = 0;

    // Test files that should succeed
    if success_dir.exists() {
        say!(Cyan "Testing files that should succeed:");
        println!("{}", "-".repeat(SEPARATOR_LINE_LENGTH));
        if let Ok(entries) = fs::read_dir(&success_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "bst") {
                    total_tests += 1;
                    let file_name = path.file_name().unwrap().to_string_lossy();

                    // println!("\n------------------------------------------");
                    println!("  {}", file_name);

                    let html_project_builder = HtmlProjectBuilder::new();
                    let path_string = path.to_string_lossy().to_string();

                    match build_project(&html_project_builder, &path_string, &flags) {
                        Ok(build_result) => {
                            say!(Green "✓ PASS");
                            if !build_result.warnings.is_empty() {
                                say!(
                                    Yellow "With ",
                                    build_result.warnings.len().to_string(),
                                    " warnings"
                                );
                                if show_warnings {
                                    for warning in build_result.warnings {
                                        print_formatted_warning(warning);
                                    }
                                }
                            }
                            passed_tests += 1;
                        }
                        Err(messages) => {
                            say!(Red "✗ FAIL");
                            failed_tests += 1;
                            for error in messages.errors {
                                print_formatted_error(error);
                            }
                        }
                    }
                }

                println!("{}", "-".repeat(SEPARATOR_LINE_LENGTH));
            }
        }
    }

    println!();

    // Test files that should fail
    if failure_dir.exists() {
        say!(Cyan "Testing files that should fail:");
        println!("{}", "-".repeat(SEPARATOR_LINE_LENGTH));
        if let Ok(entries) = fs::read_dir(&failure_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "bst") {
                    total_tests += 1;
                    let file_name = path.file_name().unwrap().to_string_lossy();

                    // println!("\n------------------------------------------");
                    println!("  {}", file_name);
                    let html_project_builder = HtmlProjectBuilder::new();
                    let path_string = path.to_string_lossy().to_string();

                    match build_project(&html_project_builder, &path_string, &flags) {
                        Ok(build_result) => {
                            say!(Yellow "✗ UNEXPECTED SUCCESS");
                            unexpected_successes += 1;
                            if !build_result.warnings.is_empty() {
                                say!(
                                    Yellow "With ",
                                    build_result.warnings.len().to_string(),
                                    " warnings"
                                );
                                if show_warnings {
                                    for warning in build_result.warnings {
                                        print_formatted_warning(warning);
                                    }
                                }
                            }
                        }
                        Err(messages) => {
                            say!(Green "✓ EXPECTED FAILURE");
                            expected_failures += 1;
                            for error in messages.errors {
                                say!(Yellow error_type_to_str(&error.error_type));
                                // print_formatted_error(error);
                            }
                            if !messages.warnings.is_empty() {
                                say!("With ", messages.warnings.len().to_string(), " warnings");
                                if show_warnings {
                                    for warning in messages.warnings {
                                        print_formatted_warning(warning);
                                    }
                                }
                            }
                        }
                    }
                }
                println!("{}", "-".repeat(SEPARATOR_LINE_LENGTH));
            }
        }
    }

    println!();

    // Print summary
    println!("\n{}", "=".repeat(SEPARATOR_LINE_LENGTH));
    print!("Test Results Summary. Took: ");
    say!(Green #timer.elapsed());
    say!("  Total tests: ", Yellow total_tests);
    say!("  Successful compilations: ", Blue passed_tests);
    say!("  Failed compilations: ", Blue failed_tests);
    say!("  Expected failures: ", Blue expected_failures);
    say!("  Unexpected successes: ", Blue unexpected_successes);

    let correct_results = passed_tests + expected_failures;
    let incorrect_results = failed_tests + unexpected_successes;

    println!("\n  Correct results: {} / {}", correct_results, total_tests);
    println!(
        "  Incorrect results: {} / {}",
        incorrect_results, total_tests
    );

    if incorrect_results == 0 {
        say!("\n🎉 All tests behaved as expected!");
    } else {
        let percentage = (correct_results as f64 / total_tests as f64) * 100.0;
        say!(Yellow "\n⚠ ", Bright Yellow format!("{:.1}", percentage), " %", Reset " of tests behaved as expected");
    }

    println!("{}", "=".repeat(SEPARATOR_LINE_LENGTH));
}
