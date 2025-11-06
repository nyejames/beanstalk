use colour::{
    e_dark_magenta, e_dark_yellow_ln, e_magenta_ln, e_red_ln, e_yellow, e_yellow_ln, red_ln,
};
use std::path::PathBuf;
use std::{env, fs};
use crate::compiler::compiler_warnings::CompilerWarning;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::{InternedString, StringTable};

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

/// Pre-interned common error message templates for efficiency
#[derive(Debug, Clone)]
pub struct CommonErrorMessages {
    // Syntax error templates
    pub expected_semicolon: InternedString,
    pub expected_colon: InternedString,
    pub unexpected_token: InternedString,
    pub malformed_expression: InternedString,
    
    // Type error templates
    pub type_mismatch_template: InternedString,
    pub cannot_add_types: InternedString,
    pub invalid_operation: InternedString,
    
    // Rule error templates
    pub undefined_variable: InternedString,
    pub undefined_function: InternedString,
    pub variable_already_declared: InternedString,
    pub function_already_declared: InternedString,
    
    // Compiler bug templates
    pub unimplemented_feature: InternedString,
    pub internal_error: InternedString,
    pub wir_transformation_failed: InternedString,
    
    // File error templates
    pub file_not_found: InternedString,
    pub permission_denied: InternedString,
    pub io_error: InternedString,
    
    // Common suggestions
    pub check_spelling: InternedString,
    pub check_imports: InternedString,
    pub check_scope: InternedString,
    pub use_type_conversion: InternedString,
}

impl CommonErrorMessages {
    /// Initialize common error messages by pre-interning them in the string table
    pub fn new(string_table: &mut StringTable) -> Self {
        Self {
            // Syntax error templates
            expected_semicolon: string_table.intern("Expected ';' to close scope"),
            expected_colon: string_table.intern("Expected ':' to open scope"),
            unexpected_token: string_table.intern("Unexpected token"),
            malformed_expression: string_table.intern("Malformed expression"),
            
            // Type error templates
            type_mismatch_template: string_table.intern("Type mismatch: expected {}, found {}"),
            cannot_add_types: string_table.intern("Cannot add {} and {} - both operands must be the same numeric type"),
            invalid_operation: string_table.intern("Invalid operation {} on type {}"),
            
            // Rule error templates
            undefined_variable: string_table.intern("Undefined variable '{}'. Variable must be declared before use"),
            undefined_function: string_table.intern("Undefined function '{}'. Function must be declared before use"),
            variable_already_declared: string_table.intern("Variable '{}' is already declared in this scope"),
            function_already_declared: string_table.intern("Function '{}' is already declared in this scope"),
            
            // Compiler bug templates
            unimplemented_feature: string_table.intern("COMPILER BUG - {} not yet implemented"),
            internal_error: string_table.intern("COMPILER BUG - Internal error: {}"),
            wir_transformation_failed: string_table.intern("COMPILER BUG - WIR transformation failed: {}"),
            
            // File error templates
            file_not_found: string_table.intern("File not found: {}"),
            permission_denied: string_table.intern("Permission denied accessing file: {}"),
            io_error: string_table.intern("I/O error: {}"),
            
            // Common suggestions
            check_spelling: string_table.intern("Check the spelling and make sure it's declared in scope"),
            check_imports: string_table.intern("Make sure the function is defined in this file or imported from another module"),
            check_scope: string_table.intern("Make sure it's declared in this scope or a parent scope"),
            use_type_conversion: string_table.intern("Use appropriate type conversion or ensure both operands are the same type"),
        }
    }

    /// Create an error message using a template with parameter substitution
    pub fn format_template(&self, template: InternedString, args: &[&str], string_table: &mut StringTable) -> InternedString {
        let template_str = string_table.resolve(template);
        
        // Simple template substitution - replace {} with arguments in order
        let mut result = template_str.to_string();
        for arg in args {
            if let Some(pos) = result.find("{}") {
                result.replace_range(pos..pos+2, arg);
            }
        }
        
        string_table.intern(&result)
    }

