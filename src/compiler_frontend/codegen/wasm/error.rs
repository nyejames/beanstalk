//! WASM Generation Error Types
//!
//! This module defines WASM-specific error types that integrate with Beanstalk's
//! unified error handling system. These errors provide detailed context about
//! WASM generation failures and map to the appropriate CompilerError types.
//!
//! ## Error Categories
//!
//! The error system is organized into several categories:
//!
//! ### LIR Analysis Errors
//! - Invalid LIR structure or malformed modules
//! - Type resolution failures
//! - Missing dependencies or undefined references
//!
//! ### Instruction Lowering Errors
//! - Unable to convert LIR instructions to WASM
//! - Stack discipline violations
//! - Invalid instruction sequences
//!
//! ### Validation Errors
//! - WASM module validation failures (via wasmparser)
//! - Type mismatches between expected and actual types
//! - Stack imbalances in generated code
//!
//! ### Memory Model Errors
//! - Layout calculation failures
//! - Alignment violations
//! - Tagged pointer operation failures
//!
//! ## Usage
//!
//! ```rust
//! use crate::compiler_frontend::codegen::wasm::error::WasmGenerationError;
//!
//! // Create a validation error
//! let error = WasmGenerationError::validation_failure(
//!     "type mismatch: expected i32, found i64",
//!     "function 'main' return statement",
//!     "Check that the return type matches the function signature"
//! );
//!
//! // Convert to CompilerError with location
//! let compiler_error = error.to_compiler_error(location);
//! ```

// Many error variants and helper methods are prepared for later implementation phases
// (ownership system, memory model, host function integration)
#![allow(dead_code)]

use crate::compiler_frontend::compiler_errors::{
    CompilerError, ErrorLocation, ErrorMetaDataKey, ErrorType,
};
use std::collections::HashMap;

/// WASM-specific error types that can occur during codegen.
///
/// Each variant provides detailed context about the failure, including:
/// - What operation was being performed
/// - What went wrong
/// - Suggestions for fixing the issue
#[derive(Debug, Clone)]
pub enum WasmGenerationError {
    /// LIR analysis failed - unable to extract required information.
    ///
    /// This error occurs during the analysis phase when the LIR module
    /// cannot be properly analyzed for type information, local mappings,
    /// or function signatures.
    LirAnalysis {
        /// Description of what went wrong
        context: String,
        /// The LIR construct that caused the failure
        lir_construct: String,
        /// Optional suggestion for fixing the issue
        suggestion: Option<String>,
    },

    /// Instruction lowering failed - cannot convert LIR instruction to WASM.
    ///
    /// This error occurs when an LIR instruction cannot be translated
    /// to equivalent WASM bytecode, often due to unsupported operations
    /// or invalid instruction sequences.
    InstructionLowering {
        /// The instruction that failed to lower
        instruction: String,
        /// Context about why the lowering failed
        context: String,
        /// Optional stack state information for debugging
        stack_state: Option<String>,
    },

    /// WASM module validation failed.
    ///
    /// This error occurs when the generated WASM module fails validation
    /// by wasmparser. It includes the original wasmparser error message
    /// along with LIR context to help identify the source of the problem.
    ValidationFailure {
        /// The wasmparser error message
        wasm_error: String,
        /// LIR context where the error originated
        lir_context: String,
        /// Suggestion for fixing the validation error
        suggestion: String,
    },

    /// Type mismatch error during WASM generation.
    ///
    /// This error occurs when there's a mismatch between expected and
    /// actual types, such as in function signatures, local variables,
    /// or stack operations.
    TypeMismatch {
        /// The expected type
        expected: String,
        /// The actual type found
        found: String,
        /// Context where the mismatch occurred
        context: String,
        /// Optional suggestion for fixing the mismatch
        suggestion: Option<String>,
    },

    /// Stack imbalance error during WASM generation.
    ///
    /// This error occurs when the operand stack is not properly balanced,
    /// such as when a function body leaves extra values on the stack or
    /// consumes more values than available.
    StackImbalance {
        /// Expected stack depth
        expected_depth: i32,
        /// Actual stack depth
        actual_depth: i32,
        /// The instruction sequence that caused the imbalance
        instruction_context: String,
        /// Suggestion for fixing the imbalance
        suggestion: String,
    },

