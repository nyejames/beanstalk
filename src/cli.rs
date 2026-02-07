use crate::build::BuildTarget;
use crate::build_system::html_project::html_project_builder::HtmlProjectBuilder;
use crate::build_system::html_project::new_html_project;
use crate::build_system::repl;
use crate::compiler::basic_utility_functions::check_if_valid_file_path;
use crate::compiler::compiler_errors::{print_compiler_messages, print_formatted_error};
use crate::compiler_tests::integration_test_runner::run_all_test_cases;
use crate::settings::Config;
use crate::{Flag, build, dev_server};
use colour::{e_red_ln, green_ln_bold, grey_ln, red_ln};
use std::path::PathBuf;
use std::{
    env, fs,
    io::{self, Write},
    path::Path,
};

enum Command {
    NewHTMLProject(String), // Creates a new HTML project template

    Build { path: String, target: BuildTarget }, // Builds a file or project in development mode
    Release { path: String, target: BuildTarget }, // Builds a file or project in release mode

    // Run(&'static str),  //TODO:  Jit the project/file. This will be an eventual Rust interpreter project

    // Runs a hot reloading dev server that can be accessed in the browser
    // Will only support HTML projects for now
    Dev { path: String, target: BuildTarget },

    Help,
    CompilerTests, // Runs all the compiler integration tests for Beanstalk compiler development
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
            red_ln!("{}", e);
            print_help(true);
            return;
        }
    };

    // Gather a list of any additional flags
    let flags = get_flags(&compiler_args);
    // grey_ln!("compiler settings {:#?}", flags);

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

            match new_html_project::create_html_project_template(path, project_name) {
                Ok(_) => {
                    println!("Creating new HTML project...");
                }
                Err(e) => {
                    e_red_ln!("Error creating project: {:?}", e);
                }
            }
        }

        Command::Build { path, target } => {
            let html_project_builder = Box::new(HtmlProjectBuilder::new(flags));
            let messages = match build::build_project_files(html_project_builder, &path, false) {
                Ok(messages) => messages,
                Err(e) => {
                    print_formatted_error(e);
                    return;
                }
            };
            print_compiler_messages(messages);
        }

        // Command::Run(path) => {
        //     let messages =
        //         build::build_project_files(&path, false, &flags, Some(BuildTarget::Interpreter));
        //     print_compiler_messages(messages);
        // }
        Command::Release { path, target } => {
            let html_project_builder = Box::new(HtmlProjectBuilder::new(flags));
            let messages = match build::build_project_files(html_project_builder, &path, true) {
                Ok(messages) => messages,
                Err(e) => {
                    print_formatted_error(e);
                    return;
                }
            };
            print_compiler_messages(messages);
        }

        Command::Dev { path, target } => {
            println!("\nStarting dev server...");
            dev_server::start_dev_server(&path, &flags);
        }

        Command::CompilerTests => {
            // Warnings are hidden by default for compiler tests,
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
                Some(str) => Ok(Command::Build {
                    path: str.to_string(),
                    target: BuildTarget::HtmlJSProject,
                }),
                _ => {
                    // Return no path (will work from whatever dir the user is inside)
                    Ok(Command::Build {
                        path: String::new(),
                        target: BuildTarget::HtmlJSProject,
                    })
                }
            }
        }

        // Some("run") => match args.get(1).map(String::as_str) {
        //     Some(str) => {
        //         Ok(Command::Run(str))
        //     }
        //     _ => Ok(Command::Run("")),
        // },
        Some("release") => match args.get(1) {
            Some(str) => Ok(Command::Release {
                path: str.to_string(),
                target: BuildTarget::HtmlJSProject,
            }),
            _ => Ok(Command::Release {
                path: String::new(),
                target: BuildTarget::HtmlJSProject,
            }),
        },

        Some("dev") => match args.get(1) {
            // TODO: remove the testing path and make this a proper path
            Some(path) => {
                if path.is_empty() {
                    Ok(Command::Dev {
                        path: String::from("../../test_output"),
                        target: BuildTarget::HtmlJSProject,
                    })
                } else {
                    Ok(Command::Dev {
                        path: path.to_owned(),
                        target: BuildTarget::HtmlJSProject,
                    })
                }
            }
            None => Ok(Command::Dev {
                path: String::from("../../test_output"),
                target: BuildTarget::HtmlJSProject,
            }),
        },

        Some("tests") => Ok(Command::CompilerTests),

        _ => Err(format!("Invalid command: '{}'", command.unwrap())),
    }
}

fn get_flags(args: &[String]) -> Vec<Flag> {
    let mut flags = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--hide-warnings" => flags.push(Flag::DisableWarnings),
            "--hide-timers" => flags.push(Flag::DisableTimers),
            "--show-warnings" => flags.push(Flag::ShowWarnings),
            _ => {}
        }
    }

    flags
}

// Checks the path and converts it to a PathBuf
// Resolves mixing unix and windows paths
fn check_if_valid_directory_path(path: &str) -> Result<PathBuf, String> {
    // If it contains Unix-style slashes, convert them
    let path = if cfg!(windows) && path.contains('/') {
        // Replace forward slashes with backslashes
        &path.replace('/', "\\")
    } else {
        path
    };

    let path = Path::new(path);

    // Check if the path exists
    if !path.exists() {
        return Err(format!("Path does not exist: {}", path.display()));
    }

    // Check if the path is a directory
    if !path.is_dir() {
        return Err(format!("Path is not a directory: {}", path.display()));
    }

    // Check if the directory is writable
    let metadata = fs::metadata(path).expect("Unable to read metadata");
    if metadata.permissions().readonly() {
        return Err(format!("Directory is not writable: {}", path.display()));
    }

    Ok(path.to_path_buf())
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
        grey_ln!("------------------------------------");
        green_ln_bold!("The Beanstalk compiler and build system");
        println!("Usage: <command> <args>");
    }
    green_ln_bold!("\nCommands:");
    //println!("  new <project name>   - Creates a new project");
    //println!(
    //   // "  dev <path>           - Runs the dev server (builds files in dev directory with hot reloading)"
    //);
    //println!("  build <path>         - Builds a file");
    println!("  run <path>           - JITs a file");
    println!("  release <path>       - Builds a project in release mode");
    println!("  tests                - Runs the test suite");
    // println!("  wat <path>           - Compiles a WAT file to WebAssembly");
}
