//! # Compiler Error Handling System
//!
//! This module provides a unified error handling system for the Beanstalk compiler.
//! All error types are consolidated here with structured metadata for LLM and LSP integration.
//!
//! ## Architecture
//!
//! The error system is built around three core types:
//! - [`CompileError`]: The unified error type with owned data and structured metadata
//! - [`ErrorLocation`]: Owned location information without string interning dependencies
//! - [`ErrorMetaDataKey`]: Structured metadata keys for intelligent error analysis
//!
//! ## Error Types
//!
//! The compiler uses different error types for different failure categories:
//! - **Syntax**: Malformed code that doesn't follow language syntax rules
//! - **Type**: Type system violations and mismatches
//! - **Rule**: Semantic errors like undefined variables or scope violations
//! - **BorrowChecker**: Memory safety violations detected during lifetime analysis
//! - **WirTransformation**: Failures during AST to WIR conversion (compiler bugs)
//! - **WasmGeneration**: Failures during WIR to WASM codegen (compiler bugs)
//! - **Compiler**: Internal compiler bugs (not user's fault)
//! - **File**: File system errors
//! - **Config**: Configuration file issues
//! - **DevServer**: Development server issues
//!
//! ## Error Creation Macros
//!
//! The module provides convenient macros for creating errors with consistent patterns:
//!
//! ### User-Facing Errors
//! - [`return_syntax_error!`]: For syntax violations in user code
//! - [`return_type_error!`]: For type system violations
//! - [`return_rule_error!`]: For semantic rule violations
//! - [`return_borrow_checker_error!`]: For memory safety violations
//!
//! ### Compiler Bug Errors
//! - [`return_compiler_error!`]: For internal compiler bugs
//! - [`return_wir_transformation_error!`]: For WIR transformation failures
//! - [`return_wasm_generation_error!`]: For WASM generation failures
//!
//! ### Specialized Borrow Checker Errors
//! - [`create_multiple_mutable_borrows_error!`]: Multiple mutable borrow conflicts
//! - [`create_shared_mutable_conflict_error!`]: Shared/mutable borrow conflicts
//! - [`create_use_after_move_error!`]: Use after move violations
//! - [`create_move_while_borrowed_error!`]: Move while borrowed violations
//!
//! ## Usage Examples
//!
//! ### Basic Error Creation
//! ```rust
//! // Syntax error with metadata
//! return_syntax_error!(
//!     "Expected ';' after statement",
//!     location,
//!     {
//!         CompilationStage => "Parsing",
//!         PrimarySuggestion => "Add a semicolon at the end of the statement"
//!     }
//! );
//!
//! // Simple error without metadata
//! return_type_error!("Cannot add Int and String", location);
//! ```
//!
//! ### Borrow Checker Errors
//! ```rust
//! // Multiple mutable borrows
//! let error = create_multiple_mutable_borrows_error!(
//!     place,
//!     existing_location,
//!     new_location
//! );
//! errors.push(error);
//!
//! // Or return immediately
//! return_multiple_mutable_borrows_error!(place, existing_loc, new_loc);
//! ```
//!
//! ### ErrorLocation Conversion
//! ```rust
//! // Convert from TextLocation (used in frontend)
//! let error_location = text_location.to_error_location(string_table);
//!
//! // Use in error creation
//! return_wir_transformation_error!(
//!     format!("Cannot transform expression type {:?}", expr_type),
//!     error_location,
//!     {
//!         CompilationStage => "WIR Transformation",
//!         PrimarySuggestion => "This is a compiler bug - please report it"
//!     }
//! );
//! ```
//!
//! ## Design Principles
//!
//! ### No StringTable Dependencies
//! All error types use owned `String` and `PathBuf` data, eliminating the need to pass
//! `StringTable` through the call stack. This simplifies error propagation and allows
//! errors to be created and returned without additional context.
//!
//! ### Structured Metadata
//! Errors include structured metadata via `ErrorMetaDataKey` for:
//! - LLM integration: Enables intelligent code suggestions and fixes
//! - LSP integration: Provides rich IDE diagnostics
//! - User experience: Offers helpful suggestions and context
//!
//! ### Consistent Patterns
//! All error macros follow consistent patterns:
//! - Simple form: `macro!(message, location)`
//! - Detailed form: `macro!(message, location, { metadata })`
//! - Returning vs non-returning variants for flexibility
//!
//! ## Error Flow Through Compilation Pipeline
//!
//! ```text
//! Source Code
//!     ↓
//! Tokenizer → Syntax Errors
//!     ↓
//! Parser → Syntax/Rule Errors
//!     ↓
//! AST Builder → Type/Rule Errors
//!     ↓
//! WIR Builder → WirTransformation Errors
//!     ↓
//! Borrow Checker → BorrowChecker Errors
//!     ↓
//! WASM Codegen → WasmGeneration Errors
//!     ↓
//! CompilerMessages (aggregated errors and warnings)
//! ```