    /// Memory layout calculation failed.
    ///
    /// This error occurs when calculating struct layouts, field offsets,
    /// or alignment requirements fails.
    MemoryLayout {
        /// Name of the struct being laid out
        struct_name: String,
        /// Information about the field that caused the issue
        field_info: String,
        /// Optional alignment-specific issue description
        alignment_issue: Option<String>,
    },

    /// Control flow generation failed.
    ///
    /// This error occurs when generating WASM control flow structures
    /// (blocks, loops, if/else) fails due to invalid nesting, branch
    /// targets, or block types.
    ControlFlow {
        /// Type of control flow block (block, loop, if)
        block_type: String,
        /// Current nesting depth
        nesting_depth: u32,
        /// Optional invalid branch target
        branch_target: Option<u32>,
        /// Additional context about the failure
        context: Option<String>,
    },

    /// Tagged pointer operation failed.
    ///
    /// This error occurs when Beanstalk's ownership bit manipulation
    /// operations fail, such as tagging, masking, or testing ownership.
    TaggedPointer {
        /// The operation that failed (tag, mask, test)
        operation: String,
        /// Context about why the operation failed
        context: String,
    },

    /// Function signature mismatch.
    ///
    /// This error occurs when a function's signature doesn't match
    /// between the LIR definition and the WASM generation, or when
    /// a call site doesn't match the callee's signature.
    SignatureMismatch {
        /// Expected signature
        expected: String,
        /// Found signature
        found: String,
        /// Name of the function with the mismatch
        function_name: String,
    },

    /// Index management error.
    ///
    /// This error occurs when an index (type, function, memory, global)
    /// is out of bounds or inconsistent across WASM sections.
    IndexError {
        /// The section where the index error occurred
        section: String,
        /// The invalid index
        index: u32,
        /// The maximum valid index
        max_index: u32,
    },

    /// Host function integration error.
    ///
    /// This error occurs when integrating host functions fails,
    /// such as when import declarations are invalid or type
    /// compatibility cannot be established.
    HostFunction {
        /// Name of the host function
        function_name: String,
        /// The module the function is imported from
        module_name: String,
        /// Description of the error
        context: String,
        /// Suggestion for fixing the issue
        suggestion: Option<String>,
    },

    /// Export handling error.
    ///
    /// This error occurs when export declarations are invalid,
    /// such as exporting a non-existent function or duplicate
    /// export names.
    ExportError {
        /// Name of the export
        export_name: String,
        /// Kind of export (function, memory, global)
        export_kind: String,
        /// Description of the error
        context: String,
    },

    /// Section ordering error.
    ///
    /// This error occurs when WASM sections are added in the wrong
    /// order, violating the WASM specification requirements.
    SectionOrdering {
        /// The section that was added out of order
        section: String,
        /// The section that should have come before
        expected_before: String,
        /// Suggestion for fixing the ordering
        suggestion: String,
    },
}

