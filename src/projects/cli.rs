//! Command-line entrypoints for the Beanstalk toolchain.
//!
//! This module parses CLI commands and dispatches them into build, dev-server, scaffolding, and
//! compiler test workflows.

use crate::build_system::build;
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::display_messages::print_compiler_messages;
use crate::compiler_tests::integration_test_runner::run_all_test_cases;
use crate::projects::dev_server::{self, DevServerOptions};
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;
use crate::projects::html_project::new_html_project;
use saying::say;
use std::{
    env,
    io::{self, Write},
};

#[derive(Debug, PartialEq, Eq)]
enum Command {
    NewHTMLProject(String), // Creates a new HTML project template

    Build(String), // Builds a file or project

    // Run(&'static str),  //TODO:  Jit the project/file. This will be an eventual Rust interpreter project

    // Runs a hot reloading dev server that can be accessed in the browser
    // Will only support HTML projects for now
    Dev {
        path: String,
        options: DevServerOptions,
    },

    Help,
    CompilerTests, // Runs all the compiler_frontend integration tests for Beanstalk compiler_frontend development
}

pub fn start_cli() {
    let compiler_args: Vec<String> = env::args().collect();

    if compiler_args.len() < 2 {
        // TODO: When interpreter is working properly
        // Start REPL session for running small snippets of Beanstalk code
        // repl::start_repl_session();
        print_help(true);
        return;
    }

    let command = match get_command(&compiler_args[1..]) {
        Ok(command) => command,
        Err(e) => {
            say!(e);
            print_help(true);
            return;
        }
    };

    // Gather a list of any additional flags
    let flags = get_flags(&compiler_args);
    // grey_ln!("compiler_frontend settings {:#?}", flags);

    match command {
        Command::Help => {
            print_help(true);
        }

        Command::NewHTMLProject(path) => {
            let args = prompt_user_for_input("Project name: ");
            let name_args = args.first();

            let project_name = match name_args {
                Some(name) => name,
                None => "",
            };

            match new_html_project::create_html_project_template(path, project_name, flags) {
                Ok(_) => {
                    say!("Creating new HTML project...");
                }
                Err(e) => {
                    say!("Error creating project:  ", e);
                }
            }
        }

        Command::Build(path) => {
            let html_project_builder = Box::new(HtmlProjectBuilder::new());
            let messages = build::build_project(html_project_builder, &path, &flags);
            print_compiler_messages(messages);
        }

        // Command::Run(path) => {
        //     let messages =
        //         build::build_project_files(&path, false, &flags, Some(BuildTarget::Interpreter));
        //     print_compiler_messages(messages);
        // }
        Command::Dev { path, options } => {
            say!("\nStarting dev server...");
            let html_project_builder = Box::new(HtmlProjectBuilder::new());
            match dev_server::run_dev_server(html_project_builder, &path, &flags, options) {
                Ok(_) => {}
                Err(messages) => print_compiler_messages(messages),
            }
        }

        Command::CompilerTests => {
            run_all_test_cases(true);
        }
    }
}

fn get_command(args: &[String]) -> Result<Command, String> {
    let command = args.first().map(String::as_str);

    match command {
        Some("help") => Ok(Command::Help),

        Some("new") => {
            // Check which type of project it is
            match args.get(1).map(String::as_str) {
                Some("html") => {
                    let dir = &prompt_user_for_input("Enter project path: ");

                    if dir.len() == 1 {
                        let dir = dir[0].to_string();
                        Ok(Command::NewHTMLProject(dir))
                    } else {
                        // use the current directory
                        Ok(Command::NewHTMLProject(String::new()))
                    }
                }
                _ => {
                    Err("Invalid project type - currently only 'html' is supported (try 'cargo run new html')".to_string())
                }
            }
        }

        Some("build") => {
            // For now, the backend is always JS
            // Eventually, if the Wasm backend is done,
            // using JS will be a flag that will switch it to that backend
            match args.get(1) {
                Some(path) => Ok(Command::Build(path.to_string())),

                // Return no path (will work from whatever dir the user is inside)
                _ => Ok(Command::Build(String::new())),
            }
        }

        // Some("run") => match args.get(1).map(String::as_str) {
        //     Some(str) => {
        //         Ok(Command::Run(str))
        //     }
        //     _ => Ok(Command::Run("")),
        // },
        Some("dev") => parse_dev_command(args),

        Some("tests") => Ok(Command::CompilerTests),

        _ => Err(format!("Invalid command: '{}'", command.unwrap())),
    }
}

fn get_flags(args: &[String]) -> Vec<Flag> {
    let mut flags = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--release" => flags.push(Flag::Release),
            "--hide-warnings" => flags.push(Flag::DisableWarnings),
            "--hide-timers" => flags.push(Flag::DisableTimers),
            _ => {}
        }
    }

    flags
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
            "--release" | "--hide-warnings" | "--hide-timers" | "--show-warnings" => {
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

fn prompt_user_for_input(msg: &str) -> Vec<String> {
    let mut input = String::new();
    print!("{msg}");
    io::stdout().flush().unwrap(); // Make sure the prompt is immediately displayed
    io::stdin().read_line(&mut input).unwrap();
    let args: Vec<String> = input.split_whitespace().map(String::from).collect();

    args
}

fn print_help(commands_only: bool) {
    if !commands_only {
        say!(Bright Black "------------------------------------");
        say!(Green Bold "The Beanstalk compiler_frontend and build system");
        say!("Usage: ", Bold "<command>",  Italic "<args>");
    }
    say!(Green Bold "\nCommands:");
    //say!("  new <project name>   - Creates a new project");
    //say!(
    //   // "  dev <path>           - Runs the dev server (builds files in dev directory with hot reloading)"
    //);
    //say!("  build <path>         - Builds a file");
    say!("  run <path>        - JITs a file");
    say!("  build <path>      - Builds a project");
    say!("  dev <path>        - Runs the hot reloading dev server");
    say!("  tests             - Runs the test suite");

    say!(Green Bold "\nFlags:");
    say!("  --release");
    say!("  --hide-warnings");
    say!("  --hide-timers");
    say!("  --show-warnings");
    say!("\nDev command options:");
    say!("  --host <host>            (default: 127.0.0.1)");
    say!("  --port <port>            (default: 6342)");
    say!("  --poll-interval-ms <ms>  (default: 300)");
}

#[cfg(test)]
#[path = "tests/cli_tests.rs"]
mod tests;
