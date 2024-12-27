use std::path::PathBuf;
use std::time::Instant;
use std::{env, fs, io::{self, Write}, path::Path};

mod bs_css;
pub mod bs_types;
mod build;
mod create_new_project;
pub mod dev_server;
mod settings;
mod test;
mod tokenize_scene;
mod tokenizer;
mod tokens;
mod parsers {
    pub mod ast_nodes;
    pub mod build_ast;
    pub mod collections;
    mod create_scene_node;
    pub mod functions;
    mod expressions {
        pub mod constant_folding;
        pub mod eval_expression;
        pub mod parse_expression;
    }
    pub mod styles;
    pub mod tuples;
    pub mod util;
    pub mod variables;
}
mod html_output {
    pub mod colors;
    pub mod dom_hooks;
    pub mod generate_html;
    pub mod js_parser;
    pub mod web_parser;
    pub mod code_block_highlighting;
}
mod wasm_output {
    pub mod wasm_generator;
    pub mod wat_parser;
}
use colour::{dark_cyan, e_dark_red_ln_bold, e_dark_yellow_ln, green_ln_bold, grey_ln, red_ln};
pub use tokens::Token;
enum Command {
    NewHTMLProject(PathBuf),
    Dev(PathBuf),  // Runs local dev server
    Release(PathBuf),
    Test,
    Wat(PathBuf), // Compiles a WAT file to WebAssembly
}

pub struct CompileError {
    pub msg: String,
    pub line_number: u32,
}

fn main() {
    let compiler_args: Vec<String> = env::args().collect();

    if compiler_args.len() < 2 {
        print_help(false);
        return;
    }

    let command = match get_command(&compiler_args[1..].to_vec()) {
        Ok(command) => command,
        Err(e) => {
            red_ln!("{}", e);
            print_help(true);
            return;
        }
    };

    match command {
        Command::NewHTMLProject(path) => {
            let args = prompt_user_for_input("Project name: ".to_string());
            let name_args = args.get(0);

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
                    red_ln!("Error creating project: {:?}", e);
                }
            }
        }

        Command::Release(path) => {
            dark_cyan!("Building project...");
            let start = Instant::now();
            match build::build(&path, true) {
                Ok(_) => {
                    let duration = start.elapsed();
                    grey_ln!("------------------------------------");
                    print!("\nProject built in: ");
                    green_ln_bold!("{:?}", duration);
                }
                Err(e) => {
                    red_ln!("Error building project: {:?}", e.msg);
                }
            }
        }

        Command::Test => {
            println!("Testing...");
            let test_path = PathBuf::from("test_output");
            match test::test_build(&test_path) {
                Ok(_) => {}
                Err(e) => {
                    print_formatted_error(&e, &test_path.join("src/#page.bs"));
                }
            };
        }

        Command::Dev(path) => {
            println!("Starting dev server...");
            let mut path = PathBuf::from(path);

            match dev_server::start_dev_server(&mut path) {
                Ok(_) => {
                    println!("Dev server shutting down ... ");
                }
                Err(e) => {
                    print_formatted_error(&e, &path);
                }
            }
        }

        Command::Wat(path) => {
            println!("Compiling WAT to WebAssembly...");
            match wasm_output::wasm_generator::compile_wat_file(&path) {
                Ok(_) => {}
                Err(e) => {
                    print_formatted_error(&e, &path);
                }
            }
        }
    }
}
fn get_command(args: &Vec<String>) -> Result<Command, String> {
    match args.get(0).map(String::as_str) {

        Some("new") => {
            // Check type of project
            match args.get(1).map(String::as_str) {
                Some("html") => {
                    let dir = &prompt_user_for_input("Enter project path: ".to_string());

                    if dir.len() == 1 {
                        let dir = dir[0].to_string();
                        check_if_valid_directory_path(&dir)?;
                        Ok(Command::NewHTMLProject(PathBuf::from(dir)))
                    } else {
                        // use current directory
                        Ok(Command::NewHTMLProject(PathBuf::from("")))
                    }
                }
                _ => {
                   Err("Invalid project type - currently only 'html' is supported (try 'cargo run new html')".to_string())
                }
            }
        }

        Some("release") => {
            let entry_path = env::current_dir()
                .map_err(|e| format!("Error getting current directory: {:?}", e))?;

            match args.get(1).map(String::as_str) {
                Some(string) => {
                    Ok(Command::Release(entry_path.join(string)))
                }
                _ => {
                    // Return current working directory path
                    Ok(Command::Release(entry_path))
                }
            }
        }

        Some("test") => {
            Ok(Command::Test)
        }

        Some("dev") => {
            match args.get(1) {
                Some(path) => {
                    if path.is_empty() {
                        Ok(Command::Dev(PathBuf::from("test_output")))
                    } else {
                        Ok(Command::Dev(PathBuf::from(path)))
                    }
                }
                None => Ok(Command::Dev(PathBuf::from("test_output"))),
            }
        }

        Some("wat") => {
            match args.get(1).map(String::as_str) {
                Some(path) => {
                    if path.is_empty() {
                        Ok(Command::Wat(PathBuf::from("test_output/test.wat")))
                    } else {
                        Ok(Command::Wat(PathBuf::from(path)))
                    }
                }
                None => Ok(Command::Wat(PathBuf::from("test_output/test.wat"))),
            }
        }

        _ => {
            Ok(Command::Test)
        }
    }
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

fn prompt_user_for_input(msg: String) -> Vec<String> {
    let mut input = String::new();
    print!("{}", msg);
    io::stdout().flush().unwrap(); // Make sure the prompt is immediately displayed
    io::stdin().read_line(&mut input).unwrap();
    let args: Vec<String> = input.split_whitespace().map(String::from).collect();

    args
}

fn print_formatted_error(e: &CompileError, file_path: &PathBuf) {
    // Read the file and get the line as a string
    let file = match fs::read_to_string(file_path) {
        Ok(file) => file,
        Err(e) => {
            red_ln!("Error reading file path when printing errors: {:?}", e);
            return;
        }
    };

    let line = match file.lines().nth(e.line_number as usize) {
        Some(line) => line,
        None => {
            red_ln!("Error: Line number is out of range");
            return;
        }
    };

    println!("(╯°□°)╯  🔥🔥🔥🔥🔥🔥🔥🔥 ");

    if e.line_number == 0 {
        e_dark_yellow_ln!("Error during compilation");
        e_dark_red_ln_bold!("{}", e.msg);
    } else {
        e_dark_yellow_ln!("Error during compilation at line {}", e.line_number);
        e_dark_red_ln_bold!("{}", e.msg);

        println!("{}", line);
        red_ln!("{}", std::iter::repeat('^').take(line.len()).collect::<String>());
    }

    println!("🔥🔥🔥🔥🔥🔥🔥🔥   ╰(°□°╰)");
}

fn print_help(commands_only: bool) {
    if !commands_only {
        grey_ln!("------------------------------------");
        green_ln_bold!("The Beanstalk compiler!");
        println!("Usage: cargo run <command> <args>");
    }
    green_ln_bold!("Commands:");
    println!("  new <project name>   - Creates a new HTML project");
    println!("  dev <path>           - Runs the dev server (builds files in dev directory with hot reloading)");
    println!("  build <path>         - Builds a file");
    println!("  release <path>       - Builds a project in release mode");
    println!("  test                 - Runs the test suite (currently just for testing the compiler)");
    println!("  wat <path>           - Compiles a WAT file to WebAssembly");
}