    /// Get a pre-interned error message for undefined variable with suggestions
    pub fn undefined_variable_with_suggestions(
        &self, 
        var_name: &str, 
        suggestions: &[&str], 
        string_table: &mut StringTable
    ) -> InternedString {
        let mut msg = format!("Undefined variable '{}'. Variable must be declared before use.", var_name);
        
        if !suggestions.is_empty() {
            msg.push_str(&format!(" Did you mean one of: {}?", suggestions.join(", ")));
        } else {
            msg.push_str(&format!(" Make sure '{}' is declared in this scope or a parent scope.", var_name));
        }
        
        string_table.intern(&msg)
    }

    /// Get a pre-interned error message for undefined function with suggestions
    pub fn undefined_function_with_suggestions(
        &self, 
        func_name: &str, 
        suggestions: &[&str], 
        string_table: &mut StringTable
    ) -> InternedString {
        let mut msg = format!("Undefined function '{}'. Function must be declared before use.", func_name);
        
        if !suggestions.is_empty() {
            msg.push_str(&format!(" Did you mean one of: {}?", suggestions.join(", ")));
        } else {
            msg.push_str(" Make sure the function is defined in this file or imported from another module.");
        }
        
        string_table.intern(&msg)
    }

    /// Get a pre-interned type mismatch error message
    pub fn type_mismatch_error(
        &self, 
        expected: &str, 
        found: &str, 
        context: &str, 
        string_table: &mut StringTable
    ) -> InternedString {
        let msg = format!(
            "Type mismatch in {}: expected {}, found {}. Make sure the types match or use appropriate type conversion.",
            context, expected, found
        );
        string_table.intern(&msg)
    }
}

#[derive(Debug)]
pub struct CompileError {
    pub msg: InternedString,
    pub location: TextLocation,
    pub error_type: ErrorType,
    pub file_path: PathBuf,
    pub suggestions: Vec<InternedString>,
}

impl CompileError {
    pub fn with_file_path(mut self, file_path: PathBuf) -> Self {
        self.file_path = file_path;
        self
    }

    /// Create a new rule error with descriptive message and suggestions
    pub fn new_rule_error(msg: InternedString, location: TextLocation) -> Self {
        CompileError {
            msg,
            location,
            error_type: ErrorType::Rule,
            file_path: PathBuf::new(),
            suggestions: Vec::new(),
        }
    }

    /// Create a new rule error with suggestions
    pub fn new_rule_error_with_suggestions(
        msg: InternedString, 
        location: TextLocation, 
        suggestions: Vec<InternedString>
    ) -> Self {
        CompileError {
            msg,
            location,
            error_type: ErrorType::Rule,
            file_path: PathBuf::new(),
            suggestions,
        }
    }

    /// Create a new type error with type information and suggestions
    pub fn new_type_error(msg: InternedString, location: TextLocation) -> Self {
        CompileError {
            msg,
            location,
            error_type: ErrorType::Type,
            file_path: PathBuf::new(),
            suggestions: Vec::new(),
        }
    }

    /// Create a new type error with suggestions
    pub fn new_type_error_with_suggestions(
        msg: InternedString, 
        location: TextLocation, 
        suggestions: Vec<InternedString>
    ) -> Self {
        CompileError {
            msg,
            location,
            error_type: ErrorType::Type,
            file_path: PathBuf::new(),
            suggestions,
        }
    }

    /// Create a new syntax error with clear explanation
    pub fn new_syntax_error(msg: InternedString, location: TextLocation) -> Self {
        CompileError {
            msg,
            location,
            error_type: ErrorType::Syntax,
            file_path: PathBuf::new(),
            suggestions: Vec::new(),
        }
    }

