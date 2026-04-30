//! Command-line entrypoints for the Beanstalk toolchain.
//!
//! This module parses CLI commands and dispatches them into build, dev-server, scaffolding, and
//! compiler test workflows.

use crate::build_system::build;
use crate::build_system::build::BuildResult;
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::display_messages::print_compiler_messages;
use crate::compiler_tests::integration_test_runner::{
    IntegrationRunSummary, run_all_test_cases, run_all_test_cases_with_backend_filter,
};
use crate::projects::check::{self, CheckOptions};
use crate::projects::dev_server::{self, DevServerOptions};
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;
use crate::projects::html_project::new_html_project::NewHtmlProjectOptions;
use saying::say;
use std::time::Instant;
use std::{env, process};

#[derive(Debug, PartialEq, Eq)]
enum Command {
    NewHTMLProject(NewHtmlProjectOptions), // Creates a new HTML project template

    Build(String), // Builds a file or project

    Check {
        path: String,
        terse: bool,
    }, // Runs frontend-only compilation without writing artefacts

    // Runs a hot reloading dev server that can be accessed in the browser
    // Will only support HTML projects for now
    Dev {
        path: String,
        options: DevServerOptions,
    },

    Help,
    CompilerTests {
        backend_filter: Option<String>,
    }, // Runs all compiler integration tests, optionally filtered by backend
}

pub fn start_cli() {
    let compiler_args: Vec<String> = env::args().collect();
    let cli_args = &compiler_args[1..];

    if cli_args.is_empty() {
        print_help(true);
        return;
    }

    if cli_args[0].starts_with("--") || cli_args[0].starts_with('-') {
        if cli_args
            .iter()
            .all(|arg| arg.starts_with("--") || arg.starts_with('-'))
        {
            let standalone_flags = get_flags(cli_args);
            if standalone_flags.contains(&Flag::Version) {
                println!("bean {}", env!("CARGO_PKG_VERSION"));
            } else {
                print_help(true);
            }
            return;
        }

        say!("Flags must come after a command, unless used on their own.");
        print_help(true);
        return;
    }

    let command = match get_command(cli_args) {
        Ok(command) => command,
        Err(e) => {
            say!(e);
            print_help(true);
            return;
        }
    };

    let flags = get_flags(&cli_args[1..]);

    match command {
        Command::Help => {
            print_help(true);
        }

        Command::NewHTMLProject(options) => {
            match crate::projects::html_project::new_html_project::create_html_project_template(
                options,
            ) {
                Ok(_) => {}
                Err(e) if e == "Cancelled project creation." => {
                    println!("{e}");
                }
                Err(e) => {
                    println!("{e}");
                }
            }
        }

        Command::Build(path) => {
            let start = Instant::now();
            let project_builder = build::ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
            match build::build_project(&project_builder, &path, &flags) {
                Ok(build_result) => {
                    let output_root = if build_result.config.entry_dir.is_dir() {
                        build::resolve_project_output_root(&build_result.config, &flags)
                    } else {
                        match env::current_dir() {
                            Ok(path) => path,
                            Err(error) => {
                                print_compiler_messages(CompilerMessages {
                                    errors: vec![CompilerError::compiler_error(format!(
                                        "Could not resolve current directory for build outputs: {error}"
                                    ))],
                                    warnings: build_result.warnings,
                                    string_table: build_result.string_table,
                                });
                                return;
                            }
                        }
                    };

                    let write_result = build::write_project_outputs(
                        &build_result.project,
                        &build::WriteOptions {
                            output_root,
                            project_entry_dir: Some(build_result.config.entry_dir.clone()),
                            write_mode: build::WriteMode::AlwaysWrite,
                        },
                        &build_result.string_table,
                    );

                    match write_result {
                        Ok(()) => {
                            let duration = start.elapsed();
                            print_build_message(build_result, duration);
                        }
                        Err(mut messages) => {
                            messages.warnings.extend(build_result.warnings);
                            print_compiler_messages(messages);
                        }
                    }
                }
                Err(messages) => print_compiler_messages(messages),
            }
        }

        Command::Check { path, terse } => {
            check::run_check(&path, CheckOptions { terse });
        }

        Command::Dev { path, options } => {
            say!("\nStarting dev server...");
            let project_builder = build::ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
            match dev_server::run_dev_server(project_builder, &path, &flags, options) {
                Ok(_) => {}
                Err(messages) => print_compiler_messages(messages),
            }
        }

        Command::CompilerTests { backend_filter } => {
            let summary_result = if let Some(backend_filter) = backend_filter.as_deref() {
                run_all_test_cases_with_backend_filter(true, Some(backend_filter))
            } else {
                run_all_test_cases(true)
            };

            match summary_result {
                Ok(summary) => {
                    let exit_code = integration_tests_exit_code(summary);
                    if exit_code != 0 {
                        process::exit(exit_code);
                    }
                }
                Err(error) => {
                    say!(Red "Failed to run integration tests:");
                    println!("  {error}");
                    process::exit(1);
                }
            }
        }
    }
}

