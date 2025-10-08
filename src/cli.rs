use crate::build_system::repl;
use crate::compiler::codegen::wat_to_wasm::compile_wat_file;
use crate::compiler::compiler_errors::{print_errors, print_formatted_error};
use crate::compiler_tests::run_all_test_cases;
use crate::{Flag, build, create_new_project, dev_server, timer_log};
use colour::{e_red_ln, green_ln_bold, grey_ln, red_ln};
use std::path::PathBuf;
use std::time::Instant;
use std::{
    env, fs,
    io::{self, Write},
    path::Path,
};



enum Command {
    NewHTMLProject(PathBuf),
    Build(PathBuf), // Builds a file or project in development mode

    Run(PathBuf), // Jit the project/file

    Dev(PathBuf), // Runs local dev server
    Release(PathBuf),
    Wat(PathBuf), // Compiles a WAT file to WebAssembly

    Help,
    CompilerTests,
    AnalyzeCode,
}

pub fn start_cli() {

    
    let compiler_args: Vec<String> = env::args().collect();

    if compiler_args.len() < 2 {
        // Start REPL session for running small snippets of Beanstalk code
        repl::start_repl_session();
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
                Some(name) => {
                    if name.is_empty() {
                        "test_output".to_string()
                    } else {
                        name.to_string()
                    }
                }
                None => "test_output".to_string(),
            };

            match create_new_project::create_project(path, &project_name) {
                Ok(_) => {
                    println!("Creating new HTML project...");
                }
                Err(e) => {
                    e_red_ln!("Error creating project: {:?}", e);
                }
            }
        }

        Command::Build(path) => {
            build_project(&path, false, &flags);
        }

        Command::Run(path) => {
            jit_project(&path, &flags);
        }

        Command::Release(path) => {
            build_project(&path, true, &flags);
        }

        Command::Dev(ref path) => {
            println!("Starting dev server...");

            match dev_server::start_dev_server(path, &flags) {
                Ok(_) => {
                    println!("Dev server shutting down ... ");
                }
                Err(e) => {
                    e_red_ln!("Errors while building project: \n");
                    print_errors(e);
                }
            }
        }

        Command::Wat(path) => {
            println!("Compiling WAT to WebAssembly...");
            match compile_wat_file(&path) {
                Ok(_) => {}
                Err(e) => {
                    print_formatted_error(e);
                }
            }
        }

        Command::CompilerTests => {
            run_all_test_cases();
        }

        Command::AnalyzeCode => {
            use crate::compiler_tests::run_comprehensive_analysis;
            match run_comprehensive_analysis() {
                Ok(()) => {
                    green_ln_bold!("Code analysis completed successfully!");
                }
                Err(e) => {
                    e_red_ln!("Code analysis failed: {}", e);
                }
            }
        }
    }
}

fn get_command(args: &[String]) -> Result<Command, String> {
    let command = args.first().map(String::as_str);

    match command {
        Some("help") => Ok(Command::Help),

        Some("new") => {
            // Check type of project
            match args.get(1).map(String::as_str) {
                Some("html") => {
                    let dir = &prompt_user_for_input("Enter project path: ");

                    if dir.len() == 1 {
                        let dir = dir[0].to_string();
                        check_if_valid_directory_path(&dir)?;
                        Ok(Command::NewHTMLProject(PathBuf::from(dir)))
                    } else {
                        // use the current directory
                        Ok(Command::NewHTMLProject(PathBuf::from("")))
                    }
                }
                _ => {
                    Err("Invalid project type - currently only 'html' is supported (try 'cargo run new html')".to_string())
                }
            }
        }

        Some("build") => {
            let entry_path = env::current_dir()
                .map_err(|e| format!("Error getting current directory: {:?}", e))?;

            match args.get(1).map(String::as_str) {
                Some(string) => Ok(Command::Build(entry_path.join(string))),
                _ => {
                    // Return the current working directory path
                    Ok(Command::Build(entry_path))
                }
            }
        }

        Some("run") => {
            let entry_path = env::current_dir()
                .map_err(|e| format!("Error getting current directory: {:?}", e))?;

            match args.get(1).map(String::as_str) {
                Some(string) => Ok(Command::Run(entry_path.join(string))),
                _ => Ok(Command::Run(entry_path)),
            }
        }

        Some("release") => {
            let entry_path = env::current_dir()
                .map_err(|e| format!("Error getting current directory: {:?}", e))?;

            match args.get(1).map(String::as_str) {
                Some(string) => Ok(Command::Release(entry_path.join(string))),
                _ => Ok(Command::Release(entry_path)),
            }
        }

        Some("dev") => match args.get(1) {
            Some(path) => {
                if path.is_empty() {
                    Ok(Command::Dev(PathBuf::from("../../test_output")))
                } else {
                    Ok(Command::Dev(PathBuf::from(path)))
                }
            }
            None => Ok(Command::Dev(PathBuf::from("../../test_output"))),
        },

        Some("wat") => match args.get(1).map(String::as_str) {
            Some(path) => {
                if path.is_empty() {
                    Ok(Command::Wat(PathBuf::from("../../test_output/test.wat")))
                } else {
                    Ok(Command::Wat(PathBuf::from(path)))
                }
            }
            None => Ok(Command::Wat(PathBuf::from("../../test_output/test.wat"))),
        },

        Some("tests") => Ok(Command::CompilerTests),

        Some("analyze") => Ok(Command::AnalyzeCode),

        _ => Err(format!("Invalid command: '{}'", command.unwrap())),
    }
}

fn build_project(path: &Path, release_build: bool, flags: &[Flag]) {
    let start = Instant::now();

    match build::build_project_files(path, release_build, flags) {
        Ok(project) => {
            let duration = start.elapsed();

            // Show build results
            println!("Build completed successfully");
            println!("Generated {} output file(s)", project.output_files.len());

            grey_ln!("------------------------------------");
            print!("\nProject built in: ");
            green_ln_bold!("{:?}", duration);
        }
        Err(e) => {
            e_red_ln!("Errors while building project: \n");
            print_errors(e);
        }
    }
}

fn jit_project(path: &Path, flags: &[Flag]) {
    use crate::build_system::build_system::BuildTarget;
    let start = Instant::now();

    match build::build_project_files_with_target(path, false, flags, Some(BuildTarget::Jit)) {
        Ok(_project) => {
            timer_log!(start, "\nJIT program ran for: ");
        }
        Err(e) => {
            e_red_ln!("Errors while running project: \n");
            print_errors(e);
        }
    }
}

fn get_flags(args: &[String]) -> Vec<Flag> {
    let mut flags = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--ast" => flags.push(Flag::ShowAst),
            "--hide-warnings" => flags.push(Flag::DisableWarnings),
            "--hide-timers" => flags.push(Flag::DisableTimers),

            _ => {}
        }
    }

    flags
}

fn check_if_valid_directory_path(path: &str) -> Result<(), String> {
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

    Ok(())
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
        green_ln_bold!("The Beanstalk compiler!");
        println!("Usage: cargo run <command> <args>");
    }
    green_ln_bold!("Commands:");
    println!("  new <project name>   - Creates a new project");
    println!(
        "  dev <path>           - Runs the dev server (builds files in dev directory with hot reloading)"
    );
    println!("  build <path>         - Builds a file");
    println!("  release <path>       - Builds a project in release mode");
    println!(
        "  test                 - Runs the test suite (currently just for testing the compiler)"
    );
    println!("  wat <path>           - Compiles a WAT file to WebAssembly");
}