impl WasmGenerationError {
    /// Convert to CompilerError with appropriate metadata.
    ///
    /// This method transforms the WASM-specific error into a CompilerError
    /// that integrates with Beanstalk's unified error handling system.
    /// Each error variant produces a CompilerError with:
    /// - A descriptive message
    /// - The error location in source code
    /// - Structured metadata for LLM/LSP integration
    /// - Suggestions for fixing the issue
    pub fn to_compiler_error(self, location: ErrorLocation) -> CompilerError {
        match self {
            WasmGenerationError::LirAnalysis {
                context,
                lir_construct,
                suggestion,
            } => {
                let mut metadata = HashMap::new();
                metadata.insert(
                    ErrorMetaDataKey::CompilationStage,
                    "WASM Generation - LIR Analysis",
                );
                if let Some(suggestion) = suggestion {
                    let suggestion_str: &'static str = Box::leak(suggestion.into_boxed_str());
                    metadata.insert(ErrorMetaDataKey::PrimarySuggestion, suggestion_str);
                }

                CompilerError {
                    msg: format!("LIR analysis failed for {}: {}", lir_construct, context),
                    location,
                    error_type: ErrorType::WasmGeneration,
                    metadata,
                }
            }

            WasmGenerationError::InstructionLowering {
                instruction,
                context,
                stack_state,
            } => {
                let mut metadata = HashMap::new();
                metadata.insert(
                    ErrorMetaDataKey::CompilationStage,
                    "WASM Generation - Instruction Lowering",
                );
                metadata.insert(
                    ErrorMetaDataKey::PrimarySuggestion,
                    "Check LIR instruction format and WASM compatibility",
                );
                if let Some(stack_state) = stack_state {
                    let stack_str: &'static str = Box::leak(stack_state.into_boxed_str());
                    metadata.insert(ErrorMetaDataKey::AlternativeSuggestion, stack_str);
                }

                CompilerError {
                    msg: format!("Cannot lower instruction '{}': {}", instruction, context),
                    location,
                    error_type: ErrorType::WasmGeneration,
                    metadata,
                }
            }

            WasmGenerationError::ValidationFailure {
                wasm_error,
                lir_context,
                suggestion,
            } => {
                let mut metadata = HashMap::new();
                metadata.insert(
                    ErrorMetaDataKey::CompilationStage,
                    "WASM Generation - Validation",
                );
                let suggestion_str: &'static str = Box::leak(suggestion.into_boxed_str());
                metadata.insert(ErrorMetaDataKey::PrimarySuggestion, suggestion_str);
                let lir_context_str: &'static str = Box::leak(lir_context.clone().into_boxed_str());
                metadata.insert(ErrorMetaDataKey::AlternativeSuggestion, lir_context_str);

                CompilerError {
                    msg: format!("WASM validation failed: {}", wasm_error),
                    location,
                    error_type: ErrorType::WasmGeneration,
                    metadata,
                }
            }

            WasmGenerationError::TypeMismatch {
                expected,
                found,
                context,
                suggestion,
            } => {
                let mut metadata = HashMap::new();
                metadata.insert(
                    ErrorMetaDataKey::CompilationStage,
                    "WASM Generation - Type Checking",
                );
                let expected_str: &'static str = Box::leak(expected.clone().into_boxed_str());
                let found_str: &'static str = Box::leak(found.clone().into_boxed_str());
                metadata.insert(ErrorMetaDataKey::ExpectedType, expected_str);
                metadata.insert(ErrorMetaDataKey::FoundType, found_str);
                if let Some(suggestion) = suggestion {
                    let suggestion_str: &'static str = Box::leak(suggestion.into_boxed_str());
                    metadata.insert(ErrorMetaDataKey::PrimarySuggestion, suggestion_str);
                } else {
                    metadata.insert(
                        ErrorMetaDataKey::PrimarySuggestion,
                        "Ensure types match between LIR and WASM",
                    );
                }

                CompilerError {
                    msg: format!(
                        "Type mismatch in {}: expected {}, found {}",
                        context, expected, found
                    ),
                    location,
                    error_type: ErrorType::WasmGeneration,
                    metadata,
                }
            }

            WasmGenerationError::StackImbalance {
                expected_depth,
                actual_depth,
                instruction_context,
                suggestion,
            } => {
                let mut metadata = HashMap::new();
                metadata.insert(
                    ErrorMetaDataKey::CompilationStage,
                    "WASM Generation - Stack Validation",
                );
                let suggestion_str: &'static str = Box::leak(suggestion.into_boxed_str());
                metadata.insert(ErrorMetaDataKey::PrimarySuggestion, suggestion_str);
                let context_str: &'static str =
                    Box::leak(instruction_context.clone().into_boxed_str());
                metadata.insert(ErrorMetaDataKey::AlternativeSuggestion, context_str);

                CompilerError {
                    msg: format!(
                        "Stack imbalance: expected depth {}, found {} after {}",
                        expected_depth, actual_depth, instruction_context
                    ),
                    location,
                    error_type: ErrorType::WasmGeneration,
                    metadata,
                }
            }

            WasmGenerationError::MemoryLayout {
                struct_name,
                field_info,
                alignment_issue,
            } => {
                let mut metadata = HashMap::new();
                metadata.insert(
                    ErrorMetaDataKey::CompilationStage,
                    "WASM Generation - Memory Layout",
                );
                metadata.insert(
                    ErrorMetaDataKey::PrimarySuggestion,
                    "Check struct field types and alignment requirements",
                );
                if let Some(alignment_issue) = alignment_issue {
                    let alignment_str: &'static str = Box::leak(alignment_issue.into_boxed_str());
                    metadata.insert(ErrorMetaDataKey::AlternativeSuggestion, alignment_str);
                }

                CompilerError {
                    msg: format!(
                        "Memory layout calculation failed for struct '{}': {}",
                        struct_name, field_info
                    ),
                    location,
                    error_type: ErrorType::WasmGeneration,
                    metadata,
                }
            }

            WasmGenerationError::ControlFlow {
                block_type,
                nesting_depth,
                branch_target,
                context,
            } => {
                let mut metadata = HashMap::new();
                metadata.insert(
                    ErrorMetaDataKey::CompilationStage,
                    "WASM Generation - Control Flow",
                );
                metadata.insert(
                    ErrorMetaDataKey::PrimarySuggestion,
                    "Check block nesting and branch target validity",
                );
                if let Some(target) = branch_target {
                    let target_str: &'static str =
                        Box::leak(format!("Invalid branch target: {}", target).into_boxed_str());
                    metadata.insert(ErrorMetaDataKey::AlternativeSuggestion, target_str);
                }

                let msg = if let Some(ctx) = context {
                    format!(
                        "Control flow generation failed for {} at depth {}: {}",
                        block_type, nesting_depth, ctx
                    )
                } else {
                    format!(
                        "Control flow generation failed for {} at depth {}",
                        block_type, nesting_depth
                    )
                };

                CompilerError {
                    msg,
                    location,
                    error_type: ErrorType::WasmGeneration,
                    metadata,
                }
            }

            WasmGenerationError::TaggedPointer { operation, context } => {
                let mut metadata = HashMap::new();
                metadata.insert(
                    ErrorMetaDataKey::CompilationStage,
                    "WASM Generation - Tagged Pointers",
                );
                metadata.insert(
                    ErrorMetaDataKey::PrimarySuggestion,
                    "Check pointer alignment and ownership bit manipulation",
                );

                CompilerError {
                    msg: format!(
                        "Tagged pointer operation '{}' failed: {}",
                        operation, context
                    ),
                    location,
                    error_type: ErrorType::WasmGeneration,
                    metadata,
                }
            }

            WasmGenerationError::SignatureMismatch {
                expected,
                found,
                function_name,
            } => {
                let mut metadata = HashMap::new();
                metadata.insert(
                    ErrorMetaDataKey::CompilationStage,
                    "WASM Generation - Function Signatures",
                );
                let expected_str: &'static str = Box::leak(expected.clone().into_boxed_str());
                let found_str: &'static str = Box::leak(found.clone().into_boxed_str());
                metadata.insert(ErrorMetaDataKey::ExpectedType, expected_str);
                metadata.insert(ErrorMetaDataKey::FoundType, found_str);
                metadata.insert(
                    ErrorMetaDataKey::PrimarySuggestion,
                    "Check function signature compatibility between LIR and WASM",
                );

                CompilerError {
                    msg: format!(
                        "Function signature mismatch for '{}': expected {}, found {}",
                        function_name, expected, found
                    ),
                    location,
                    error_type: ErrorType::WasmGeneration,
                    metadata,
                }
            }

            WasmGenerationError::IndexError {
                section,
                index,
                max_index,
            } => {
                let mut metadata = HashMap::new();
                metadata.insert(
                    ErrorMetaDataKey::CompilationStage,
                    "WASM Generation - Index Management",
                );
                metadata.insert(
                    ErrorMetaDataKey::PrimarySuggestion,
                    "Check section index consistency and bounds",
                );

                CompilerError {
                    msg: format!(
                        "Index {} out of bounds for {} section (max: {})",
                        index, section, max_index
                    ),
                    location,
                    error_type: ErrorType::WasmGeneration,
                    metadata,
                }
            }

            WasmGenerationError::HostFunction {
                function_name,
                module_name,
                context,
                suggestion,
            } => {
                let mut metadata = HashMap::new();
                metadata.insert(
                    ErrorMetaDataKey::CompilationStage,
                    "WASM Generation - Host Functions",
                );
                if let Some(suggestion) = suggestion {
                    let suggestion_str: &'static str = Box::leak(suggestion.into_boxed_str());
                    metadata.insert(ErrorMetaDataKey::PrimarySuggestion, suggestion_str);
                } else {
                    metadata.insert(
                        ErrorMetaDataKey::PrimarySuggestion,
                        "Check host function import declaration and type compatibility",
                    );
                }

                CompilerError {
                    msg: format!(
                        "Host function error for '{}' from module '{}': {}",
                        function_name, module_name, context
                    ),
                    location,
                    error_type: ErrorType::WasmGeneration,
                    metadata,
                }
            }

            WasmGenerationError::ExportError {
                export_name,
                export_kind,
                context,
            } => {
                let mut metadata = HashMap::new();
                metadata.insert(
                    ErrorMetaDataKey::CompilationStage,
                    "WASM Generation - Exports",
                );
                metadata.insert(
                    ErrorMetaDataKey::PrimarySuggestion,
                    "Check export declaration and ensure the exported item exists",
                );

                CompilerError {
                    msg: format!(
                        "Export error for {} '{}': {}",
                        export_kind, export_name, context
                    ),
                    location,
                    error_type: ErrorType::WasmGeneration,
                    metadata,
                }
            }

            WasmGenerationError::SectionOrdering {
                section,
                expected_before,
                suggestion,
            } => {
                let mut metadata = HashMap::new();
                metadata.insert(
                    ErrorMetaDataKey::CompilationStage,
                    "WASM Generation - Section Ordering",
                );
                let suggestion_str: &'static str = Box::leak(suggestion.into_boxed_str());
                metadata.insert(ErrorMetaDataKey::PrimarySuggestion, suggestion_str);

                CompilerError {
                    msg: format!(
                        "Section '{}' added out of order (should come after '{}')",
                        section, expected_before
                    ),
                    location,
                    error_type: ErrorType::WasmGeneration,
                    metadata,
                }
            }
        }
    }