fn integration_tests_exit_code(summary: IntegrationRunSummary) -> i32 {
    if summary.incorrect_results() > 0 {
        1
    } else {
        0
    }
}

fn get_command(args: &[String]) -> Result<Command, String> {
    let command = args.first().map(String::as_str);

    match command {
        Some("help") => Ok(Command::Help),

        Some("new") => parse_new_command(args),

        Some("build") => parse_build_command(args),

        Some("check") => parse_check_command(args),

        Some("dev") => parse_dev_command(args),

        Some("tests") => parse_tests_command(args),

        Some(other) => Err(format!("Invalid command: '{other}'")),
        None => Err(String::from("Missing command.")),
    }
}

fn get_flags(args: &[String]) -> Vec<Flag> {
    let mut flags = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--version" | "-v" | "-V" => flags.push(Flag::Version),
            "--release" => flags.push(Flag::Release),
            "--hide-warnings" => flags.push(Flag::DisableWarnings),
            "--hide-timers" => flags.push(Flag::DisableTimers),
            "--html-wasm" => flags.push(Flag::HtmlWasm),
            _ => {}
        }
    }

    flags
}

fn parse_new_command(args: &[String]) -> Result<Command, String> {
    match args.get(1).map(String::as_str) {
        Some("html") => {
            let mut raw_path = None;
            let mut force = false;
            let mut index = 2usize;

            while let Some(arg) = args.get(index) {
                match arg.as_str() {
                    "--force" => {
                        force = true;
                        index += 1;
                    }
                    "--release" | "--hide-warnings" | "--hide-timers" | "--show-warnings"
                    | "--html-wasm" => {
                        index += 1;
                    }
                    _ if arg.starts_with("--") => {
                        return Err(format!(
                            "Unknown new flag: '{arg}'. Supported flags are --force."
                        ));
                    }
                    _ => {
                        if raw_path.is_none() {
                            raw_path = Some(arg.to_owned());
                            index += 1;
                        } else {
                            return Err(String::from(
                                "New html command accepts at most one path argument.",
                            ));
                        }
                    }
                }
            }

            Ok(Command::NewHTMLProject(NewHtmlProjectOptions { raw_path, force }))
        }
        _ => {
            Err("Invalid project type - currently only 'html' is supported (try 'cargo run -- new html')".to_string())
        }
    }
}

fn parse_build_command(args: &[String]) -> Result<Command, String> {
    let mut path = String::new();
    let mut index = 1usize;

    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "--release" | "--hide-warnings" | "--hide-timers" | "--show-warnings"
            | "--html-wasm" => {
                index += 1;
            }
            _ if arg.starts_with("--") => {
                return Err(format!(
                    "Unknown build flag: '{arg}'. Supported build flags are --release, --hide-warnings, --hide-timers, and --html-wasm."
                ));
            }
            _ => {
                if path.is_empty() {
                    path = arg.to_owned();
                    index += 1;
                } else {
                    return Err(String::from(
                        "Build command accepts at most one path argument.",
                    ));
                }
            }
        }
    }

    Ok(Command::Build(path))
}

fn parse_tests_command(args: &[String]) -> Result<Command, String> {
    // Parse optional backend filtering flags for integration tests so local loops can
    // focus on one backend profile without changing fixture contracts.
    let mut backend_filter = None;
    let mut index = 1usize;

    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "--backend" => {
                let Some(backend_value) = args.get(index + 1) else {
                    return Err(String::from(
                        "Missing value for --backend. Supported values: html, html_wasm.",
                    ));
                };
                if backend_value.starts_with("--") {
                    return Err(String::from(
                        "Missing value for --backend. Supported values: html, html_wasm.",
                    ));
                }
                if backend_filter.is_some() {
                    return Err(String::from(
                        "Tests command accepts at most one --backend value.",
                    ));
                }
                backend_filter = Some(backend_value.to_owned());
                index += 2;
            }
            _ if arg.starts_with("--") => {
                return Err(format!(
                    "Unknown tests flag: '{arg}'. Supported tests flag is --backend <html|html_wasm>."
                ));
            }
            _ => {
                return Err(String::from(
                    "Tests command does not accept positional arguments.",
                ));
            }
        }
    }

    Ok(Command::CompilerTests { backend_filter })
}

