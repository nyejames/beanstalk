use colour::{
    e_dark_magenta, e_dark_yellow_ln, e_magenta_ln, e_red_ln, e_yellow, e_yellow_ln, red_ln,
};
use std::fmt::Display;
use std::path::PathBuf;
use std::{env, fs};
use crate::compiler::compiler_warnings::CompilerWarning;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;

#[derive(Debug)]
pub struct CompilerMessages {
    pub errors: Vec<CompileError>,
    pub warnings: Vec<CompilerWarning>,
}

impl CompilerMessages {
    pub fn new() -> Self {
        CompilerMessages {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug)]
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

    /// Create a new rule error with descriptive message and suggestions
    pub fn new_rule_error(msg: String, location: TextLocation) -> Self {
        CompileError {
            msg,
            location,
            error_type: ErrorType::Rule,
            file_path: PathBuf::new(),
        }
    }

    /// Create a new type error with type information and suggestions
    pub fn new_type_error(msg: String, location: TextLocation) -> Self {
        CompileError {
            msg,
            location,
            error_type: ErrorType::Type,
            file_path: PathBuf::new(),
        }
    }

    /// Create a new syntax error with clear explanation
    pub fn new_syntax_error(msg: String, location: TextLocation) -> Self {
        CompileError {
            msg,
            location,
            error_type: ErrorType::Syntax,
            file_path: PathBuf::new(),
        }
    }

    /// Create a thread panic error (internal compiler issue)
    pub fn new_thread_panic(msg: String) -> Self {
        CompileError {
            msg: format!("COMPILER BUG - Thread panicked: {}", msg),
            location: TextLocation::default(),
            error_type: ErrorType::Compiler,
            file_path: PathBuf::new(),
        }
    }

    /// Create a compiler error (internal bug, not user's fault)
    pub fn compiler_error(msg: &str) -> Self {
        CompileError {
            msg: format!("COMPILER BUG - {}", msg),
            location: TextLocation::default(),
            error_type: ErrorType::Compiler,
            file_path: PathBuf::new(),
        }
    }

    /// Create a file system error
    pub fn file_error(path: &std::path::Path, msg: &str) -> Self {
        CompileError {
            msg: msg.to_string(),
            location: TextLocation::default(),
            error_type: ErrorType::File,
            file_path: path.to_path_buf(),
        }
    }

    /// Create a WASM validation error with mapping from wasmparser errors
    pub fn wasm_validation_error(
        wasm_error: &wasmparser::BinaryReaderError,
        source_location: Option<TextLocation>,
    ) -> Self {
        let location = source_location.unwrap_or_default();
        let msg = Self::map_wasm_error_to_message(wasm_error);
        let error_str = format!("{}", wasm_error);

        // Determine error type based on error content
        let error_type =
            if error_str.contains("type mismatch") || error_str.contains("TypeMismatch") {
                ErrorType::Type
            } else {
                ErrorType::Compiler
            };

        CompileError {
            msg,
            location,
            error_type,
            file_path: PathBuf::new(),
        }
    }

    /// Map WASM validation errors to helpful user messages
    fn map_wasm_error_to_message(wasm_error: &wasmparser::BinaryReaderError) -> String {
        let error_str = format!("{}", wasm_error);

        // Check error message content to determine type
        if error_str.contains("type mismatch") || error_str.contains("TypeMismatch") {
            format!(
                "WASM type validation failed: {}. This indicates a type mismatch in the generated code. Check function signatures and variable types.",
                wasm_error
            )
        } else if error_str.contains("function index") || error_str.contains("FunctionIndex") {
            format!(
                "WASM function index validation failed: {}. This is a compiler bug in function reference generation.",
                wasm_error
            )
        } else if error_str.contains("branch") || error_str.contains("BranchDepth") {
            format!(
                "WASM control flow validation failed: {}. This is a compiler bug in control flow generation.",
                wasm_error
            )
        } else {
            format!(
                "WASM module validation failed: {}. This indicates a bug in WASM generation.",
                wasm_error
            )
        }
    }

