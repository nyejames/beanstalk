//! Tests for CLI command parsing and validation.

use super::{Command, get_command, integration_tests_exit_code};
use crate::compiler_tests::integration_test_runner::IntegrationRunSummary;
use crate::projects::dev_server::DevServerOptions;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn dev_command_uses_default_options() {
    let command = get_command(&args(&["dev", "main.bst"])).expect("command should parse");
    assert_eq!(
        command,
        Command::Dev {
            path: String::from("main.bst"),
            options: DevServerOptions::default(),
        }
    );
}

#[test]
fn build_command_uses_current_directory_when_path_is_missing() {
    let command = get_command(&args(&["build"])).expect("build command should parse");
    assert_eq!(command, Command::Build(String::new()));
}

#[test]
fn build_command_supports_mixed_path_and_flag_ordering() {
    let command =
        get_command(&args(&["build", "--release", "main.bst"])).expect("command should parse");
    assert_eq!(command, Command::Build(String::from("main.bst")));
}

#[test]
fn build_command_rejects_unknown_flags() {
    let error =
        get_command(&args(&["build", "--wat"])).expect_err("unknown build flag should fail");
    assert!(error.contains("Unknown build flag"));
}

#[test]
fn new_html_command_uses_current_directory_when_path_is_missing() {
    let command = get_command(&args(&["new", "html"])).expect("new html command should parse");
    assert_eq!(command, Command::NewHTMLProject(String::new()));
}

#[test]
fn new_html_command_parses_project_path() {
    let command = get_command(&args(&["new", "html", "site"])).expect("new html should parse");
    assert_eq!(command, Command::NewHTMLProject(String::from("site")));
}

#[test]
fn dev_command_parses_custom_host_port_and_poll_interval() {
    let command = get_command(&args(&[
        "dev",
        "main.bst",
        "--host",
        "0.0.0.0",
        "--port",
        "7777",
        "--poll-interval-ms",
        "120",
    ]))
    .expect("command should parse");

    assert_eq!(
        command,
        Command::Dev {
            path: String::from("main.bst"),
            options: DevServerOptions {
                host: String::from("0.0.0.0"),
                port: 7777,
                poll_interval_ms: 120,
            },
        }
    );
}

#[test]
fn dev_command_rejects_invalid_port_values() {
    let error = get_command(&args(&["dev", "main.bst", "--port", "invalid"]))
        .expect_err("invalid port should fail");
    assert!(error.contains("Invalid --port value"));
}

#[test]
fn dev_command_rejects_unknown_flags() {
    let error =
        get_command(&args(&["dev", "main.bst", "--wat"])).expect_err("unknown flag should fail");
    assert!(error.contains("Unknown dev flag"));
}

#[test]
fn dev_command_rejects_missing_flag_values() {
    let host_error =
        get_command(&args(&["dev", "main.bst", "--host"])).expect_err("missing host value");
    assert!(host_error.contains("Missing value for --host"));

    let port_error =
        get_command(&args(&["dev", "main.bst", "--port"])).expect_err("missing port value");
    assert!(port_error.contains("Missing value for --port"));
}

#[test]
fn dev_command_rejects_zero_poll_interval() {
    let error = get_command(&args(&["dev", "main.bst", "--poll-interval-ms", "0"]))
        .expect_err("zero interval should fail");
    assert!(error.contains("greater than zero"));
}

#[test]
fn dev_command_supports_path_and_flag_ordering() {
    let command = get_command(&args(&[
        "dev",
        "--host",
        "localhost",
        "main.bst",
        "--poll-interval-ms",
        "900",
    ]))
    .expect("command should parse with mixed ordering");

    assert_eq!(
        command,
        Command::Dev {
            path: String::from("main.bst"),
            options: DevServerOptions {
                host: String::from("localhost"),
                port: 6342,
                poll_interval_ms: 900,
            },
        }
    );
}

#[test]
fn new_html_command_rejects_multiple_paths() {
    let error = get_command(&args(&["new", "html", "a", "b"]))
        .expect_err("multiple new html paths should fail");
    assert!(error.contains("at most one path"));
}

#[test]
fn tests_command_uses_default_backend_selection() {
    let command = get_command(&args(&["tests"])).expect("tests command should parse");
    assert_eq!(
        command,
        Command::CompilerTests {
            backend_filter: None,
        }
    );
}

#[test]
fn tests_command_parses_backend_filter() {
    let command = get_command(&args(&["tests", "--backend", "html_wasm"]))
        .expect("tests backend filter should parse");
    assert_eq!(
        command,
        Command::CompilerTests {
            backend_filter: Some(String::from("html_wasm")),
        }
    );
}

#[test]
fn tests_command_rejects_missing_backend_value() {
    let error =
        get_command(&args(&["tests", "--backend"])).expect_err("missing backend value should fail");
    assert!(error.contains("Missing value for --backend"));
}

#[test]
fn tests_command_rejects_unknown_flags() {
    let error =
        get_command(&args(&["tests", "--wat"])).expect_err("unknown tests flag should fail");
    assert!(error.contains("Unknown tests flag"));
}

#[test]
fn check_command_uses_default_options() {
    let command = get_command(&args(&["check"])).expect("check command should parse");
    assert_eq!(
        command,
        Command::Check {
            path: String::new(),
            terse: false,
        }
    );
}

#[test]
fn check_command_parses_path_and_terse_flag() {
    let command = get_command(&args(&["check", "main.bst", "--terse"]))
        .expect("check command should parse path and terse flag");
    assert_eq!(
        command,
        Command::Check {
            path: String::from("main.bst"),
            terse: true,
        }
    );
}

#[test]
fn check_command_supports_mixed_argument_ordering() {
    let command = get_command(&args(&["check", "--terse", "main.bst"]))
        .expect("check command should parse mixed argument ordering");
    assert_eq!(
        command,
        Command::Check {
            path: String::from("main.bst"),
            terse: true,
        }
    );
}

#[test]
fn check_command_rejects_unknown_flags() {
    let error =
        get_command(&args(&["check", "--release"])).expect_err("unknown check flag should fail");
    assert!(error.contains("Unknown check flag"));
}

#[test]
fn check_command_rejects_multiple_paths() {
    let error = get_command(&args(&["check", "a.bst", "b.bst"]))
        .expect_err("multiple check paths should fail");
    assert!(error.contains("at most one path"));
}

#[test]
fn integration_tests_exit_code_is_zero_when_suite_is_correct() {
    let summary = IntegrationRunSummary {
        total_tests: 5,
        passed_tests: 3,
        failed_tests: 0,
        expected_failures: 2,
        unexpected_successes: 0,
    };

    assert_eq!(integration_tests_exit_code(summary), 0);
}

#[test]
fn integration_tests_exit_code_is_non_zero_when_suite_is_incorrect() {
    let summary = IntegrationRunSummary {
        total_tests: 5,
        passed_tests: 2,
        failed_tests: 1,
        expected_failures: 1,
        unexpected_successes: 1,
    };

    assert_eq!(integration_tests_exit_code(summary), 1);
}
