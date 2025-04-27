use std::path::PathBuf;
use std::time::Instant;
use std::{
    env, fs,
    io::{self, Write},
    path::Path,
};

pub mod bs_types;
mod build;
mod create_new_project;
pub mod dev_server;
mod settings;
mod test;
mod tokenizer;
mod tokens;
mod parsers {
    pub mod ast_nodes;
    pub mod build_ast;
    pub mod collections;
    mod create_scene_node;
    pub mod functions;
    pub mod markdown;
    mod expressions {
        pub mod constant_folding;
        pub mod eval_expression;
        pub mod function_call_inline;
        pub mod parse_expression;
    }
    pub mod codeblock;
    pub mod scene;
    pub mod structs;
    pub mod util;
    pub mod variables;
}
mod html_output {
    pub mod code_block_highlighting;
    // pub mod colors;
    pub mod dom_hooks;
    pub mod generate_html;
    pub mod html_styles;
    pub mod js_parser;
    pub mod web_parser;
}
mod wasm_output {
    pub mod wat_to_wasm;
}
use crate::tokenizer::TokenPosition;
use colour::{
    dark_red, dark_red_ln, e_dark_blue_ln, e_dark_magenta, e_dark_yellow_ln,
    e_magenta_ln, e_red_ln, e_yellow, e_yellow_ln, green_ln_bold, grey_ln, red_ln,
};

pub use tokens::Token;

enum Command {
    NewHTMLProject(PathBuf),
    Dev(PathBuf), // Runs local dev server
    Release(PathBuf),
    Test,
    Wat(PathBuf), // Compiles a WAT file to WebAssembly
}

pub struct CompileError {
    pub msg: String,

    // If start pos is 0, there is no line to print
    pub start_pos: TokenPosition,

    // After the start pos
    // This can be the entire rest of the line
    // and will be set to U32::MAX in this case
    // 0 if there is no line to print
    pub end_pos: TokenPosition,

    pub error_type: ErrorType,
}

impl CompileError {
    fn to_error(self, file_path: PathBuf) -> Error {
        Error {
            msg: self.msg,
            start_pos: self.start_pos,
            end_pos: self.end_pos,
            file_path,
            error_type: self.error_type,
        }
    }
}

// Adds more information to the CompileError
// So it knows the file path (possible specific part of the line soon)
// And the type of error
#[derive(PartialEq)]
pub enum ErrorType {
    Suggestion,
    Caution,
    Syntax,
    Type,
    Rule,
    File,
    Compiler,
    DevServer,
}
pub struct Error {
    msg: String,
    start_pos: TokenPosition,
    end_pos: TokenPosition,
    file_path: PathBuf,
    error_type: ErrorType,
}

// Compiler Output_info_levels
// Will need to be converted to cli args eventually
// So cba to do enum yet because this might just change to strings or something
const SHOW_TOKENS: i32 = 6;
const SHOW_AST: i32 = 5;
const SHOW_WAT: i32 = 4;
const SHOW_TIMINGS: i32 = 3;
const DONT_SHOW_TIMINGS: i32 = 2;

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
                    red_ln!("Error creating project: {:?}", e);
                }
            }
        }

        Command::Release(path) => {
            let start = Instant::now();

            // TODO - parse config file instead of using default config
            match build::build(&path, true, SHOW_TIMINGS) {
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
                    print_formatted_error(e);
                }
            };
        }

        Command::Dev(ref path) => {
            println!("Starting dev server...");

            match dev_server::start_dev_server(path, DONT_SHOW_TIMINGS) {
                Ok(_) => {
                    println!("Dev server shutting down ... ");
                }
                Err(e) => {
                    print_formatted_error(e);
                }
            }
        }

        Command::Wat(path) => {
            println!("Compiling WAT to WebAssembly...");
            match wasm_output::wat_to_wasm::compile_wat_file(&path) {
                Ok(_) => {}
                Err(e) => {
                    print_formatted_error(e.to_error(path));
                }
            }
        }
    }
}
fn get_command(args: &[String]) -> Result<Command, String> {
    match args.first().map(String::as_str) {
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
                Some(string) => Ok(Command::Release(entry_path.join(string))),
                _ => {
                    // Return current working directory path
                    Ok(Command::Release(entry_path))
                }
            }
        }

        Some("test") => Ok(Command::Test),

        Some("dev") => match args.get(1) {
            Some(path) => {
                if path.is_empty() {
                    Ok(Command::Dev(PathBuf::from("test_output")))
                } else {
                    Ok(Command::Dev(PathBuf::from(path)))
                }
            }
            None => Ok(Command::Dev(PathBuf::from("test_output"))),
        },

        Some("wat") => match args.get(1).map(String::as_str) {
            Some(path) => {
                if path.is_empty() {
                    Ok(Command::Wat(PathBuf::from("test_output/test.wat")))
                } else {
                    Ok(Command::Wat(PathBuf::from(path)))
                }
            }
            None => Ok(Command::Wat(PathBuf::from("test_output/test.wat"))),
        },

        _ => Ok(Command::Test),
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