    // =========================================================================
    // Factory Methods for Creating Errors
    // =========================================================================

    /// Create a LIR analysis error.
    pub fn lir_analysis(context: impl Into<String>, lir_construct: impl Into<String>) -> Self {
        WasmGenerationError::LirAnalysis {
            context: context.into(),
            lir_construct: lir_construct.into(),
            suggestion: None,
        }
    }

    /// Create a LIR analysis error with suggestion.
    pub fn lir_analysis_with_suggestion(
        context: impl Into<String>,
        lir_construct: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        WasmGenerationError::LirAnalysis {
            context: context.into(),
            lir_construct: lir_construct.into(),
            suggestion: Some(suggestion.into()),
        }
    }

    /// Create an instruction lowering error.
    pub fn instruction_lowering(
        instruction: impl Into<String>,
        context: impl Into<String>,
    ) -> Self {
        WasmGenerationError::InstructionLowering {
            instruction: instruction.into(),
            context: context.into(),
            stack_state: None,
        }
    }

    /// Create an instruction lowering error with stack state.
    pub fn instruction_lowering_with_stack(
        instruction: impl Into<String>,
        context: impl Into<String>,
        stack_state: impl Into<String>,
    ) -> Self {
        WasmGenerationError::InstructionLowering {
            instruction: instruction.into(),
            context: context.into(),
            stack_state: Some(stack_state.into()),
        }
    }