    /// Create an enhanced rule error with context and suggestions
    pub fn rule_error_with_suggestion(
        location: TextLocation,
        item_name: &str,
        error_type: &str,
        suggestion: &str,
    ) -> Self {
        let msg = format!(
            "{} '{}' {}. {}",
            error_type, item_name, "not found", suggestion
        );
        CompileError::new_rule_error(msg, location)
    }

    /// Create an enhanced type error with expected and actual types
    pub fn type_mismatch_error(
        location: TextLocation,
        expected: &str,
        found: &str,
        context: &str,
    ) -> Self {
        let msg = format!(
            "Type mismatch in {}: expected {}, found {}. Make sure the types match or use appropriate type conversion.",
            context, expected, found
        );
        CompileError::new_type_error(msg, location)
    }

    /// Create an unimplemented feature error with helpful context
    pub fn unimplemented_feature_error(
        feature_name: &str,
        location: Option<TextLocation>,
        workaround: Option<&str>,
    ) -> Self {
        let mut msg = format!(
            "{} not yet implemented in the Beanstalk compiler.",
            feature_name
        );

        if let Some(workaround_text) = workaround {
            msg.push_str(&format!(" Workaround: {}", workaround_text));
        }

        msg.push_str(" This feature is planned for a future release.");

        CompileError {
            msg,
            location: location.unwrap_or_default(),
            error_type: ErrorType::Compiler,
            file_path: PathBuf::new(),
        }
    }

    /// Validate error message quality
    pub fn validate_message_quality(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.msg.is_empty() {
            issues.push("Error message is empty".to_string());
        }

        if self.msg.len() < 10 {
            issues.push("Error message is too short to be helpful".to_string());
        }

        if self.msg.contains("panic") || self.msg.contains("unwrap") {
            issues.push("Error message contains internal implementation details".to_string());
        }

        // Check for helpful patterns
        let has_suggestion = self.msg.contains("Try")
            || self.msg.contains("Consider")
            || self.msg.contains("Make sure")
            || self.msg.contains("Did you mean");

        if matches!(
            self.error_type,
            ErrorType::Rule | ErrorType::Type | ErrorType::Syntax
        ) && !has_suggestion
        {
            issues.push("User-facing error should include suggestions or guidance".to_string());
        }

        issues
    }
}

// Adds more information to the CompileError
// So it knows the file path (possible specific part of the line soon)
// And the type of error
#[derive(PartialEq, Debug)]
pub enum ErrorType {
    Syntax,
    Type,
    Rule,
    File,
    Config,
    Compiler,
    DevServer,
    BorrowChecker,
    WirTransformation,
    WasmGeneration,
}

pub fn error_type_to_str(e_type: &ErrorType) -> &'static str {
    match e_type {
        ErrorType::Compiler => "Compiler Bug",
        ErrorType::Syntax => "Syntax Error",
        ErrorType::Config => "Malformed Config",
        ErrorType::File => "File Error",
        ErrorType::Rule => "Language Rule Violation",
        ErrorType::Type => "Type Error",
        ErrorType::DevServer => "Dev Server Issue",
        ErrorType::BorrowChecker => "Borrow Checker",
        ErrorType::WirTransformation => "WIR Transformation",
        ErrorType::WasmGeneration => "WASM Generation",
    }
}

/// Returns a new CompileError for syntax violations.
///
/// Syntax errors indicate malformed code that doesn't follow Beanstalk language rules.
/// These should include clear explanations and suggestions when possible.
///
/// Usage: `return_syntax_error!(location, "Expected ';' after statement, found '{}'", token)`;
#[macro_export]
macro_rules! return_syntax_error {
    ($location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: format!($($msg)+),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::Syntax,
            file_path: std::path::PathBuf::new(),
        })
    };
}

