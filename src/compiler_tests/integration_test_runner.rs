//! Test runner for validating core Beanstalk compiler functionality
use crate::compiler::compiler_messages::compiler_errors::{
    error_type_to_str,
};
use crate::compiler::compiler_messages::compiler_warnings::print_formatted_warning;
use saying::say;
use crate::compiler::display_messages::print_formatted_error;
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;

const INTEGRATION_TESTS_PATH: &str = "tests/cases";

/// This module provides a focused test suite that validates the essential
/// compiler operations without getting bogged down in implementation details.
///
/// Run all test cases from the tests/cases directory
pub fn run_all_test_cases(show_warnings: bool) {
    use crate::Flag;
    use crate::build_system::build::build_project_files;
    use std::fs;
    use std::path::Path;

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
        println!("------------------------------------------");
        if let Ok(entries) = fs::read_dir(&success_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "bst") {
                    total_tests += 1;
                    let file_name = path.file_name().unwrap().to_string_lossy();

                    // println!("\n------------------------------------------");
                    println!("  {}", file_name);

                    let html_project_builder = Box::new(HtmlProjectBuilder::new());

                    let messages = build_project_files(
                        html_project_builder,
                        INTEGRATION_TESTS_PATH,
                        &flags,
                    );

                    if messages.errors.is_empty() {
                        say!(Green "✓ PASS");
                        if !messages.warnings.is_empty() {
                            say!(Yellow "With ", messages.warnings.len().to_string(), " warnings");
                            if show_warnings {
                                for warning in messages.warnings {
                                    print_formatted_warning(warning);
                                }
                            }
                        }
                        passed_tests += 1;
                    } else {
                        say!(Red "✗ FAIL");
                        failed_tests += 1;
                        for error in messages.errors {
                            print_formatted_error(error);
                        }
                    }
                }

                println!("------------------------------------------");
            }
        }
    }

    println!();

    // Test files that should fail
    if failure_dir.exists() {
        say!(Cyan "Testing files that should fail:");
        println!("------------------------------------------");
        if let Ok(entries) = fs::read_dir(&failure_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "bst") {
                    total_tests += 1;
                    let file_name = path.file_name().unwrap().to_string_lossy();

                    // println!("\n------------------------------------------");
                    println!("  {}", file_name);
                    let html_project_builder = Box::new(HtmlProjectBuilder::new());

                    let messages = build_project_files(
                        html_project_builder,
                        INTEGRATION_TESTS_PATH,
                        &flags,
                    );

                    if messages.errors.is_empty() {
                        say!(Yellow "✗ UNEXPECTED SUCCESS");
                        unexpected_successes += 1;
                        if !messages.warnings.is_empty() {
                            say!(Yellow "With ", messages.warnings.len().to_string(), " warnings");
                            if show_warnings {
                                for warning in messages.warnings {
                                    print_formatted_warning(warning);
                                }
                            }
                        }
                    } else {
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
                println!("------------------------------------------");
            }
        }
    }

    println!();

    // Print summary
    println!("\n{}", "=".repeat(50));
    print!("Test Results Summary. Took: ");
    say!(#timer.elapsed());
    println!("  Total tests: {}", total_tests);
    println!("  Successful compilations: {}", passed_tests);
    println!("  Failed compilations: {}", failed_tests);
    println!("  Expected failures: {}", expected_failures);
    println!("  Unexpected successes: {}", unexpected_successes);

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
        say!("\n⚠", percentage, "% of tests behaved as expected");
    }

    println!("{}", "=".repeat(50));
}