    /// Create a validation failure error.
    pub fn validation_failure(
        wasm_error: impl Into<String>,
        lir_context: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        WasmGenerationError::ValidationFailure {
            wasm_error: wasm_error.into(),
            lir_context: lir_context.into(),
            suggestion: suggestion.into(),
        }
    }

    /// Create a type mismatch error.
    pub fn type_mismatch(
        expected: impl Into<String>,
        found: impl Into<String>,
        context: impl Into<String>,
    ) -> Self {
        WasmGenerationError::TypeMismatch {
            expected: expected.into(),
            found: found.into(),
            context: context.into(),
            suggestion: None,
        }
    }

    /// Create a type mismatch error with suggestion.
    pub fn type_mismatch_with_suggestion(
        expected: impl Into<String>,
        found: impl Into<String>,
        context: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        WasmGenerationError::TypeMismatch {
            expected: expected.into(),
            found: found.into(),
            context: context.into(),
            suggestion: Some(suggestion.into()),
        }
    }

    /// Create a stack imbalance error.
    pub fn stack_imbalance(
        expected_depth: i32,
        actual_depth: i32,
        instruction_context: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        WasmGenerationError::StackImbalance {
            expected_depth,
            actual_depth,
            instruction_context: instruction_context.into(),
            suggestion: suggestion.into(),
        }
    }