/// Returns a new CompileError for type system violations.
///
/// Type errors indicate mismatched types or invalid type operations.
/// Should mention both expected and actual types with suggestions.
///
/// Usage: `return_type_error!(location, "Cannot add {} and {}, both must be numeric", lhs_type, rhs_type)`;
#[macro_export]
macro_rules! return_type_error {
    ($location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: format!($($msg)+),
            location: $location,
            error_type: $crate::compiler::compiler_errors::ErrorType::Type,
            file_path: std::path::PathBuf::new(),
        })
    };
}

/// Returns a new CompileError for semantic rule violations.
///
/// Rule errors indicate violations of language semantics like undefined variables,
/// scope violations, or incorrect usage patterns. Should include specific names
/// and helpful suggestions.
///
/// Usage: `return_rule_error!(location, "Undefined variable '{}'. Did you mean '{}'?", name, suggestion)`;
#[macro_export]
macro_rules! return_rule_error {
    ($location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: format!($($msg)+),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::Rule,
            file_path: std::path::PathBuf::new(),
        })
    };
}
/// Returns a new CompileError
///
/// Usage: `bail!(Path, "message", message format args)`;
#[macro_export]
macro_rules! return_file_error {
    ($path:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: format!($($msg)+),
            location: crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
            error_type: crate::compiler::compiler_errors::ErrorType::File,
            file_path: $path.to_owned(),
        })
    };
}

/// Returns a new CompileError
///
/// Usage: `bail!(Path, "message", message format args)`;
#[macro_export]
macro_rules! return_config_error {
    ($location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: format!($($msg)+),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::Config,
            file_path: std::path::PathBuf::new(),
        })
    };
}

/// Returns a new CompileError for internal compiler bugs.
///
/// Compiler errors indicate bugs in the compiler itself, not user code issues.
/// These are automatically prefixed with "COMPILER BUG" and should include
/// context about what was being processed when the error occurred.
///
/// Usage: `return_compiler_error!("Feature '{}' not implemented at line {}", feature, line)`;
#[macro_export]
macro_rules! return_compiler_error {
    ($($msg:tt)+) => {
        return Err(CompileError {
            msg: format!("COMPILER BUG - {}", format!($($msg)+)),
            location: crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
        })
    };
}

/// Returns a new CompileError for development server issues.
/// INSIDE A VEC ALREADY.
///
/// Usage: `return_dev_server_error!(path, "Server failed to start: {}", reason)`;
#[macro_export]
macro_rules! return_dev_server_error {
    ($path:expr, $($msg:tt)+) => {
        return Err(CompilerMessages {
            errors: vec![CompileError {
                msg: format!($($msg)+),
                location: crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
                error_type: crate::compiler::compiler_errors::ErrorType::DevServer,
                file_path: $path.to_owned(),
            }],
            warnings: Vec::new()
        })
    };
}

/// Returns a new CompileError for undefined variables with suggestions.
///
/// Provides enhanced error messages for undefined variable access with
/// suggestions for similar variable names or common fixes.
///
/// Usage: `return_undefined_variable_error!(location, "my_var", vec!["my_variable", "my_val"])`;
#[macro_export]
macro_rules! return_undefined_variable_error {
    ($location:expr, $var_name:expr, $suggestions:expr) => {{
        let mut msg = format!(
            "Undefined variable '{}'. Variable must be declared before use.",
            $var_name
        );
        let suggestions: Vec<String> = $suggestions;
        if !suggestions.is_empty() {
            msg.push_str(&format!(
                " Did you mean one of: {}?",
                suggestions.join(", ")
            ));
        } else {
            msg.push_str(&format!(
                " Make sure '{}' is declared in this scope or a parent scope.",
                $var_name
            ));
        }
        return Err(CompileError {
            msg,
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::Rule,
            file_path: std::path::PathBuf::new(),
        });
    }};
}

