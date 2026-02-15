use crate::build_system::build;
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::display_messages::print_compiler_messages;
use crate::compiler_tests::integration_test_runner::run_all_test_cases;
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;
use crate::projects::html_project::{dev_server, new_html_project};
use saying::say;
use std::{
    env,
    io::{self, Write},
};

enum Command {
    NewHTMLProject(String), // Creates a new HTML project template

    Build(String), // Builds a file or project

    // Run(&'static str),  //TODO:  Jit the project/file. This will be an eventual Rust interpreter project

    // Runs a hot reloading dev server that can be accessed in the browser
    // Will only support HTML projects for now
    Dev(String),

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
        Command::Dev(path) => {
            say!("\nStarting dev server...");
            dev_server::start_dev_server(&path, &flags);
        }

        Command::CompilerTests => {
            // Warnings are hidden by default for compiler_frontend tests,
            // unless the show-warnings flag is set
            let show_warnings = flags.contains(&Flag::ShowWarnings);
            run_all_test_cases(show_warnings);
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
        Some("dev") => match args.get(1) {
            // TODO: remove the testing path and make this a proper path
            Some(path) => {
                if path.is_empty() {
                    Ok(Command::Dev(String::from("../../test_output")))
                } else {
                    Ok(Command::Dev(path.to_owned()))
                }
            }
            None => Ok(Command::Dev(String::from("../../test_output"))),
        },

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
            "--show-warnings" => flags.push(Flag::ShowWarnings),
            _ => {}
        }
    }

    flags
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
    say!("  tests             - Runs the test suite");

    say!(Green Bold "\nFlags:");
    say!("  --release");
    say!("  --hide-warnings");
    say!("  --hide-timers");
    say!("  --show-warnings");
}