use crate::compiler::compiler_warnings::{CompilerWarning, print_formatted_warning};
use crate::compiler::parsers::tokenizer::tokens::CharPosition;
use colour::{
    e_dark_magenta, e_dark_yellow_ln, e_magenta_ln, e_red_ln, e_yellow, e_yellow_ln, red_ln,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::{env, fs};

// The final set of errors and warnings emitted from the compiler
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

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub enum ErrorMetaDataKey {
    VariableName,
    CompilationStage,

    // Optional suggestions
    // Can be expanded to add more later
    PrimarySuggestion,     // One-line fix or top-level idea
    AlternativeSuggestion, // Secondary alternative
    SuggestedReplacement,  // Text that could replace the offending code
    SuggestedInsertion,    // Text that could be inserted
    SuggestedLocation,     // Relative descriptor: "before token X", "after semicolon"

    // Data type information
    ExpectedType,
    FoundType,
    InferredType,
    BorrowKind,          // "Shared" or "Mutable"
    LifetimeHint,        // For lifetime inference explanations
    MovedVariable,       // Variable name that was moved
    BorrowedVariable,    // Variable name that was borrowed
    ConflictingVariable, // Variable causing a borrow conflict
}

// A completely owned version of TextLocation
// Without interning to avoid having to pass the string table up with compiler messages
#[derive(Debug, Clone)]
pub struct ErrorLocation {
    pub scope: PathBuf,
    pub start_pos: CharPosition,
    pub end_pos: CharPosition,
}

impl ErrorLocation {
    pub fn new(path_buf: PathBuf, start: CharPosition, end: CharPosition) -> ErrorLocation {
        ErrorLocation {
            scope: path_buf,
            start_pos: start,
            end_pos: end,
        }
    }
    pub fn default() -> ErrorLocation {
        ErrorLocation {
            scope: PathBuf::new(),
            start_pos: CharPosition::default(),
            end_pos: CharPosition::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompileError {
    pub msg: String,

    // Includes the scope path, which will have the file name and header data.
    // This file path will need to be resolved to the actual file path when the error is displayed.
    // As this path will include the actual name of the header that the error came from.
    pub location: ErrorLocation,
    pub error_type: ErrorType,

    // This is for creating more structured and detailed error messages
    // Optimized for LLMs to understand exactly what went wrong
    pub metadata: HashMap<ErrorMetaDataKey, &'static str>,
}

impl CompileError {
    pub fn new(
        msg: impl Into<String>,
        location: ErrorLocation,
        error_type: ErrorType,
    ) -> CompileError {
        CompileError {
            msg: msg.into(),
            location,
            error_type,
            metadata: HashMap::new(),
        }
    }

    pub fn with_file_path(mut self, file_path: PathBuf) -> Self {
        self.location.scope = file_path;
        self
    }

    pub fn with_error_type(mut self, error_type: ErrorType) -> Self {
        self.error_type = error_type;
        self
    }

    pub fn new_metadata_entry(&mut self, key: ErrorMetaDataKey, value: &'static str) {
        self.metadata.insert(key, value);
    }

    /// Create a new syntax error with a clear explanation
    pub fn new_syntax_error(msg: impl Into<String>, location: ErrorLocation) -> Self {
        CompileError {
            msg: msg.into(),
            location,
            error_type: ErrorType::Syntax,
            metadata: HashMap::new(),
        }
    }

    /// Create a new rule error with a descriptive message (no metadata)
    pub fn new_rule_error(msg: impl Into<String>, location: ErrorLocation) -> Self {
        CompileError {
            msg: msg.into(),
            location,
            error_type: ErrorType::Rule,
            metadata: HashMap::new(),
        }
    }

    /// Create a new rule error with metadata
    pub fn new_rule_error_with_metadata(
        msg: impl Into<String>,
        location: ErrorLocation,
        metadata: HashMap<ErrorMetaDataKey, &'static str>,
    ) -> Self {
        CompileError {
            msg: msg.into(),
            location,
            error_type: ErrorType::Rule,
            metadata,
        }
    }

    /// Create a new type error with type information and suggestions
    pub fn new_type_error(msg: impl Into<String>, location: ErrorLocation) -> Self {
        CompileError {
            msg: msg.into(),
            location,
            error_type: ErrorType::Type,
            metadata: HashMap::new(),
        }
    }

    /// Create a thread panic error (internal compiler issue)
    pub fn new_thread_panic(msg: impl Into<String>) -> Self {
        CompileError {
            msg: msg.into(),
            location: ErrorLocation::default(),
            error_type: ErrorType::Compiler,
            metadata: HashMap::new(),
        }
    }

    /// Create a compiler error (internal bug, not user's fault)
    pub fn compiler_error(msg: impl Into<String>) -> Self {
        CompileError {
            msg: msg.into(),
            location: ErrorLocation::default(),
            error_type: ErrorType::Compiler,
            metadata: HashMap::new(),
        }
    }

    /// Create a file system error from a Path (legacy method without metadata)
    pub fn file_error(path: &std::path::Path, msg: impl Into<String>) -> Self {
        CompileError {
            msg: msg.into(),
            location: ErrorLocation::new(
                path.to_path_buf(),
                CharPosition::default(),
                CharPosition::default(),
            ),
            error_type: ErrorType::File,
            metadata: HashMap::new(),
        }
    }

    /// Create a file system error from Path with metadata
    pub fn new_file_error(
        path: &std::path::Path,
        msg: impl Into<String>,
        metadata: HashMap<ErrorMetaDataKey, &'static str>,
    ) -> Self {
        CompileError {
            msg: msg.into(),
            location: ErrorLocation::new(
                path.to_path_buf(),
                CharPosition::default(),
                CharPosition::default(),
            ),
            error_type: ErrorType::File,
            metadata,
        }
    }

    //     pub fn to_llm_friendly_json(&self) -> serde_json::Value {
    //         json!({
    //             "type": format!("{:?}", self.error_type),
    // x           "message": self.msg.to_string(),
    //             "file": self.location.scope.to_string(),
    //             "line": self.location.line,
    //             "column": self.location.column,
    //             "suggestions": self.suggestions.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    //             "metadata": self.metadata,
    //         })
    //     }
}

// Adds more information to the CompileError
// So it knows the file path (possible specific part of the line soon)
// And the type of error
#[derive(PartialEq, Debug, Clone)]
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
/// Syntax errors indicate malformed code that don't follow Beanstalk language rules.
/// These should include clear explanations and suggestions when possible.
///
/// Usage:
/// `return_syntax_error!("message", location, {
///     VariableName => "foo",
///     CompilationStage => "Parsing",
///     PrimarySuggestion => "Did you mean 'bar'?",
/// })`;
#[macro_export]
macro_rules! return_syntax_error {
    ($msg:expr, $loc:expr, { $( $key:ident => $value:expr ),* $(,)? }) => {
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $loc,
            error_type: $crate::compiler::compiler_errors::ErrorType::Syntax,
            metadata: {
                let map = std::collections::HashMap::new();
                $(
                    map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::$key, $value);
                )*
                map
            },
        })
    };
}

/// Returns a new CompileError for type system violations.
///
/// Type errors indicate mismatched types or invalid type operations.
/// Should mention both expected and actual types with suggestions.
///
/// Usage:
/// `return_type_error!("Cannot add x and y — both must be numeric", location, { ExpectedType => "Int", FoundType => "String" })`;
#[macro_export]
macro_rules! return_type_error {
    // New with metadata
    ($msg:expr, $location:expr, { $( $key:ident => $value:expr ),* $(,)? }) => {
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $location,
            error_type: $crate::compiler::compiler_errors::ErrorType::Type,
            metadata: {
                let mut map = std::collections::HashMap::new();
                $( map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::$key, $value); )*
                map
            },
        })
    };
    // New simple
    ($msg:expr, $location:expr) => {
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $location,
            error_type: $crate::compiler::compiler_errors::ErrorType::Type,
            metadata: std::collections::HashMap::new(),
        })
    };
}