    /// Create a new syntax error with suggestions
    pub fn new_syntax_error_with_suggestions(
        msg: InternedString, 
        location: TextLocation, 
        suggestions: Vec<InternedString>
    ) -> Self {
        CompileError {
            msg,
            location,
            error_type: ErrorType::Syntax,
            file_path: PathBuf::new(),
            suggestions,
        }
    }

    /// Create a thread panic error (internal compiler issue)
    pub fn new_thread_panic(msg: InternedString) -> Self {
        CompileError {
            msg,
            location: TextLocation::default(),
            error_type: ErrorType::Compiler,
            file_path: PathBuf::new(),
            suggestions: Vec::new(),
        }
    }

    /// Create a compiler error (internal bug, not user's fault)
    pub fn compiler_error(msg: InternedString) -> Self {
        CompileError {
            msg,
            location: TextLocation::default(),
            error_type: ErrorType::Compiler,
            file_path: PathBuf::new(),
            suggestions: Vec::new(),
        }
    }

    /// Create a file system error
    pub fn file_error(path: &std::path::Path, msg: InternedString) -> Self {
        CompileError {
            msg,
            location: TextLocation::default(),
            error_type: ErrorType::File,
            file_path: path.to_path_buf(),
            suggestions: Vec::new(),
        }
    }

    /// Create a WASM validation error with mapping from wasmparser errors
    pub fn wasm_validation_error(
        wasm_error: &wasmparser::BinaryReaderError,
        source_location: Option<TextLocation>,
        string_table: &mut StringTable,
    ) -> Self {
        let location = source_location.unwrap_or_default();
        let msg = Self::map_wasm_error_to_message(wasm_error, string_table);
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
            suggestions: Vec::new(),
        }
    }

    /// Map WASM validation errors to helpful user messages
    fn map_wasm_error_to_message(wasm_error: &wasmparser::BinaryReaderError, string_table: &mut StringTable) -> InternedString {
        let error_str = format!("{}", wasm_error);

        // Check error message content to determine type
        let message = if error_str.contains("type mismatch") || error_str.contains("TypeMismatch") {
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
        };

        string_table.intern(&message)
    }

    /// Create an enhanced rule error with context and suggestions
    pub fn rule_error_with_suggestion(
        location: TextLocation,
        item_name: &str,
        error_type: &str,
        suggestion: &str,
        string_table: &mut StringTable,
    ) -> Self {
        let msg = format!(
            "{} '{}' {}. {}",
            error_type, item_name, "not found", suggestion
        );
        let interned_msg = string_table.intern(&msg);
        CompileError::new_rule_error(interned_msg, location)
    }

    /// Create an enhanced type error with expected and actual types
    pub fn type_mismatch_error(
        location: TextLocation,
        expected: &str,
        found: &str,
        context: &str,
        string_table: &mut StringTable,
    ) -> Self {
        let msg = format!(
            "Type mismatch in {}: expected {}, found {}. Make sure the types match or use appropriate type conversion.",
            context, expected, found
        );
        let interned_msg = string_table.intern(&msg);
        CompileError::new_type_error(interned_msg, location)
    }

    /// Create an unimplemented feature error with helpful context
    pub fn unimplemented_feature_error(
        feature_name: &str,
        location: Option<TextLocation>,
        workaround: Option<&str>,
        string_table: &mut StringTable,
    ) -> Self {
        let mut msg = format!(
            "{} not yet implemented in the Beanstalk compiler.",
            feature_name
        );

        if let Some(workaround_text) = workaround {
            msg.push_str(&format!(" Workaround: {}", workaround_text));
        }

        msg.push_str(" This feature is planned for a future release.");

        let interned_msg = string_table.intern(&msg);

        CompileError {
            msg: interned_msg,
            location: location.unwrap_or_default(),
            error_type: ErrorType::Compiler,
            file_path: PathBuf::new(),
            suggestions: Vec::new(),
        }
    }

    /// Resolve the error message using the provided string table
    pub fn resolve_message<'a>(&self, string_table: &'a StringTable) -> &'a str {
        string_table.resolve(self.msg)
    }

    /// Resolve all suggestion messages using the provided string table
    pub fn resolve_suggestions<'a>(&self, string_table: &'a StringTable) -> Vec<&'a str> {
        self.suggestions.iter()
            .map(|&suggestion| string_table.resolve(suggestion))
            .collect()
    }

    /// Validate error message quality
    pub fn validate_message_quality(&self, string_table: &StringTable) -> Vec<String> {
        let mut issues = Vec::new();
        let msg = self.resolve_message(string_table);

        if msg.is_empty() {
            issues.push("Error message is empty".to_string());
        }

        if msg.len() < 10 {
            issues.push("Error message is too short to be helpful".to_string());
        }

        if msg.contains("panic") || msg.contains("unwrap") {
            issues.push("Error message contains internal implementation details".to_string());
        }

        // Check for helpful patterns
        let has_suggestion = msg.contains("Try")
            || msg.contains("Consider")
            || msg.contains("Make sure")
            || msg.contains("Did you mean")
            || !self.suggestions.is_empty();

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
/// Usage: `return_syntax_error!(string_table, location, "Expected ';' after statement, found '{}'", token)`;
#[macro_export]
macro_rules! return_syntax_error {
    ($string_table:expr, $location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: $string_table.intern(&format!($($msg)+)),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::Syntax,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        })
    };
}