/// Returns a new CompileError for undefined functions with suggestions.
///
/// Provides enhanced error messages for undefined function calls with
/// suggestions for similar function names or import hints.
///
/// Usage: `return_undefined_function_error!(location, "my_func", vec!["my_function"])`;
#[macro_export]
macro_rules! return_undefined_function_error {
    ($location:expr, $func_name:expr, $suggestions:expr) => {{
        let mut msg = format!(
            "Undefined function '{}'. Function must be declared before use.",
            $func_name
        );
        let suggestions: Vec<String> = $suggestions;
        if !suggestions.is_empty() {
            msg.push_str(&format!(
                " Did you mean one of: {}?",
                suggestions.join(", ")
            ));
        } else {
            msg.push_str(
                " Make sure the function is defined in this file or imported from another module.",
            );
        }
        return Err(CompileError {
            msg,
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::Rule,
            file_path: std::path::PathBuf::new(),
        });
    }};
}

/// Returns a new CompileError for type mismatches with detailed context.
///
/// Provides enhanced type error messages with expected vs actual types
/// and suggestions for fixing the mismatch.
///
/// Usage: `return_type_mismatch_error!(location, "Int", "String", "arithmetic operation")`;
#[macro_export]
macro_rules! return_type_mismatch_error {
    ($location:expr, $expected:expr, $found:expr, $context:expr) => {
        return Err(CompileError {
            msg: format!(
                "Type mismatch in {}: expected {}, found {}. Make sure the types match or use appropriate type conversion.",
                $context, $expected, $found
            ),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::Type,
            file_path: std::path::PathBuf::new(),
        })
    };
}

/// Returns a new CompileError for unimplemented features with context.
///
/// Provides helpful error messages for features not yet implemented,
/// including workarounds when available.
///
/// Usage: `return_unimplemented_feature_error!("Complex expressions", Some(location), Some("break into simpler parts"))`;
#[macro_export]
macro_rules! return_unimplemented_feature_error {
    ($feature:expr, $location:expr, $workaround:expr) => {{
        let mut msg = format!(
            "{} not yet implemented in the Beanstalk compiler.",
            $feature
        );
        if let Some(workaround_text) = $workaround {
            msg.push_str(&format!(" Workaround: {}", workaround_text));
        }
        msg.push_str(" This feature is planned for a future release.");

        return Err(CompileError {
            msg,
            location: $location.unwrap_or_default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
        });
    }};
}

/// Returns a new CompileError for WASM validation failures.
///
/// Maps WASM validation errors to appropriate compiler error types
/// with helpful context about what went wrong.
///
/// Usage: `return_wasm_validation_error!(wasm_error, Some(location))`;
#[macro_export]
macro_rules! return_wasm_validation_error {
    ($wasm_error:expr, $location:expr) => {
        return Err(CompileError::wasm_validation_error($wasm_error, $location))
    };
}

/// Returns a new CompileError for borrow checking violations.
///
/// Borrow checker errors indicate memory safety violations detected during
/// lifetime analysis. These should include clear explanations of the conflict
/// and suggestions for resolving it.
///
/// Usage: `return_borrow_checker_error!(location, "Cannot borrow '{}' as mutable because it is already borrowed", var_name)`;
#[macro_export]
macro_rules! return_borrow_checker_error {
    ($location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: format!($($msg)+),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::BorrowChecker,
            file_path: std::path::PathBuf::new(),
        })
    };
}

/// Returns a new CompileError for WIR transformation failures.
///
/// WIR transformation errors indicate failures during AST to WIR conversion.
/// These are typically compiler bugs where the WIR infrastructure is missing
/// or incomplete for a particular language feature.
///
/// Usage: `return_wir_transformation_error!(location, "Function '{}' transformation not yet implemented", func_name)`;
#[macro_export]
macro_rules! return_wir_transformation_error {
    ($location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: format!($($msg)+),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::WirTransformation,
            file_path: std::path::PathBuf::new(),
        })
    };
}