/// Returns a new CompileError for semantic rule violations.
///
/// Rule errors indicate violations of language semantics like undefined variables,
/// scope violations, or incorrect usage patterns. Include specific names and
/// helpful suggestions when possible.
///
/// Usage examples:
/// `return_rule_error!("Undefined variable", location, { VariableName => "x" })`;
#[macro_export]
macro_rules! return_rule_error {
    // Arm with metadata map
    ($msg:expr, $location:expr, { $( $key:ident => $value:expr ),* $(,)? }) => {
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $location,
            error_type: $crate::compiler::compiler_errors::ErrorType::Rule,
            metadata: {
                let map = std::collections::HashMap::new();
                $( map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::$key, $value); )*
                map
            },
        })
    };
    // Arm without metadata
    ($msg:expr, $location:expr) => {
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $location,
            error_type: $crate::compiler::compiler_errors::ErrorType::Rule,
            metadata: std::collections::HashMap::new(),
        })
    };
}
/// Returns a new CompileError
///
/// Usage: `return_file_error!(path, "message", { metadata })`;
#[macro_export]
macro_rules! return_file_error {
    // New usage with metadata (Path)
    ($path:expr, $msg:expr, { $( $key:ident => $value:expr ),* $(,)? }) => {{
        return Err($crate::compiler::compiler_errors::CompileError::new_file_error(
            $path,
            $msg,
            {
                let mut map = std::collections::HashMap::new();
                $( map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::$key, $value); )*
                map
            },
        ));
    }};
    // Legacy usage without metadata (for backward compatibility)
    ($path:expr, $msg:expr) => {{
        return Err($crate::compiler::compiler_errors::CompileError::file_error(
            $path, $msg,
        ));
    }};
}

