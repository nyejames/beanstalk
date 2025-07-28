use crate::compiler::parsers::tokens::TextLocation;
use colour::{
    e_dark_magenta, e_dark_yellow_ln, e_magenta_ln, e_red_ln, e_yellow, e_yellow_ln, red_ln,
};
use std::path::PathBuf;
use std::{env, fs};

pub struct CompileError {
    pub msg: String,
    pub location: TextLocation,
    pub error_type: ErrorType,
    pub file_path: PathBuf,
}

impl CompileError {
    pub fn with_file_path(mut self, file_path: PathBuf) -> Self {
        self.file_path = file_path;

        self
    }
    pub fn new_rule_error(msg: String, location: TextLocation) -> Self {
        CompileError {
            msg,
            location,
            error_type: ErrorType::Rule,
            file_path: PathBuf::new(),
        }
    }
    pub fn new_type_error(msg: String, location: TextLocation) -> Self {
        CompileError {
            msg,
            location,
            error_type: ErrorType::Type,
            file_path: PathBuf::new(),
        }
    }
    pub fn new_syntax_error(msg: String, location: TextLocation) -> Self {
        CompileError {
            msg,
            location,
            error_type: ErrorType::Syntax,
            file_path: PathBuf::new(),
        }
    }

    pub fn new_thread_panic(msg: String) -> Self {
        CompileError {
            msg,
            location: TextLocation::default(),
            error_type: ErrorType::Compiler,
            file_path: PathBuf::new(),
        }
    }
}

// Adds more information to the CompileError
// So it knows the file path (possible specific part of the line soon)
// And the type of error
#[derive(PartialEq)]
pub enum ErrorType {
    Syntax,
    Type,
    Rule,
    File,
    Compiler,
    DevServer,
}

/// Returns a new CompileError.
///
/// Usage: `bail!(TextLocation, "message", message format args)`;
#[macro_export]
macro_rules! return_syntax_error {
    ($location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: format!($($msg)+),
            location: $location,
            error_type: ErrorType::Syntax,
            file_path: std::path::PathBuf::new(),
        })
    };
}

/// Returns a new CompileError.
///
/// Usage: `bail!(TextLocation, "message", message format args)`;
#[macro_export]
macro_rules! return_type_error {
    ($location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: format!($($msg)+),
            location: $location,
            error_type: ErrorType::Type,
            file_path: std::path::PathBuf::new(),
        })
    };
}

/// Returns a new CompileError.
///
/// Usage: `bail!(TextLocation, "message", message format args)`;
#[macro_export]
macro_rules! return_rule_error {
    ($location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: format!($($msg)+),
            location: $location,
            error_type: ErrorType::Rule,
            file_path: std::path::PathBuf::new(),
        })
    };
}

/// Returns a new CompileError INSIDE A VEC ALREADY.
///
/// Usage: `bail!(Path, "message", message format args)`;
#[macro_export]
macro_rules! return_file_errors {
    ($path:expr, $($msg:tt)+) => {
        return Err(vec![CompileError {
            msg: format!($($msg)+),
            location: TextLocation::default(),
            error_type: ErrorType::File,
            file_path: $path.to_owned(),
        }])
    };
}

/// Returns a new CompileError.
///
/// Usage: `bail!("message", message format args)`;
#[macro_export]
macro_rules! return_compiler_error {
    ($($msg:tt)+) => {
        return Err(CompileError {
            msg: format!($($msg)+),
            location: TextLocation::default(),
            error_type: ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
        })
    };
}

/// Returns a new CompileError.
/// INSIDE A VEC ALREADY.
///
/// Usage: `bail!(TextLocation, "message", message format args)`;
#[macro_export]
macro_rules! return_dev_server_error {
    ($path:expr, $($msg:tt)+) => {
        return Err(vec![CompileError {
            msg: format!($($msg)+),
            location: TextLocation::default(),
            error_type: ErrorType::DevServer,
            file_path: $path.to_owned(),
        }])
    };
}

#[macro_export]
macro_rules! return_err_with_added_msg {
    ($($extra_context:tt)+) => {
        .map_err(|e| {
            return Err(CompileError {
                msg: format!($($extra_context)+).push_str(&e.msg),
                location: e.location,
                error_type: $e.error_type,
                file_path: $e.file_path,
            })
        })
    };
}

/// Takes in an existing error and adds a path to it
#[macro_export]
macro_rules! return_err_with_path {
    ($err:expr, $path:expr) => {
        return Err($err.with_file_path($path))
    };
}