    /// Create a tagged pointer error.
    pub fn tagged_pointer(operation: impl Into<String>, context: impl Into<String>) -> Self {
        WasmGenerationError::TaggedPointer {
            operation: operation.into(),
            context: context.into(),
        }
    }

    /// Create a memory layout error.
    pub fn memory_layout(
        struct_name: impl Into<String>,
        field_info: impl Into<String>,
        alignment_issue: Option<String>,
    ) -> Self {
        WasmGenerationError::MemoryLayout {
            struct_name: struct_name.into(),
            field_info: field_info.into(),
            alignment_issue,
        }
    }

    /// Create a control flow error.
    pub fn control_flow(
        block_type: impl Into<String>,
        nesting_depth: u32,
        branch_target: Option<u32>,
        context: Option<String>,
    ) -> Self {
        WasmGenerationError::ControlFlow {
            block_type: block_type.into(),
            nesting_depth,
            branch_target,
            context,
        }
    }

    /// Create a function signature mismatch error.
    pub fn signature_mismatch(
        expected: impl Into<String>,
        found: impl Into<String>,
        function_name: impl Into<String>,
    ) -> Self {
        WasmGenerationError::SignatureMismatch {
            expected: expected.into(),
            found: found.into(),
            function_name: function_name.into(),
        }
    }

    /// Create an index error.
    pub fn index_error(section: impl Into<String>, index: u32, max_index: u32) -> Self {
        WasmGenerationError::IndexError {
            section: section.into(),
            index,
            max_index,
        }
    }

    /// Create a host function error.
    pub fn host_function(
        function_name: impl Into<String>,
        module_name: impl Into<String>,
        context: impl Into<String>,
    ) -> Self {
        WasmGenerationError::HostFunction {
            function_name: function_name.into(),
            module_name: module_name.into(),
            context: context.into(),
            suggestion: None,
        }
    }

    /// Create a host function error with suggestion.
    pub fn host_function_with_suggestion(
        function_name: impl Into<String>,
        module_name: impl Into<String>,
        context: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        WasmGenerationError::HostFunction {
            function_name: function_name.into(),
            module_name: module_name.into(),
            context: context.into(),
            suggestion: Some(suggestion.into()),
        }
    }

    /// Create an export error.
    pub fn export_error(
        export_name: impl Into<String>,
        export_kind: impl Into<String>,
        context: impl Into<String>,
    ) -> Self {
        WasmGenerationError::ExportError {
            export_name: export_name.into(),
            export_kind: export_kind.into(),
            context: context.into(),
        }
    }

    /// Create a section ordering error.
    pub fn section_ordering(
        section: impl Into<String>,
        expected_before: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        WasmGenerationError::SectionOrdering {
            section: section.into(),
            expected_before: expected_before.into(),
            suggestion: suggestion.into(),
        }
    }

    // =========================================================================
    // Wasmparser Integration Methods
    // =========================================================================

    /// Create a validation error from a wasmparser BinaryReaderError.
    ///
    /// This method extracts useful information from the wasmparser error
    /// and creates a WasmGenerationError with appropriate context and suggestions.
    pub fn from_wasmparser_error(
        error: &wasmparser::BinaryReaderError,
        lir_context: impl Into<String>,
    ) -> Self {
        let error_msg = error.message();
        let suggestion = Self::suggest_fix_for_wasmparser_error(error_msg);

        WasmGenerationError::ValidationFailure {
            wasm_error: error_msg.to_string(),
            lir_context: lir_context.into(),
            suggestion,
        }
    }