/// Returns a new CompileError
///
/// Usage: `return_config_error!(string_table, location, "message", message format args)`;
#[macro_export]
macro_rules! return_config_error {
    // New with metadata
    ($msg:expr, $location:expr, { $( $key:ident => $value:expr ),* $(,)? }) => {
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $location,
            error_type: $crate::compiler::compiler_errors::ErrorType::Config,
            metadata: {
                let mut map = std::collections::HashMap::new();
                $( map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::$key, $value); )*
                map
            },
        })
    };
    // New simple
    ($msg:expr, $location:expr) => {
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $location,
            error_type: $crate::compiler::compiler_errors::ErrorType::Config,
            metadata: std::collections::HashMap::new(),
        })
    };
}

/// Returns a new CompileError for internal compiler bugs.
///
/// Compiler errors indicate bugs in the compiler itself, not user code issues.
/// These provide the location of the bug in the compiler source code
#[macro_export]
macro_rules! return_compiler_error {
    // Variant with format string, arguments, and metadata (with semicolon separator)
    ($fmt:expr, $($arg:expr),+ ; { $( $key:ident => $value:expr ),* $(,)? }) => {{
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: format!($fmt, $($arg),+),
            location: $crate::compiler::compiler_errors::ErrorLocation::default(),
            error_type: $crate::compiler::compiler_errors::ErrorType::Compiler,
            metadata: {
                let mut map = std::collections::HashMap::new();
                $( map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::$key, $value); )*
                map
            },
        });
    }};
    // Variant with format string and arguments (no metadata)
    ($fmt:expr, $($arg:expr),+ $(,)?) => {{
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: format!($fmt, $($arg),+),
            location: $crate::compiler::compiler_errors::ErrorLocation::default(),
            error_type: $crate::compiler::compiler_errors::ErrorType::Compiler,
            metadata: std::collections::HashMap::new(),
        });
    }};
    // Variant with message and metadata (with semicolon separator)
    ($msg:expr ; { $( $key:ident => $value:expr ),* $(,)? }) => {{
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $crate::compiler::compiler_errors::ErrorLocation::default(),
            error_type: $crate::compiler::compiler_errors::ErrorType::Compiler,
            metadata: {
                let mut map = std::collections::HashMap::new();
                $( map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::$key, $value); )*
                map
            },
        });
    }};
    // Simple variant with just a message (no metadata)
    ($msg:expr) => {{
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $crate::compiler::compiler_errors::ErrorLocation::default(),
            error_type: $crate::compiler::compiler_errors::ErrorType::Compiler,
            metadata: std::collections::HashMap::new(),
        });
    }};
}