/// Returns a new CompileError for type system violations.
///
/// Type errors indicate mismatched types or invalid type operations.
/// Should mention both expected and actual types with suggestions.
///
/// Usage: `return_type_error!(string_table, location, "Cannot add {} and {}, both must be numeric", lhs_type, rhs_type)`;
#[macro_export]
macro_rules! return_type_error {
    ($string_table:expr, $location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: $string_table.intern(&format!($($msg)+)),
            location: $location,
            error_type: $crate::compiler::compiler_errors::ErrorType::Type,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        })
    };
}

/// Returns a new CompileError for semantic rule violations.
///
/// Rule errors indicate violations of language semantics like undefined variables,
/// scope violations, or incorrect usage patterns. Should include specific names
/// and helpful suggestions.
///
/// Usage: `return_rule_error!(string_table, location, "Undefined variable '{}'. Did you mean '{}'?", name, suggestion)`;
#[macro_export]
macro_rules! return_rule_error {
    ($string_table:expr, $location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: $string_table.intern(&format!($($msg)+)),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::Rule,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        })
    };
}
/// Returns a new CompileError
///
/// Usage: `return_file_error!(string_table, path, "message", message format args)`;
#[macro_export]
macro_rules! return_file_error {
    ($string_table:expr, $path:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: $string_table.intern(&format!($($msg)+)),
            location: crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
            error_type: crate::compiler::compiler_errors::ErrorType::File,
            file_path: $path.to_owned(),
            suggestions: Vec::new(),
        })
    };
}

/// Returns a new CompileError
///
/// Usage: `return_config_error!(string_table, location, "message", message format args)`;
#[macro_export]
macro_rules! return_config_error {
    ($string_table:expr, $location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: $string_table.intern(&format!($($msg)+)),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::Config,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        })
    };
}

/// Returns a new CompileError for internal compiler bugs.
///
/// Compiler errors indicate bugs in the compiler itself, not user code issues.
/// These are automatically prefixed with "COMPILER BUG" and should include
/// context about what was being processed when the error occurred.
///
/// Usage: `return_compiler_error!(string_table, "Feature '{}' not implemented at line {}", feature, line)`;
/// Or legacy usage: `return_compiler_error!("Feature '{}' not implemented at line {}", feature, line)`;
#[macro_export]
macro_rules! return_compiler_error {
    // New version with string table
    ($string_table:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: $string_table.intern(&format!("COMPILER BUG - {}", format!($($msg)+))),
            location: crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        })
    };
    // Legacy version without string table (creates a temporary one)
    ($($msg:tt)+) => {{
        let mut temp_string_table = crate::compiler::string_interning::StringTable::new();
        return Err(CompileError {
            msg: temp_string_table.intern(&format!("COMPILER BUG - {}", format!($($msg)+))),
            location: crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        })
    }};
}