/// Returns a new CompileError for WASM generation failures.
///
/// WASM generation errors indicate failures during WIR to WASM codegen.
/// These are typically compiler bugs in the WASM lowering or module generation.
///
/// Usage: `return_wasm_generation_error!(location, "Failed to generate WASM export for function '{}'", func_name)`;
#[macro_export]
macro_rules! return_wasm_generation_error {
    ($location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: format!($($msg)+),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::WasmGeneration,
            file_path: std::path::PathBuf::new(),
        })
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
            location: crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
        })
    };
}

#[macro_export]
macro_rules! return_wat_err {
    ($err:expr) => {
        return Err(CompileError {
            msg: format!("Error while parsing WAT: {}", $err),
            location: crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Syntax,
            file_path: std::path::PathBuf::new(),
        })
    };
}

pub fn print_compiler_messages(messages: CompilerMessages) {

    // Format and print out the messages:
    for err in messages.errors {
        print_formatted_error(err);
    }

    // TODO
    // Format and print out the warnings:
    for warning in messages.warnings {

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
        Err(_) => {
            red_ln!("Compiler failed to find the file to give you the snippet. Another compiler developer skill issue.");
            e.file_path.to_string_lossy()
        },
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
            e_yellow_ln!("🏚 Can't find/read file or directory: {:?}", e.file_path);
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

        ErrorType::Config => {
            eprint!("\n (-_-)  🔥🔥🔥🔥 ");
            e_dark_magenta!("{}", relative_dir);
            eprintln!(" 🔥🔥🔥🔥  <(^~^)/ ");
            e_yellow!("CONFIG FILE ISSUE- ");
            e_dark_yellow_ln!(
                "Malformed config file, something doesn't make sense inside the project config)"
            );
        }

        ErrorType::DevServer => {
            eprint!("\n(ﾉ☉_⚆)ﾉ  🔥 ");
            e_dark_magenta!("{}", relative_dir);
            eprintln!(" 🔥 ╰(° O °)╯ ");
            e_yellow_ln!("Dev Server whoopsie");
            e_red_ln!("  {}", e.msg);
            return;
        }

        ErrorType::BorrowChecker => {
            eprint!("\n(╯°Д°)╯  🔥🔥 ");
            e_dark_magenta!("{}", relative_dir);
            eprintln!(" 🔥🔥  (╯°□°)╯ ");

            e_red_ln!("Borrow Checker");
            e_dark_magenta!("Line ");
            e_magenta_ln!("{}\n", line_number + 1);
        }

        ErrorType::WirTransformation => {
            eprint!("\nヽ༼☉ ‿ ⚆༽ﾉ  🔥🔥🔥 ");
            e_dark_magenta!("{}", relative_dir);
            eprintln!(" 🔥🔥🔥  ╰(° _ o╰) ");
            e_yellow!("WIR TRANSFORMATION BUG - ");
            e_dark_yellow_ln!("compiler developer skill issue (not your fault)");
        }

        ErrorType::WasmGeneration => {
            eprint!("\nヽ༼☉ ‿ ⚆༽ﾉ  🔥🔥🔥🔥 ");
            e_dark_magenta!("{}", relative_dir);
            eprintln!(" 🔥🔥🔥🔥  ╰(° _ o╰) ");
            e_yellow!("WASM GENERATION BUG - ");
            e_dark_yellow_ln!("compiler developer skill issue (not your fault)");
        }
    }

    e_red_ln!("  {}", e.msg);

    println!("\n{line}");

    // spaces before the relevant part of the line
    print!(
        "{}",
        " ".repeat((e.location.start_pos.char_column - 1).max(0) as usize)
    );

    let length_of_underline =
        (e.location.end_pos.char_column - e.location.start_pos.char_column + 1).max(1) as usize;
    red_ln!("{}", "^".repeat(length_of_underline));
}