/// Returns a new CompileError for development server issues.
/// INSIDE A VEC ALREADY.
///
/// Usage: `return_dev_server_error!("message")` or `return_dev_server_error!(path, "message", args...)`;
#[macro_export]
macro_rules! return_dev_server_error {
    // With path, format string, and arguments
    ($path:expr, $fmt:expr, $($arg:expr),+) => {
        return Err($crate::compiler::compiler_errors::CompilerMessages {
            errors: vec![$crate::compiler::compiler_errors::CompileError::file_error(
                &$path,
                &format!($fmt, $($arg),+),
            ).with_error_type($crate::compiler::compiler_errors::ErrorType::DevServer)],
            warnings: Vec::new(),
        })
    };
    // With path and message (no format args)
    ($path:expr, $msg:expr) => {
        return Err($crate::compiler::compiler_errors::CompilerMessages {
            errors: vec![$crate::compiler::compiler_errors::CompileError::file_error(
                &$path,
                $msg,
            ).with_error_type($crate::compiler::compiler_errors::ErrorType::DevServer)],
            warnings: Vec::new(),
        })
    };
    // Message only (location defaults)
    ($msg:expr) => {
        return Err($crate::compiler::compiler_errors::CompilerMessages {
            errors: vec![$crate::compiler::compiler_errors::CompileError {
                msg: $msg.into(),
                location: $crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
                error_type: $crate::compiler::compiler_errors::ErrorType::DevServer,
                metadata: std::collections::HashMap::new(),
            }],
            warnings: Vec::new(),
        })
    };
}

/// Returns a new CompileError for borrow checking violations.
///
/// Borrow checker errors indicate memory safety violations detected during
/// lifetime analysis. These should include clear explanations of the conflict
/// and suggestions for resolving it.
///
/// Usage: `return_borrow_checker_error!("Cannot borrow '{}' as mutable because it is already borrowed", location, metadata)`;
#[macro_export]
macro_rules! return_borrow_checker_error {
    // New with metadata
    ($msg:expr, $location:expr, { $( $key:ident => $value:expr ),* $(,)? }) => {
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $location,
            error_type: $crate::compiler::compiler_errors::ErrorType::BorrowChecker,
            metadata: {
                let mut map = std::collections::HashMap::new();
                $( map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::$key, $value); )*
                map
            },
        })
    };
    // New simple
    ($msg:expr, $location:expr) => {
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $location,
            error_type: $crate::compiler::compiler_errors::ErrorType::BorrowChecker,
            metadata: std::collections::HashMap::new(),
        })
    };
}

/// Creates a CompileError for multiple mutable borrows (non-returning version).
///
/// This macro creates a detailed error when attempting to create a second mutable
/// borrow while a first one is still active. Returns the error object without returning from function.
///
/// Usage: `let error = create_multiple_mutable_borrows_error!(place, existing_location, new_location);`;
#[macro_export]
macro_rules! create_multiple_mutable_borrows_error {
    ($place:expr, $existing_location:expr, $new_location:expr) => {{
        let place_str: &'static str = Box::leak(format!("{:?}", $place).into_boxed_str());

        $crate::compiler::compiler_errors::CompileError {
            msg: format!(
                "cannot mutably borrow `{:?}` because it is already mutably borrowed",
                $place
            ),
            location: $new_location,
            error_type: $crate::compiler::compiler_errors::ErrorType::BorrowChecker,
            metadata: {
                let mut map = std::collections::HashMap::new();
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::VariableName,
                    place_str,
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::BorrowKind,
                    "Mutable",
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::ConflictingVariable,
                    place_str,
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::CompilationStage,
                    "Borrow Checking",
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
                    "Ensure the first mutable borrow is no longer used before creating the second",
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::LifetimeHint,
                    "Only one mutable borrow can exist at a time",
                );
                map
            },
        }
    }};
}