/// Returns a new CompileError for development server issues.
/// INSIDE A VEC ALREADY.
///
/// Usage: `return_dev_server_error!(string_table, path, "Server failed to start: {}", reason)`;
#[macro_export]
macro_rules! return_dev_server_error {
    ($string_table:expr, $path:expr, $($msg:tt)+) => {
        return Err(CompilerMessages {
            errors: vec![CompileError {
                msg: $string_table.intern(&format!($($msg)+)),
                location: crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
                error_type: crate::compiler::compiler_errors::ErrorType::DevServer,
                file_path: $path.to_owned(),
                suggestions: Vec::new(),
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
/// Usage: `return_undefined_variable_error!(string_table, location, "my_var", vec!["my_variable", "my_val"])`;
#[macro_export]
macro_rules! return_undefined_variable_error {
    ($string_table:expr, $location:expr, $var_name:expr, $suggestions:expr) => {{
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
            msg: $string_table.intern(&msg),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::Rule,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        });
    }};
}

/// Returns a new CompileError for undefined functions with suggestions.
///
/// Provides enhanced error messages for undefined function calls with
/// suggestions for similar function names or import hints.
///
/// Usage: `return_undefined_function_error!(string_table, location, "my_func", vec!["my_function"])`;
#[macro_export]
macro_rules! return_undefined_function_error {
    ($string_table:expr, $location:expr, $func_name:expr, $suggestions:expr) => {{
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
            msg: $string_table.intern(&msg),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::Rule,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        });
    }};
}

/// Returns a new CompileError for type mismatches with detailed context.
///
/// Provides enhanced type error messages with expected vs actual types
/// and suggestions for fixing the mismatch.
///
/// Usage: `return_type_mismatch_error!(string_table, location, "Int", "String", "arithmetic operation")`;
#[macro_export]
macro_rules! return_type_mismatch_error {
    ($string_table:expr, $location:expr, $expected:expr, $found:expr, $context:expr) => {
        return Err(CompileError {
            msg: $string_table.intern(&format!(
                "Type mismatch in {}: expected {}, found {}. Make sure the types match or use appropriate type conversion.",
                $context, $expected, $found
            )),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::Type,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        })
    };
}

/// Returns a new CompileError for unimplemented features with context.
///
/// Provides helpful error messages for features not yet implemented,
/// including workarounds when available.
///
/// Usage: `return_unimplemented_feature_error!(string_table, "Complex expressions", Some(location), Some("break into simpler parts"))`;
/// Or legacy usage: `return_unimplemented_feature_error!("Complex expressions", Some(location), Some("break into simpler parts"))`;
#[macro_export]
macro_rules! return_unimplemented_feature_error {
    // New version with string table
    ($string_table:expr, $feature:expr, $location:expr, $workaround:expr) => {{
        let mut msg = format!(
            "{} not yet implemented in the Beanstalk compiler.",
            $feature
        );
        if let Some(workaround_text) = $workaround {
            msg.push_str(&format!(" Workaround: {}", workaround_text));
        }
        msg.push_str(" This feature is planned for a future release.");

        return Err(CompileError {
            msg: $string_table.intern(&msg),
            location: $location.unwrap_or_default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        });
    }};
    // Legacy version without string table
    ($feature:expr, $location:expr, $workaround:expr) => {{
        let mut temp_string_table = crate::compiler::string_interning::StringTable::new();
        let mut msg = format!(
            "{} not yet implemented in the Beanstalk compiler.",
            $feature
        );
        if let Some(workaround_text) = $workaround {
            msg.push_str(&format!(" Workaround: {}", workaround_text));
        }
        msg.push_str(" This feature is planned for a future release.");

        return Err(CompileError {
            msg: temp_string_table.intern(&msg),
            location: $location.unwrap_or_default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        });
    }};
}

/// Returns a new CompileError for WASM validation failures.
///
/// Maps WASM validation errors to appropriate compiler error types
/// with helpful context about what went wrong.
///
/// Usage: `return_wasm_validation_error!(wasm_error, Some(location), string_table)`;
#[macro_export]
macro_rules! return_wasm_validation_error {
    ($wasm_error:expr, $location:expr, $string_table:expr) => {
        return Err(CompileError::wasm_validation_error($wasm_error, $location, $string_table))
    };
}

/// Returns a new CompileError for borrow checking violations.
///
/// Borrow checker errors indicate memory safety violations detected during
/// lifetime analysis. These should include clear explanations of the conflict
/// and suggestions for resolving it.
///
/// Usage: `return_borrow_checker_error!(string_table, location, "Cannot borrow '{}' as mutable because it is already borrowed", var_name)`;
#[macro_export]
macro_rules! return_borrow_checker_error {
    ($string_table:expr, $location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: $string_table.intern(&format!($($msg)+)),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::BorrowChecker,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        })
    };
}