fn parse_check_command(args: &[String]) -> Result<Command, String> {
    let mut path = String::new();
    let mut terse = false;
    let mut index = 1usize;

    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "--terse" => {
                terse = true;
                index += 1;
            }
            _ if arg.starts_with("--") => {
                return Err(format!(
                    "Unknown check flag: '{arg}'. Supported check flag is --terse."
                ));
            }
            _ => {
                if path.is_empty() {
                    path = arg.to_owned();
                    index += 1;
                } else {
                    return Err(String::from(
                        "Check command accepts at most one path argument.",
                    ));
                }
            }
        }
    }

    Ok(Command::Check { path, terse })
}

fn parse_dev_command(args: &[String]) -> Result<Command, String> {
    let mut path = String::new();
    let mut options = DevServerOptions::default();
    let mut index = 1usize;

    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "--host" => {
                let Some(host) = args.get(index + 1) else {
                    return Err(String::from("Missing value for --host"));
                };
                if host.starts_with("--") {
                    return Err(String::from("Missing value for --host"));
                }
                options.host = host.to_owned();
                index += 2;
            }
            "--port" => {
                let Some(port_value) = args.get(index + 1) else {
                    return Err(String::from("Missing value for --port"));
                };
                if port_value.starts_with("--") {
                    return Err(String::from("Missing value for --port"));
                }
                options.port = match port_value.parse::<u16>() {
                    Ok(port) => port,
                    Err(_) => {
                        return Err(format!(
                            "Invalid --port value: '{port_value}'. Port must be a number from 0 to 65535."
                        ));
                    }
                };
                index += 2;
            }
            "--poll-interval-ms" => {
                let Some(interval_value) = args.get(index + 1) else {
                    return Err(String::from("Missing value for --poll-interval-ms"));
                };
                if interval_value.starts_with("--") {
                    return Err(String::from("Missing value for --poll-interval-ms"));
                }
                options.poll_interval_ms = match interval_value.parse::<u64>() {
                    Ok(interval) if interval > 0 => interval,
                    Ok(_) => {
                        return Err(String::from(
                            "Invalid --poll-interval-ms value: '0'. It must be greater than zero.",
                        ));
                    }
                    Err(_) => {
                        return Err(format!(
                            "Invalid --poll-interval-ms value: '{interval_value}'. It must be a positive integer."
                        ));
                    }
                };
                index += 2;
            }
            "--release" | "--hide-warnings" | "--hide-timers" | "--show-warnings"
            | "--html-wasm" => {
                index += 1;
            }
            _ if arg.starts_with("--") => {
                return Err(format!(
                    "Unknown dev flag: '{arg}'. Supported dev flags are --host, --port, --poll-interval-ms."
                ));
            }
            _ => {
                if path.is_empty() {
                    path = arg.to_owned();
                    index += 1;
                } else {
                    return Err(String::from(
                        "Dev command accepts at most one path argument.",
                    ));
                }
            }
        }
    }

    Ok(Command::Dev { path, options })
}

fn print_help(commands_only: bool) {
    if !commands_only {
        say!(Bright Black "------------------------------------");
        say!(Green Bold "The Beanstalk compiler and build system");
        say!("Usage: ", Bold "<command>",  Italic "<args>");
    }

    say!(Green Bold "Beanstalk", Reset " is version ", Blue Bold env!("CARGO_PKG_VERSION"));

    say!(Green Bold "\nCommands:");
    say!("  build [path]      - Builds a project");
    say!("  check [path]      - Runs frontend-only diagnostics (no artifacts)");
    say!("  dev [path]        - Runs the hot reloading dev server");
    say!("  new html [path] [--force] - Creates an HTML project scaffold");
    say!("  tests [--backend <id>] - Runs the integration test suite");

    say!(Green Bold "\nFlags:");
    say!("  --release");
    say!("  --hide-warnings");
    say!("  --hide-timers");
    say!("  --show-warnings");
    say!("  --html-wasm");
    say!("\nTests command options:");
    say!("  --backend <id>         (supported: html, html_wasm)");
    say!("\nCheck command options:");
    say!("  --terse                (compact one-line diagnostics)");
    say!("\nDev command options:");
    say!("  --host <host>            (default: 127.0.0.1)");
    say!("  --port <port>            (default: 6342)");
    say!("  --poll-interval-ms <ms>  (default: 300)");
}

fn print_build_message(build_result: BuildResult, duration: std::time::Duration) {
    say!(
        "\n🌱 Built ",
        Blue build_result.project.output_files.len(),
        Reset " files successfully in: ",
        Green Bold #duration,
    );

    print_compiler_messages(CompilerMessages {
        errors: Vec::new(),
        warnings: build_result.warnings,
        string_table: build_result.string_table,
    });
}

#[cfg(test)]
#[path = "tests/cli_tests.rs"]
mod tests;