/// Creates a borrow checker error for multiple mutable borrows (returning version).
///
/// This macro creates a detailed error when attempting to create a second mutable
/// borrow while a first one is still active, and returns it immediately.
///
/// Usage: `return_multiple_mutable_borrows_error!(place, existing_location, new_location)`;
#[macro_export]
macro_rules! return_multiple_mutable_borrows_error {
    ($place:expr, $existing_location:expr, $new_location:expr) => {{
        return Err(create_multiple_mutable_borrows_error!(
            $place,
            $existing_location,
            $new_location
        ));
    }};
}

/// Creates a CompileError for shared/mutable borrow conflicts (non-returning version).
///
/// This macro creates a detailed error when attempting to create a borrow that
/// conflicts with an existing borrow (e.g., mutable when shared exists, or vice versa).
///
/// Usage: `let error = create_shared_mutable_conflict_error!(place, existing_kind, new_kind, existing_location, new_location);`;
#[macro_export]
macro_rules! create_shared_mutable_conflict_error {
    ($place:expr, $existing_kind:expr, $new_kind:expr, $existing_location:expr, $new_location:expr) => {{
        use $crate::compiler::wir::wir_nodes::BorrowKind;

        let place_str: &'static str = Box::leak(format!("{:?}", $place).into_boxed_str());
        let existing_kind_str: &'static str = match $existing_kind {
            BorrowKind::Shared => "Shared",
            BorrowKind::Mut => "Mutable",
        };
        let new_kind_str: &'static str = match $new_kind {
            BorrowKind::Shared => "Shared",
            BorrowKind::Mut => "Mutable",
        };

        let (message, suggestion, lifetime_hint) = match (&$existing_kind, &$new_kind) {
            (BorrowKind::Shared, BorrowKind::Mut) => (
                format!(
                    "cannot mutably borrow `{:?}` because it is already referenced",
                    $place
                ),
                "Ensure all shared references are finished before creating mutable access",
                "Mutable borrows require exclusive access - no other borrows can exist",
            ),
            (BorrowKind::Mut, BorrowKind::Shared) => (
                format!(
                    "cannot reference `{:?}` because it is already mutably borrowed",
                    $place
                ),
                "Finish using the mutable borrow before creating shared references",
                "Mutable borrows are exclusive - no other borrows can exist while active",
            ),
            _ => (
                format!("conflicting borrows of `{:?}`", $place),
                "Resolve the borrow conflict by restructuring your code",
                "Check the borrow rules for your specific case",
            ),
        };

        let existing_borrow_info: &'static str = Box::leak(
            format!(
                "Existing {} borrow conflicts with new {} borrow",
                existing_kind_str, new_kind_str
            )
            .into_boxed_str(),
        );

        $crate::compiler::compiler_errors::CompileError {
            msg: message,
            location: $new_location,
            error_type: $crate::compiler::compiler_errors::ErrorType::BorrowChecker,
            metadata: {
                let mut map = std::collections::HashMap::new();
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::VariableName,
                    place_str,
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::BorrowKind,
                    new_kind_str,
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::ConflictingVariable,
                    place_str,
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::CompilationStage,
                    "Borrow Checking",
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
                    suggestion,
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::LifetimeHint,
                    lifetime_hint,
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::AlternativeSuggestion,
                    existing_borrow_info,
                );
                map
            },
        }
    }};
}

/// Creates a borrow checker error for shared/mutable borrow conflicts (returning version).
///
/// Usage: `return_shared_mutable_conflict_error!(place, existing_kind, new_kind, existing_location, new_location)`;
#[macro_export]
macro_rules! return_shared_mutable_conflict_error {
    ($place:expr, $existing_kind:expr, $new_kind:expr, $existing_location:expr, $new_location:expr) => {{
        return Err(create_shared_mutable_conflict_error!(
            $place,
            $existing_kind,
            $new_kind,
            $existing_location,
            $new_location
        ));
    }};
}