    /// Generate a suggestion based on common wasmparser error patterns.
    fn suggest_fix_for_wasmparser_error(error_msg: &str) -> String {
        // Type mismatch errors
        if error_msg.contains("type mismatch") {
            return "Check that all operations use compatible types. Ensure function return types match their signatures.".to_string();
        }

        // Stack underflow errors
        if error_msg.contains("stack underflow") || error_msg.contains("operand stack") {
            return "Check that all instructions have the required operands on the stack before execution.".to_string();
        }

        // Invalid function index
        if error_msg.contains("function index") || error_msg.contains("unknown function") {
            return "Check that all function calls reference valid function indices. Ensure imports are declared before use.".to_string();
        }

        // Invalid type index
        if error_msg.contains("type index") || error_msg.contains("unknown type") {
            return "Check that all type references are valid. Ensure types are declared in the type section.".to_string();
        }

        // Invalid local index
        if error_msg.contains("local index") || error_msg.contains("unknown local") {
            return "Check that all local variable accesses use valid indices. Ensure locals are declared in the function.".to_string();
        }

        // Invalid global index
        if error_msg.contains("global index") || error_msg.contains("unknown global") {
            return "Check that all global variable accesses use valid indices. Ensure globals are declared or imported.".to_string();
        }

        // Invalid memory index
        if error_msg.contains("memory index") || error_msg.contains("unknown memory") {
            return "Check that memory operations reference valid memory indices. Ensure memory is declared or imported.".to_string();
        }

        // Block/branch errors
        if error_msg.contains("branch") || error_msg.contains("label") {
            return "Check that all branch instructions target valid labels within the current block scope.".to_string();
        }

        // Section ordering errors
        if error_msg.contains("section") && error_msg.contains("order") {
            return "Check that WASM sections are added in the correct order: Type, Import, Function, Table, Memory, Global, Export, Start, Element, Code, Data.".to_string();
        }

        // Default suggestion
        "Check WASM generation logic and ensure all indices and types are consistent.".to_string()
    }
}

/// Convenience macro for returning WASM generation errors.
///
/// Usage:
/// ```rust
/// return_wasm_error!(error, location);
/// ```
#[macro_export]
macro_rules! return_wasm_error {
    ($error:expr, $location:expr) => {
        return Err($error.to_compiler_error($location))
    };
}

/// Convenience macro for creating and returning WASM generation errors.
///
/// This macro provides shorthand for common error patterns:
///
/// ```rust
/// // LIR analysis error
/// wasm_error!(lir_analysis: "context", "construct", location);
///
/// // Instruction lowering error
/// wasm_error!(instruction_lowering: "instruction", "context", location);
///
/// // Validation error
/// wasm_error!(validation: "wasm_error", "lir_context", "suggestion", location);
///
/// // Type mismatch error
/// wasm_error!(type_mismatch: "expected", "found", "context", location);
///
/// // Stack imbalance error
/// wasm_error!(stack_imbalance: expected_depth, actual_depth, "context", "suggestion", location);
///
/// // Tagged pointer error
/// wasm_error!(tagged_pointer: "operation", "context", location);
/// ```
#[macro_export]
macro_rules! wasm_error {
    (lir_analysis: $context:expr, $construct:expr, $location:expr) => {
        return Err($crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::lir_analysis($context, $construct).to_compiler_error($location))
    };
    (lir_analysis_with_suggestion: $context:expr, $construct:expr, $suggestion:expr, $location:expr) => {
        return Err($crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::lir_analysis_with_suggestion($context, $construct, $suggestion).to_compiler_error($location))
    };
    (instruction_lowering: $instruction:expr, $context:expr, $location:expr) => {
        return Err($crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::instruction_lowering($instruction, $context).to_compiler_error($location))
    };
    (instruction_lowering_with_stack: $instruction:expr, $context:expr, $stack:expr, $location:expr) => {
        return Err($crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::instruction_lowering_with_stack($instruction, $context, $stack).to_compiler_error($location))
    };
    (validation: $wasm_error:expr, $lir_context:expr, $suggestion:expr, $location:expr) => {
        return Err($crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::validation_failure($wasm_error, $lir_context, $suggestion).to_compiler_error($location))
    };
    (type_mismatch: $expected:expr, $found:expr, $context:expr, $location:expr) => {
        return Err($crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::type_mismatch($expected, $found, $context).to_compiler_error($location))
    };
    (type_mismatch_with_suggestion: $expected:expr, $found:expr, $context:expr, $suggestion:expr, $location:expr) => {
        return Err($crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::type_mismatch_with_suggestion($expected, $found, $context, $suggestion).to_compiler_error($location))
    };
    (stack_imbalance: $expected:expr, $actual:expr, $context:expr, $suggestion:expr, $location:expr) => {
        return Err($crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::stack_imbalance($expected, $actual, $context, $suggestion).to_compiler_error($location))
    };
    (tagged_pointer: $operation:expr, $context:expr, $location:expr) => {
        return Err($crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::tagged_pointer($operation, $context).to_compiler_error($location))
    };
    (control_flow: $block_type:expr, $depth:expr, $target:expr, $context:expr, $location:expr) => {
        return Err($crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::control_flow($block_type, $depth, $target, $context).to_compiler_error($location))
    };
    (signature_mismatch: $expected:expr, $found:expr, $func_name:expr, $location:expr) => {
        return Err($crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::signature_mismatch($expected, $found, $func_name).to_compiler_error($location))
    };
    (index_error: $section:expr, $index:expr, $max:expr, $location:expr) => {
        return Err($crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::index_error($section, $index, $max).to_compiler_error($location))
    };
    (host_function: $func_name:expr, $module:expr, $context:expr, $location:expr) => {
        return Err($crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::host_function($func_name, $module, $context).to_compiler_error($location))
    };
    (export_error: $name:expr, $kind:expr, $context:expr, $location:expr) => {
        return Err($crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::export_error($name, $kind, $context).to_compiler_error($location))
    };
}