/// Returns a new CompileError for WIR transformation failures.
///
/// WIR transformation errors indicate failures during AST to WIR conversion.
/// These are typically compiler bugs where the WIR infrastructure is missing
/// or incomplete for a particular language feature.
///
/// Usage: `return_wir_transformation_error!(string_table, location, "Function '{}' transformation not yet implemented", func_name)`;
#[macro_export]
macro_rules! return_wir_transformation_error {
    ($string_table:expr, $location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: $string_table.intern(&format!($($msg)+)),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::WirTransformation,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        })
    };
}

/// Returns a new CompileError for WASM generation failures.
///
/// WASM generation errors indicate failures during WIR to WASM codegen.
/// These are typically compiler bugs in the WASM lowering or module generation.
///
/// Usage: `return_wasm_generation_error!(string_table, location, "Failed to generate WASM export for function '{}'", func_name)`;
#[macro_export]
macro_rules! return_wasm_generation_error {
    ($string_table:expr, $location:expr, $($msg:tt)+) => {
        return Err(CompileError {
            msg: $string_table.intern(&format!($($msg)+)),
            location: $location,
            error_type: crate::compiler::compiler_errors::ErrorType::WasmGeneration,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        })
    };
}

#[macro_export]
macro_rules! return_err_with_added_msg {
    ($string_table:expr, $($extra_context:tt)+) => {
        .map_err(|e| {
            let combined_msg = format!("{}{}", format!($($extra_context)+), e.resolve_message($string_table));
            return Err(CompileError {
                msg: $string_table.intern(&combined_msg),
                location: e.location,
                error_type: e.error_type,
                file_path: e.file_path,
                suggestions: e.suggestions,
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
    ($string_table:expr, $process:expr) => {
        return Err(CompileError {
            msg: $string_table.intern(&format!("Thread panicked during {}", $process)),
            location: crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        })
    };
}

#[macro_export]
macro_rules! return_wat_err {
    // New version with string table
    ($string_table:expr, $err:expr) => {
        return Err(CompileError {
            msg: $string_table.intern(&format!("Error while parsing WAT: {}", $err)),
            location: crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Syntax,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        })
    };
    // Legacy version without string table
    ($err:expr) => {{
        let mut temp_string_table = crate::compiler::string_interning::StringTable::new();
        return Err(CompileError {
            msg: temp_string_table.intern(&format!("Error while parsing WAT: {}", $err)),
            location: crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Syntax,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        })
    }};
}

pub fn print_compiler_messages(messages: CompilerMessages, string_table: Option<&StringTable>) {

    // Format and print out the messages:
    for err in messages.errors {
        print_formatted_error(err, string_table);
    }

    // TODO
    // Format and print out the warnings:
    for _warning in messages.warnings {

    }

}

pub fn print_formatted_error(e: CompileError, string_table: Option<&StringTable>) {
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
            let msg = if let Some(st) = string_table {
                e.resolve_message(st)
            } else {
                // Fallback: create temporary string table to resolve message
                let temp_st = StringTable::new();
                temp_st.try_resolve(e.msg).unwrap_or("Error message unavailable")
            };
            e_red_ln!("  {}", msg);
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
            let msg = if let Some(st) = string_table {
                e.resolve_message(st)
            } else {
                let temp_st = StringTable::new();
                temp_st.try_resolve(e.msg).unwrap_or("Error message unavailable")
            };
            e_red_ln!("  {}", msg);
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

    let msg = if let Some(st) = string_table {
        e.resolve_message(st)
    } else {
        let temp_st = StringTable::new();
        temp_st.try_resolve(e.msg).unwrap_or("Error message unavailable")
    };
    e_red_ln!("  {}", msg);

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

/// Convenience macros for using pre-interned error messages

/// Returns a syntax error using pre-interned common messages when possible
#[macro_export]
macro_rules! return_common_syntax_error {
    ($compiler:expr, $location:expr, expected_semicolon) => {
        return Err(CompileError::new_syntax_error($compiler.common_error_messages().expected_semicolon, $location))
    };
    ($compiler:expr, $location:expr, expected_colon) => {
        return Err(CompileError::new_syntax_error($compiler.common_error_messages().expected_colon, $location))
    };
    ($compiler:expr, $location:expr, unexpected_token) => {
        return Err(CompileError::new_syntax_error($compiler.common_error_messages().unexpected_token, $location))
    };
    ($compiler:expr, $location:expr, malformed_expression) => {
        return Err(CompileError::new_syntax_error($compiler.common_error_messages().malformed_expression, $location))
    };
}

/// Returns a type error using pre-interned common messages
#[macro_export]
macro_rules! return_common_type_error {
    ($compiler:expr, $location:expr, type_mismatch, $expected:expr, $found:expr, $context:expr) => {
        return Err(CompileError::new_type_error(
            $compiler.common_error_messages().type_mismatch_error($expected, $found, $context, $compiler.string_table_mut()),
            $location
        ))
    };
    ($compiler:expr, $location:expr, cannot_add_types, $lhs:expr, $rhs:expr) => {
        return Err(CompileError::new_type_error(
            $compiler.common_error_messages().format_template(
                $compiler.common_error_messages().cannot_add_types,
                &[$lhs, $rhs],
                $compiler.string_table_mut()
            ),
            $location
        ))
    };
}

/// Returns a rule error using pre-interned common messages
#[macro_export]
macro_rules! return_common_rule_error {
    ($compiler:expr, $location:expr, undefined_variable, $var_name:expr, $suggestions:expr) => {
        return Err(CompileError::new_rule_error(
            $compiler.common_error_messages().undefined_variable_with_suggestions($var_name, $suggestions, $compiler.string_table_mut()),
            $location
        ))
    };
    ($compiler:expr, $location:expr, undefined_function, $func_name:expr, $suggestions:expr) => {
        return Err(CompileError::new_rule_error(
            $compiler.common_error_messages().undefined_function_with_suggestions($func_name, $suggestions, $compiler.string_table_mut()),
            $location
        ))
    };
}

/// Returns a compiler error using pre-interned common messages
#[macro_export]
macro_rules! return_common_compiler_error {
    ($compiler:expr, unimplemented_feature, $feature:expr) => {
        return Err(CompileError::compiler_error(
            $compiler.common_error_messages().format_template(
                $compiler.common_error_messages().unimplemented_feature,
                &[$feature],
                $compiler.string_table_mut()
            )
        ))
    };
    ($compiler:expr, internal_error, $details:expr) => {
        return Err(CompileError::compiler_error(
            $compiler.common_error_messages().format_template(
                $compiler.common_error_messages().internal_error,
                &[$details],
                $compiler.string_table_mut()
            )
        ))
    };
}

