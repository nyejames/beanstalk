//! Tests for CLI dev-command parsing and validation.

use super::{Command, get_command};
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