fn print_formatted_error(e: Error) {
    // Walk back through the file path until it's the current directory
    let relative_dir = match env::current_dir() {
        Ok(dir) => e
            .file_path
            .strip_prefix(dir)
            .unwrap_or(&e.file_path)
            .to_string_lossy(),
        Err(_) => e.file_path.to_string_lossy(),
    };

    // Read the file and get the actual line as a string from the code
    let line = match fs::read_to_string(&e.file_path) {
        Ok(file) => file
            .lines()
            .nth(e.start_pos.line_number as usize)
            .unwrap_or_default()
            .to_string(),
        Err(_) => {
            // red_ln!("Error with printing error ヽ༼☉ ‿ ⚆༽ﾉ File path is invalid: {}", e.file_path.display());
            "".to_string()
        }
    };

    // red_ln!("Error with printing error ヽ༼☉ ‿ ⚆༽ﾉ Line number is out of range of file. If you see this, it confirms the compiler developer is an idiot");

    // e_dark_yellow!("Error: ");

    match e.error_type {
        // This probably won't be used for the compiler
        ErrorType::Suggestion => {
            print!("\n( ͡° ͜ʖ ͡°) ");
            dark_red_ln!("{}", relative_dir);
            println!(" ( ._. ) ");
            e_dark_blue_ln!("Suggestion");
            e_dark_magenta!("Line ");
            e_magenta_ln!("{}\n", e.start_pos.line_number + 1);
        }

        ErrorType::Caution => {
            print!("\n(ಠ_ಠ)☞  ⚠ ");
            dark_red!("{}", relative_dir);
            println!("⚠  ☜(■_■¬ ) ");

            e_yellow_ln!("Caution");
            e_dark_magenta!("Line ");
            e_magenta_ln!("{}\n", e.start_pos.line_number + 1);
        }

        ErrorType::Syntax => {
            print!("\n(╯°□°)╯  🔥🔥 ");
            dark_red!("{}", relative_dir);
            println!(" 🔥🔥  Σ(°△°;) ");

            e_red_ln!("Syntax");
            e_dark_magenta!("Line ");
            e_magenta_ln!("{}\n", e.start_pos.line_number + 1);
        }

        ErrorType::Type => {
            print!("\n( ͡° ͜ʖ ͡°) ");
            dark_red_ln!("{}", relative_dir);
            println!(" ( ._. ) ");

            e_red_ln!("Type Error");
            e_dark_magenta!("Line ");
            e_magenta_ln!("{}\n", e.start_pos.line_number + 1);
        }

        ErrorType::Rule => {
            print!("\nヽ(˶°o°)ﾉ  🔥🔥🔥 ");
            dark_red!("{}", relative_dir);
            println!(" 🔥🔥🔥  ╰(°□°╰) ");

            e_red_ln!("Rule");
            e_dark_magenta!("Line ");
            e_magenta_ln!("{}\n", e.start_pos.line_number + 1);
        }

        ErrorType::File => {
            e_yellow_ln!("🏚 Can't find/read file or directory");
            e_red_ln!("  {}", e.msg);
            return;
        }

        ErrorType::Compiler => {
            print!("\nヽ༼☉ ‿ ⚆༽ﾉ  🔥🔥🔥🔥 ");
            dark_red!("{}", relative_dir);
            println!(" 🔥🔥🔥🔥  ╰(° _ o╰) ");
            e_yellow!("COMPILER BUG - ");
            e_dark_yellow_ln!("compiler developer skill issue (not your fault)");

            e_dark_magenta!("Line ");
            e_magenta_ln!("{}\n", e.start_pos.line_number + 1);
        }

        ErrorType::DevServer => {
            print!("\n(ﾉ☉_⚆)ﾉ  🔥 ");
            dark_red!("{}", relative_dir);
            println!(" 🔥 ╰(° O °)╯ ");
            e_yellow_ln!("Dev Server whoopsie");
            e_red_ln!("  {}", e.msg);
            return;
        }
    }

    e_red_ln!("  {}", e.msg);

    println!("\n{}", line);

    // spaces before the relevant part of the line
    print!("{}", " ".repeat(e.start_pos.char_column as usize / 2));

    let length_of_underline = (e.end_pos.char_column - e.start_pos.char_column).max(1) as usize;
    red_ln!("{}", "^".repeat(length_of_underline));
}

fn print_help(commands_only: bool) {
    if !commands_only {
        grey_ln!("------------------------------------");
        green_ln_bold!("The BS compiler!");
        println!("Usage: cargo run <command> <args>");
    }
    green_ln_bold!("Commands:");
    println!("  new <project name>   - Creates a new HTML project");
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