/// Creates a CompileError for use after move (non-returning version).
///
/// This macro creates a detailed error when attempting to use a value that has
/// already been moved.
///
/// Usage: `let error = create_use_after_move_error!(place, move_location, use_location);`;
#[macro_export]
macro_rules! create_use_after_move_error {
    ($place:expr, $move_location:expr, $use_location:expr) => {{
        let place_str: &'static str = Box::leak(format!("{:?}", $place).into_boxed_str());

        $crate::compiler::compiler_errors::CompileError {
            msg: format!("borrow of moved value: `{:?}`", $place),
            location: $use_location,
            error_type: $crate::compiler::compiler_errors::ErrorType::BorrowChecker,
            metadata: {
                let mut map = std::collections::HashMap::new();
                map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::VariableName, place_str);
                map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::MovedVariable, place_str);
                map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::CompilationStage, "Borrow Checking");
                map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::PrimarySuggestion, "Consider using a reference instead of moving the value");
                map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::AlternativeSuggestion, "Clone the value before moving if you need to use it later");
                map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::LifetimeHint, "Once a value is moved, ownership transfers and the original variable can no longer be used");
                map
            },
        }
    }};
}

/// Creates a borrow checker error for use after move (returning version).
///
/// Usage: `return_use_after_move_error!(place, move_location, use_location)`;
#[macro_export]
macro_rules! return_use_after_move_error {
    ($place:expr, $move_location:expr, $use_location:expr) => {{
        return Err(create_use_after_move_error!(
            $place,
            $move_location,
            $use_location
        ));
    }};
}

/// Creates a CompileError for move while borrowed (non-returning version).
///
/// This macro creates a detailed error when attempting to move a value that has
/// active borrows.
///
/// Usage: `let error = create_move_while_borrowed_error!(place, borrow_kind, borrow_location, move_location);`;
#[macro_export]
macro_rules! create_move_while_borrowed_error {
    ($place:expr, $borrow_kind:expr, $borrow_location:expr, $move_location:expr) => {{
        use $crate::compiler::wir::wir_nodes::BorrowKind;

        let place_str: &'static str = Box::leak(format!("{:?}", $place).into_boxed_str());
        let borrow_kind_str: &'static str = match $borrow_kind {
            BorrowKind::Shared => "Shared",
            BorrowKind::Mut => "Mutable",
        };

        let borrow_type = match $borrow_kind {
            BorrowKind::Shared => "referenced",
            BorrowKind::Mut => "mutably borrowed",
        };

        $crate::compiler::compiler_errors::CompileError {
            msg: format!(
                "cannot move out of `{:?}` because it is {}",
                $place, borrow_type
            ),
            location: $move_location,
            error_type: $crate::compiler::compiler_errors::ErrorType::BorrowChecker,
            metadata: {
                let mut map = std::collections::HashMap::new();
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::VariableName,
                    place_str,
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::BorrowedVariable,
                    place_str,
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::BorrowKind,
                    borrow_kind_str,
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::CompilationStage,
                    "Borrow Checking",
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
                    "Ensure all borrows are finished before moving the value",
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::AlternativeSuggestion,
                    "Use references instead of moving the value",
                );
                map.insert(
                    $crate::compiler::compiler_errors::ErrorMetaDataKey::LifetimeHint,
                    "Cannot move a value while it has active borrows - the borrows must end first",
                );
                map
            },
        }
    }};
}

/// Creates a borrow checker error for move while borrowed (returning version).
///
/// Usage: `return_move_while_borrowed_error!(place, borrow_kind, borrow_location, move_location)`;
#[macro_export]
macro_rules! return_move_while_borrowed_error {
    ($place:expr, $borrow_kind:expr, $borrow_location:expr, $move_location:expr) => {{
        return Err(create_move_while_borrowed_error!(
            $place,
            $borrow_kind,
            $borrow_location,
            $move_location
        ));
    }};
}

/// Returns a new CompileError for WIR transformation failures.
///
/// WIR transformation errors indicate failures during AST to WIR conversion.
/// These are typically compiler bugs where the WIR infrastructure is missing
/// or incomplete for a particular language feature.
///
/// Usage: `return_wir_transformation_error!("Function '{}' transformation not yet implemented", func_name, location, {})`;
#[macro_export]
macro_rules! return_wir_transformation_error {
    // New arms
    ($msg:expr, $location:expr, { $( $key:ident => $value:expr ),* $(,)? }) => {
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $location,
            error_type: $crate::compiler::compiler_errors::ErrorType::WirTransformation,
            metadata: {
                let mut map = std::collections::HashMap::new();
                $( map.insert($crate::compiler::compiler_errors::ErrorMetaDataKey::$key, $value); )*
                map
            },
        })
    };
    ($msg:expr, $location:expr) => {
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $location,
            error_type: $crate::compiler::compiler_errors::ErrorType::WirTransformation,
            metadata: std::collections::HashMap::new(),
        })
    };
}