/// Macro for creating WASM errors without returning (for collecting multiple errors).
///
/// Usage:
/// ```rust
/// let error = create_wasm_error!(type_mismatch: "i32", "i64", "return statement", location);
/// errors.push(error);
/// ```
#[macro_export]
macro_rules! create_wasm_error {
    (lir_analysis: $context:expr, $construct:expr, $location:expr) => {
        $crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::lir_analysis(
            $context, $construct,
        )
        .to_compiler_error($location)
    };
    (instruction_lowering: $instruction:expr, $context:expr, $location:expr) => {
        $crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::instruction_lowering(
            $instruction,
            $context,
        )
        .to_compiler_error($location)
    };
    (validation: $wasm_error:expr, $lir_context:expr, $suggestion:expr, $location:expr) => {
        $crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::validation_failure(
            $wasm_error,
            $lir_context,
            $suggestion,
        )
        .to_compiler_error($location)
    };
    (type_mismatch: $expected:expr, $found:expr, $context:expr, $location:expr) => {
        $crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::type_mismatch(
            $expected, $found, $context,
        )
        .to_compiler_error($location)
    };
    (stack_imbalance: $expected:expr, $actual:expr, $context:expr, $suggestion:expr, $location:expr) => {
        $crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::stack_imbalance(
            $expected,
            $actual,
            $context,
            $suggestion,
        )
        .to_compiler_error($location)
    };
    (tagged_pointer: $operation:expr, $context:expr, $location:expr) => {
        $crate::compiler_frontend::codegen::wasm::error::WasmGenerationError::tagged_pointer(
            $operation, $context,
        )
        .to_compiler_error($location)
    };
}

// =========================================================================
// Validation Helper Functions
// =========================================================================

/// Validate a WASM module using wasmparser and return a detailed error if validation fails.
///
/// This function wraps wasmparser validation and converts any errors into
/// WasmGenerationError with appropriate context and suggestions.
pub fn validate_wasm_bytes(
    wasm_bytes: &[u8],
    lir_context: &str,
) -> Result<(), WasmGenerationError> {
    match wasmparser::validate(wasm_bytes) {
        Ok(_) => Ok(()),
        Err(e) => Err(WasmGenerationError::from_wasmparser_error(&e, lir_context)),
    }
}

/// Validate a WASM module and return a CompilerError if validation fails.
///
/// This is a convenience function that combines validation and error conversion.
pub fn validate_wasm_module(
    wasm_bytes: &[u8],
    lir_context: &str,
    location: ErrorLocation,
) -> Result<(), CompilerError> {
    validate_wasm_bytes(wasm_bytes, lir_context).map_err(|e| e.to_compiler_error(location))
}