#[macro_export]
macro_rules! return_thread_err {
    ($process:expr) => {
        return Err(CompileError {
            msg: format!("Thread panicked during {}", $process),
            location: TextLocation::default(),
            error_type: ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
        })
    };
}

#[macro_export]
macro_rules! return_wat_err {
    ($err:expr) => {
        return Err(CompileError {
            msg: format!("Error while parsing WAT: {}", $err),
            location: TextLocation::default(),
            error_type: ErrorType::Syntax,
            file_path: std::path::PathBuf::new(),
        })
    };
}

pub fn print_errors(errors: Vec<CompileError>) {
    for e in errors {
        print_formatted_error(e);
    }
}

pub fn print_formatted_error(e: CompileError) {
    // Walk back through the file path until it's the current directory
    let relative_dir = match env::current_dir() {
        Ok(dir) => e
            .file_path
            .strip_prefix(dir)
            .unwrap_or(&e.file_path)
            .to_string_lossy(),
        Err(_) => e.file_path.to_string_lossy(),
    };

    let line_number = e.location.start_pos.line_number as usize;

    // Read the file and get the actual line as a string from the code
    let line = match fs::read_to_string(&e.file_path) {
        Ok(file) => file
            .lines()
            .nth(line_number)
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
        // ErrorType::Suggestion => {
        //     print!("\n( ͡° ͜ʖ ͡°) ");
        //     dark_red_ln!("{}", relative_dir);
        //     println!(" ( ._. ) ");
        //     e_dark_blue_ln!("Suggestion");
        //     e_dark_magenta!("Line ");
        //     e_magenta_ln!("{}\n", line_number + 1);
        // }
        //
        // ErrorType::Caution => {
        //     print!("\n(ಠ_ಠ)☞  ⚠ ");
        //     dark_red!("{}", relative_dir);
        //     println!("⚠  ☜(■_■¬ ) ");
        //
        //     e_yellow_ln!("Caution");
        //     e_dark_magenta!("Line ");
        //     e_magenta_ln!("{}\n", line_number + 1);
        // }
        ErrorType::Syntax => {
            eprint!("\n(╯°□°)╯  🔥🔥 ");
            e_dark_magenta!("{}", relative_dir);
            eprintln!(" 🔥🔥  Σ(°△°;) ");

            e_red_ln!("Syntax");
            e_dark_magenta!("Line ");
            e_magenta_ln!("{}\n", line_number + 1);
        }

        ErrorType::Type => {
            eprint!("\n(ಠ_ಠ) ");
            e_dark_magenta!("{}", relative_dir);
            eprintln!(" ( ._. ) ");

            e_red_ln!("Type Error");
            e_dark_magenta!("Line ");
            e_magenta_ln!("{}\n", line_number + 1);
        }

        ErrorType::Rule => {
            eprint!("\nヽ(˶°o°)ﾉ  🔥🔥🔥 ");
            e_dark_magenta!("{}", relative_dir);
            eprintln!(" 🔥🔥🔥  ╰(°□°╰) ");

            e_red_ln!("Rule");
            e_dark_magenta!("Line ");
            e_magenta_ln!("{}\n", line_number + 1);
        }

        ErrorType::File => {
            e_yellow_ln!("🏚 Can't find/read file or directory");
            e_red_ln!("  {}", e.msg);
            return;
        }

        ErrorType::Compiler => {
            eprint!("\nヽ༼☉ ‿ ⚆༽ﾉ  🔥🔥🔥🔥 ");
            e_dark_magenta!("{}", relative_dir);
            eprintln!(" 🔥🔥🔥🔥  ╰(° _ o╰) ");
            e_yellow!("COMPILER BUG - ");
            e_dark_yellow_ln!("compiler developer skill issue (not your fault)");
        }

        ErrorType::DevServer => {
            eprint!("\n(ﾉ☉_⚆)ﾉ  🔥 ");
            e_dark_magenta!("{}", relative_dir);
            eprintln!(" 🔥 ╰(° O °)╯ ");
            e_yellow_ln!("Dev Server whoopsie");
            e_red_ln!("  {}", e.msg);
            return;
        }
    }

    e_red_ln!("  {}", e.msg);

    println!("\n{line}");

    // spaces before the relevant part of the line
    print!(
        "{}",
        " ".repeat(e.location.start_pos.char_column as usize / 2)
    );

    let length_of_underline =
        (e.location.end_pos.char_column - e.location.start_pos.char_column).max(1) as usize;
    red_ln!("{}", "^".repeat(length_of_underline));
}