/// Returns a new CompileError for WASM generation failures.
///
/// WASM generation errors indicate failures during WIR to WASM codegen.
/// These are typically compiler bugs in the WASM lowering or module generation.
///
/// # Usage Examples
///
/// With metadata:
/// ```ignore
/// return_wasm_generation_error!(
///     format!("Failed to generate WASM export for function '{}'", func_name),
///     location,
///     {
///         CompilationStage => "WASM Generation",
///         PrimarySuggestion => "This is a compiler bug - please report it"
///     }
/// );
/// ```
///
/// Simple version without metadata:
/// ```ignore
/// return_wasm_generation_error!(
///     "WASM validation failed",
///     location
/// );
/// ```
#[macro_export]
macro_rules! return_wasm_generation_error {
    // With metadata
    ($msg:expr, $location:expr, { $( $key:ident => $value:expr ),* $(,)? }) => {
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $location,
            error_type: $crate::compiler::compiler_errors::ErrorType::WasmGeneration,
            metadata: {
                let mut map = std::collections::HashMap::new();
                $(
                    map.insert(
                        $crate::compiler::compiler_errors::ErrorMetaDataKey::$key,
                        $value
                    );
                )*
                map
            },
        })
    };
    // Simple version without metadata
    ($msg:expr, $location:expr) => {
        return Err($crate::compiler::compiler_errors::CompileError {
            msg: $msg.into(),
            location: $location,
            error_type: $crate::compiler::compiler_errors::ErrorType::WasmGeneration,
            metadata: std::collections::HashMap::new(),
        })
    };
}

#[macro_export]
macro_rules! return_thread_err {
    ($process:expr) => {
        return Err(CompileError {
            msg: &format!("Thread panicked during {}", $process),
            location: crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
            suggestions: Vec::new(),
        })
    };
}

#[macro_export]
macro_rules! return_wat_err {
    // New version with string table and metadata
    ($err:expr, { $( $key:ident => $value:expr ),* $(,)? }) => {
        return Err(CompileError {
            msg: &format!("Error while parsing WAT: {}", $err),
            location: crate::compiler::parsers::tokenizer::tokens::TextLocation::default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Syntax,
            metadata: {
                let mut map = std::collections::HashMap::new();
                $(
                    map.insert(
                        crate::compiler::compiler_errors::ErrorMetaDataKey::$key,
                        $value
                    );
                )*
                map
            },
        })
    };
}

pub fn print_compiler_messages(messages: CompilerMessages) {
    // Format and print out the messages:
    for err in messages.errors {
        print_formatted_error(err);
    }

    for warning in messages.warnings {
        print_formatted_warning(warning);
    }
}

pub fn print_formatted_error(e: CompileError) {
    // Walk back through the file path until it's the current directory
    let relative_dir = match env::current_dir() {
        Ok(dir) => {
            // Strip the actual header at the end of the path (.header extension)
            e.location
                .scope
                .strip_prefix(dir)
                .unwrap()
                .to_string_lossy()
                .to_string()
        }
        Err(err) => {
            red_ln!(
                "Compiler failed to find the file to give you the snippet. Another compiler developer skill issue. {}",
                err
            );
            e.location.scope.to_string_lossy().to_string()
        }
    };

    let line_number = e.location.start_pos.line_number as usize;

    // Read the file and get the actual line as a string from the code
    // Strip the actual header at the end of the path (.header extension)
    let mut actual_file = e.location.scope;
    let header = actual_file.file_name().unwrap().to_string_lossy();
    if actual_file.ends_with(".header") {
        actual_file = match actual_file.ancestors().nth(1) {
            Some(p) => p.to_path_buf(),
            None => actual_file,
        }
    }

    let line = match fs::read_to_string(&actual_file) {
        Ok(file) => file
            .lines()
            .nth(line_number)
            .unwrap_or_default()
            .to_string(),
        Err(_) => {
            red_ln!(
                "Compiler Skill Issue: Error with printing error. File path is invalid: {}",
                actual_file.display()
            );
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
            e_yellow_ln!("🏚 Can't find/read file or directory: {:?}", relative_dir);
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
