//! HIR Error Handling Module
//!
//! This module provides comprehensive error handling for HIR (High-Level Intermediate
//! Representation) generation and validation. It integrates with Beanstalk's existing
//! error system while providing HIR-specific error types and context.
//!
//! ## Error Categories
//!
//! 1. **Transformation Errors**: Failures during AST to HIR conversion
//! 2. **Validation Errors**: HIR invariant violations detected during validation
//! 3. **Context Errors**: Issues with scope, variable, or control flow context
//!
//! ## Usage
//!
//! ```rust
//! use crate::compiler_frontend::hir::errors::{HirError, HirErrorKind, HirErrorContext};
//!
//! // Create a transformation error
//! let error = HirError::transformation(
//!     HirErrorKind::UnsupportedConstruct("match expression".to_string()),
//!     location,
//!     HirErrorContext::expression_linearization(),
//! );
//!
//! // Convert to CompilerError for reporting
//! let compiler_error: CompilerError = error.into();
//! ```

use crate::compiler_frontend::compiler_errors::{
    CompilerError, ErrorLocation, ErrorMetaDataKey, ErrorType,
};
use crate::compiler_frontend::hir::hir_nodes::BlockId;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;
use std::collections::HashMap;
use std::fmt;

// ============================================================================
// HIR Error Types
// ============================================================================

/// The main HIR error type that wraps all HIR-related errors.
/// This provides a unified interface for error handling during HIR generation.
#[derive(Debug, Clone)]
pub struct HirError {
    /// The specific kind of error
    pub kind: HirErrorKind,
    /// Source location where the error occurred
    pub location: ErrorLocation,
    /// Additional context about the error
    pub context: HirErrorContext,
    /// Optional suggestion for fixing the error
    pub suggestion: Option<String>,
}

impl HirError {
    /// Creates a new HIR error with the given kind, location, and context
    pub fn new(kind: HirErrorKind, location: ErrorLocation, context: HirErrorContext) -> Self {
        HirError {
            kind,
            location,
            context,
            suggestion: None,
        }
    }

    /// Creates a transformation error
    pub fn transformation(
        kind: HirErrorKind,
        location: TextLocation,
        context: HirErrorContext,
    ) -> Self {
        HirError {
            kind,
            location: location.to_error_location_without_table(),
            context,
            suggestion: None,
        }
    }

    /// Creates a validation error
    pub fn validation(kind: HirErrorKind, location: Option<TextLocation>) -> Self {
        HirError {
            kind,
            location: location
                .map(|l| l.to_error_location_without_table())
                .unwrap_or_else(ErrorLocation::default),
            context: HirErrorContext::validation(),
            suggestion: Some("This is a compiler_frontend bug - please report it".to_string()),
        }
    }

    /// Creates a context error (scope, variable, or control flow issues)
    pub fn context_error(kind: HirErrorKind, location: TextLocation) -> Self {
        HirError {
            kind,
            location: location.to_error_location_without_table(),
            context: HirErrorContext::default(),
            suggestion: None,
        }
    }

    /// Adds a suggestion to the error
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Adds context to the error
    pub fn with_context(mut self, context: HirErrorContext) -> Self {
        self.context = context;
        self
    }

    /// Gets the error message
    pub fn message(&self) -> String {
        self.kind.to_string()
    }

    /// Checks if this is a compiler_frontend bug (internal error)
    /// Validation errors are compiler_frontend bugs because they indicate the HIR builder
    /// produced invalid HIR, which is a bug in the compiler_frontend, not user code.
    ///
    /// An error is considered a compiler_frontend bug if:
    /// 1. It's an InternalError or ValidationFailure kind
    /// 2. It's a validation-specific error kind (NestedExpression, MissingTerminator, etc.)
    /// 3. It has validation context (came from HirValidationError conversion)
    pub fn is_compiler_bug(&self) -> bool {
        // Check if it has validation context (came from validation)
        if self.has_validation_context() {
            return true;
        }

        // Check for error kinds that are always compiler_frontend bugs
        matches!(
            self.kind,
            HirErrorKind::InternalError(_)
                | HirErrorKind::ValidationFailure { .. }
                | HirErrorKind::NestedExpression { .. }
                | HirErrorKind::MissingTerminator(_)
                | HirErrorKind::MultipleTerminators { .. }
                | HirErrorKind::UnreachableBlock(_)
                | HirErrorKind::InvalidBranchTarget { .. }
        )
    }

    /// Checks if this error has validation context
    pub fn has_validation_context(&self) -> bool {
        self.context.additional_info.contains_key("invariant")
    }
}

impl fmt::Display for HirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(ref suggestion) = self.suggestion {
            write!(f, " (Suggestion: {})", suggestion)?;
        }
        Ok(())
    }
}

impl std::error::Error for HirError {}

// ============================================================================
// HIR Error Kinds
// ============================================================================

/// Specific kinds of HIR errors
#[derive(Debug, Clone)]
pub enum HirErrorKind {
    // === Transformation Errors ===
    /// An AST construct is not yet supported in HIR generation
    UnsupportedConstruct(String),

    /// Failed to transform a specific AST node type
    TransformationFailed { node_type: String, reason: String },

    /// Expression linearization failed
    ExpressionLinearizationFailed {
        expression_type: String,
        reason: String,
    },

    /// Control flow linearization failed
    ControlFlowLinearizationFailed { construct: String, reason: String },

    // === Variable and Scope Errors ===
    /// Variable not found in current scope
    UndefinedVariable(String),

    /// Variable already declared in current scope
    DuplicateVariable(String),

    /// Invalid variable access (e.g., mutable access to immutable variable)
    InvalidVariableAccess { variable: String, reason: String },

    /// Scope management error
    ScopeError(String),

    // === Control Flow Errors ===
    /// Break statement outside of loop
    BreakOutsideLoop,

    /// Continue statement outside of loop
    ContinueOutsideLoop,

    /// Invalid branch target
    InvalidBranchTarget {
        source_block: BlockId,
        target_block: BlockId,
    },

    /// Missing terminator in block
    MissingTerminator(BlockId),

    /// Multiple terminators in block
    MultipleTerminators { block_id: BlockId, count: usize },

    // === Function Errors ===
    /// Function not found
    UndefinedFunction(String),

    /// Invalid function signature
    InvalidFunctionSignature { function: String, reason: String },

    /// Function parameter error
    FunctionParameterError { function: String, reason: String },

    // === Struct Errors ===
    /// Struct not found
    UndefinedStruct(String),

    /// Field not found in struct
    UndefinedField {
        struct_name: String,
        field_name: String,
    },

    /// Invalid field access
    InvalidFieldAccess {
        struct_name: String,
        field_name: String,
        reason: String,
    },

    // === Validation Errors ===
    /// HIR invariant violation
    ValidationFailure {
        invariant: String,
        description: String,
    },

    /// Nested expression found where flat expression expected
    NestedExpression { expression: String, depth: usize },

    /// Unreachable block detected
    UnreachableBlock(BlockId),

    // === Drop and Ownership Errors ===
    /// Missing drop for variable
    MissingDrop { variable: String, exit_path: String },

    /// Invalid drop insertion
    InvalidDropInsertion { variable: String, reason: String },

    // === Template Errors ===
    /// Template processing failed
    TemplateProcessingFailed {
        template_id: Option<String>,
        reason: String,
    },

    // === Internal Errors ===
    /// Internal compiler_frontend error (bug)
    InternalError(String),
}

impl fmt::Display for HirErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Transformation errors
            HirErrorKind::UnsupportedConstruct(construct) => {
                write!(
                    f,
                    "Unsupported AST construct in HIR generation: {}",
                    construct
                )
            }
            HirErrorKind::TransformationFailed { node_type, reason } => {
                write!(f, "Failed to transform {}: {}", node_type, reason)
            }
            HirErrorKind::ExpressionLinearizationFailed {
                expression_type,
                reason,
            } => {
                write!(
                    f,
                    "Failed to linearize expression '{}': {}",
                    expression_type, reason
                )
            }
            HirErrorKind::ControlFlowLinearizationFailed { construct, reason } => {
                write!(
                    f,
                    "Failed to linearize control flow '{}': {}",
                    construct, reason
                )
            }

            // Variable and scope errors
            HirErrorKind::UndefinedVariable(var) => {
                write!(f, "Undefined variable: '{}'", var)
            }
            HirErrorKind::DuplicateVariable(var) => {
                write!(f, "Variable '{}' already declared in this scope", var)
            }
            HirErrorKind::InvalidVariableAccess { variable, reason } => {
                write!(f, "Invalid access to variable '{}': {}", variable, reason)
            }
            HirErrorKind::ScopeError(msg) => {
                write!(f, "Scope error: {}", msg)
            }

            // Control flow errors
            HirErrorKind::BreakOutsideLoop => {
                write!(f, "Break statement outside of loop")
            }
            HirErrorKind::ContinueOutsideLoop => {
                write!(f, "Continue statement outside of loop")
            }
            HirErrorKind::InvalidBranchTarget {
                source_block,
                target_block,
            } => {
                write!(
                    f,
                    "Block {} branches to invalid block {}",
                    source_block, target_block
                )
            }
            HirErrorKind::MissingTerminator(block_id) => {
                write!(f, "Block {} is missing a terminator", block_id)
            }
            HirErrorKind::MultipleTerminators { block_id, count } => {
                write!(
                    f,
                    "Block {} has {} terminators (expected 1)",
                    block_id, count
                )
            }

            // Function errors
            HirErrorKind::UndefinedFunction(func) => {
                write!(f, "Undefined function: '{}'", func)
            }
            HirErrorKind::InvalidFunctionSignature { function, reason } => {
                write!(
                    f,
                    "Invalid signature for function '{}': {}",
                    function, reason
                )
            }
            HirErrorKind::FunctionParameterError { function, reason } => {
                write!(f, "Parameter error in function '{}': {}", function, reason)
            }

            // Struct errors
            HirErrorKind::UndefinedStruct(name) => {
                write!(f, "Undefined struct: '{}'", name)
            }
            HirErrorKind::UndefinedField {
                struct_name,
                field_name,
            } => {
                write!(
                    f,
                    "Field '{}' not found in struct '{}'",
                    field_name, struct_name
                )
            }
            HirErrorKind::InvalidFieldAccess {
                struct_name,
                field_name,
                reason,
            } => {
                write!(
                    f,
                    "Invalid access to field '{}' in struct '{}': {}",
                    field_name, struct_name, reason
                )
            }

            // Validation errors
            HirErrorKind::ValidationFailure {
                invariant,
                description,
            } => {
                write!(f, "HIR invariant '{}' violated: {}", invariant, description)
            }
            HirErrorKind::NestedExpression { expression, depth } => {
                write!(
                    f,
                    "Nested expression found at depth {}: {}",
                    depth, expression
                )
            }
            HirErrorKind::UnreachableBlock(block_id) => {
                write!(f, "Block {} is unreachable from entry", block_id)
            }

            // Drop and ownership errors
            HirErrorKind::MissingDrop {
                variable,
                exit_path,
            } => {
                write!(
                    f,
                    "Missing drop for '{}' on exit path '{}'",
                    variable, exit_path
                )
            }
            HirErrorKind::InvalidDropInsertion { variable, reason } => {
                write!(f, "Invalid drop insertion for '{}': {}", variable, reason)
            }

            // Template errors
            HirErrorKind::TemplateProcessingFailed {
                template_id,
                reason,
            } => {
                if let Some(id) = template_id {
                    write!(f, "Template '{}' processing failed: {}", id, reason)
                } else {
                    write!(f, "Template processing failed: {}", reason)
                }
            }

            // Internal errors
            HirErrorKind::InternalError(msg) => {
                write!(f, "Internal HIR error: {}", msg)
            }
        }
    }
}

// ============================================================================
// HIR Error Context
// ============================================================================

/// Context information about where and why an HIR error occurred.
/// This helps provide more detailed error messages and debugging information.
#[derive(Debug, Clone, Default)]
pub struct HirErrorContext {
    /// The transformation stage where the error occurred
    pub stage: HirTransformationStage,
    /// The current function being processed (if any)
    pub current_function: Option<String>,
    /// The current block being processed (if any)
    pub current_block: Option<BlockId>,
    /// The scope depth at the time of the error
    pub scope_depth: usize,
    /// Additional context-specific information
    pub additional_info: HashMap<String, String>,
}

impl HirErrorContext {
    /// Creates a new error context
    pub fn new(stage: HirTransformationStage) -> Self {
        HirErrorContext {
            stage,
            current_function: None,
            current_block: None,
            scope_depth: 0,
            additional_info: HashMap::new(),
        }
    }

    /// Creates a context for expression linearization
    pub fn expression_linearization() -> Self {
        Self::new(HirTransformationStage::ExpressionLinearization)
    }

    /// Creates a context for control flow linearization
    pub fn control_flow_linearization() -> Self {
        Self::new(HirTransformationStage::ControlFlowLinearization)
    }

    /// Creates a context for variable declaration
    pub fn variable_declaration() -> Self {
        Self::new(HirTransformationStage::VariableDeclaration)
    }

    /// Creates a context for drop insertion
    pub fn drop_insertion() -> Self {
        Self::new(HirTransformationStage::DropInsertion)
    }

    /// Creates a context for function transformation
    pub fn function_transformation() -> Self {
        Self::new(HirTransformationStage::FunctionTransformation)
    }

    /// Creates a context for struct handling
    pub fn struct_handling() -> Self {
        Self::new(HirTransformationStage::StructHandling)
    }

    /// Creates a context for template processing
    pub fn template_processing() -> Self {
        Self::new(HirTransformationStage::TemplateProcessing)
    }

    /// Creates a context for validation
    pub fn validation() -> Self {
        Self::new(HirTransformationStage::Validation)
    }

    /// Sets the current function
    pub fn with_function(mut self, function: impl Into<String>) -> Self {
        self.current_function = Some(function.into());
        self
    }

    /// Sets the current block
    pub fn with_block(mut self, block_id: BlockId) -> Self {
        self.current_block = Some(block_id);
        self
    }

    /// Sets the scope depth
    pub fn with_scope_depth(mut self, depth: usize) -> Self {
        self.scope_depth = depth;
        self
    }

    /// Adds additional context information
    pub fn with_info(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.additional_info.insert(key.into(), value.into());
        self
    }
}

/// The stage of HIR transformation where an error occurred
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HirTransformationStage {
    #[default]
    Unknown,
    ExpressionLinearization,
    ControlFlowLinearization,
    VariableDeclaration,
    DropInsertion,
    FunctionTransformation,
    StructHandling,
    TemplateProcessing,
    Validation,
}

impl fmt::Display for HirTransformationStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HirTransformationStage::Unknown => write!(f, "Unknown"),
            HirTransformationStage::ExpressionLinearization => {
                write!(f, "Expression Linearization")
            }
            HirTransformationStage::ControlFlowLinearization => {
                write!(f, "Control Flow Linearization")
            }
            HirTransformationStage::VariableDeclaration => write!(f, "Variable Declaration"),
            HirTransformationStage::DropInsertion => write!(f, "Drop Insertion"),
            HirTransformationStage::FunctionTransformation => write!(f, "Function Transformation"),
            HirTransformationStage::StructHandling => write!(f, "Struct Handling"),
            HirTransformationStage::TemplateProcessing => write!(f, "Template Processing"),
            HirTransformationStage::Validation => write!(f, "Validation"),
        }
    }
}

// ============================================================================
// Conversion to CompilerError
// ============================================================================

impl From<HirError> for CompilerError {
    fn from(error: HirError) -> Self {
        let mut metadata = HashMap::new();

        // Add compilation stage
        metadata.insert(
            ErrorMetaDataKey::CompilationStage,
            match error.context.stage {
                HirTransformationStage::ExpressionLinearization => "HIR Expression Linearization",
                HirTransformationStage::ControlFlowLinearization => {
                    "HIR Control Flow Linearization"
                }
                HirTransformationStage::VariableDeclaration => "HIR Variable Declaration",
                HirTransformationStage::DropInsertion => "HIR Drop Insertion",
                HirTransformationStage::FunctionTransformation => "HIR Function Transformation",
                HirTransformationStage::StructHandling => "HIR Struct Handling",
                HirTransformationStage::TemplateProcessing => "HIR Template Processing",
                HirTransformationStage::Validation => "HIR Validation",
                HirTransformationStage::Unknown => "HIR Generation",
            },
        );

        // Add suggestion if present
        if let Some(ref suggestion) = error.suggestion {
            // We need to leak the string to get a static reference
            // This is acceptable for error messages which are typically short-lived
            let suggestion_static: &'static str = Box::leak(suggestion.clone().into_boxed_str());
            metadata.insert(ErrorMetaDataKey::PrimarySuggestion, suggestion_static);
        }

        // Determine error type based on whether this is a compiler_frontend bug
        let error_type = if error.is_compiler_bug() {
            ErrorType::Compiler
        } else {
            ErrorType::HirTransformation
        };

        CompilerError {
            msg: error.message(),
            location: error.location,
            error_type,
            metadata,
        }
    }
}

impl From<HirValidationError> for HirError {
    fn from(error: HirValidationError) -> Self {
        match error {
            HirValidationError::NestedExpression {
                location,
                expression,
            } => HirError::validation(
                HirErrorKind::NestedExpression {
                    expression,
                    depth: 0,
                },
                Some(location),
            )
            .with_validation_context("no_nested_expressions", "HIR expressions must be flat"),
            HirValidationError::MissingTerminator { block_id, location } => {
                HirError::validation(HirErrorKind::MissingTerminator(block_id), location)
                    .with_validation_context(
                        "explicit_terminators",
                        "Every HIR block must end with exactly one terminator",
                    )
            }
            HirValidationError::MultipleTerminators { block_id, count } => {
                HirError::validation(HirErrorKind::MultipleTerminators { block_id, count }, None)
                    .with_validation_context(
                        "explicit_terminators",
                        "Every HIR block must end with exactly one terminator",
                    )
            }
            HirValidationError::UndeclaredVariable { variable, location } => {
                HirError::validation(HirErrorKind::UndefinedVariable(variable), Some(location))
                    .with_validation_context(
                        "variable_declaration_order",
                        "All variables must be declared before use",
                    )
            }
            HirValidationError::MissingDrop {
                variable,
                exit_path,
                location,
            } => HirError::validation(
                HirErrorKind::MissingDrop {
                    variable,
                    exit_path,
                },
                Some(location),
            )
            .with_validation_context(
                "drop_coverage",
                "All ownership-capable variables must have possible_drop on exit paths",
            ),
            HirValidationError::UnreachableBlock { block_id } => {
                HirError::validation(HirErrorKind::UnreachableBlock(block_id), None)
                    .with_validation_context(
                        "block_connectivity",
                        "All HIR blocks must be reachable from the entry block",
                    )
            }
            HirValidationError::InvalidBranchTarget {
                source_block,
                target_block,
            } => HirError::validation(
                HirErrorKind::InvalidBranchTarget {
                    source_block,
                    target_block,
                },
                None,
            )
            .with_validation_context(
                "terminator_targets",
                "All branch targets must reference valid block IDs",
            ),
            HirValidationError::InvalidAssignment {
                variable,
                location,
                reason,
            } => HirError::validation(
                HirErrorKind::InvalidVariableAccess { variable, reason },
                Some(location),
            )
            .with_validation_context(
                "assignment_discipline",
                "Assignments must follow proper discipline",
            ),
        }
    }
}

// ============================================================================
// Validation Error Context Enhancement
// ============================================================================

/// Detailed context for validation errors to aid debugging.
/// This provides additional information about which invariant was violated
/// and what the expected behavior should be.
#[derive(Debug, Clone, Default)]
pub struct ValidationErrorContext {
    /// The name of the invariant that was violated
    pub invariant_name: String,
    /// Description of what the invariant requires
    pub invariant_description: String,
    /// The block ID where the violation occurred (if applicable)
    pub block_id: Option<BlockId>,
    /// The function name where the violation occurred (if applicable)
    pub function_name: Option<String>,
    /// Additional debugging information
    pub debug_info: HashMap<String, String>,
}

impl ValidationErrorContext {
    /// Creates a new validation error context
    pub fn new(invariant_name: impl Into<String>, description: impl Into<String>) -> Self {
        ValidationErrorContext {
            invariant_name: invariant_name.into(),
            invariant_description: description.into(),
            block_id: None,
            function_name: None,
            debug_info: HashMap::new(),
        }
    }

    /// Sets the block ID where the violation occurred
    pub fn with_block(mut self, block_id: BlockId) -> Self {
        self.block_id = Some(block_id);
        self
    }

    /// Sets the function name where the violation occurred
    pub fn with_function(mut self, function_name: impl Into<String>) -> Self {
        self.function_name = Some(function_name.into());
        self
    }

    /// Adds debugging information
    pub fn with_debug_info(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.debug_info.insert(key.into(), value.into());
        self
    }

    /// Formats the context for error display
    pub fn format_for_display(&self) -> String {
        let mut parts = Vec::new();

        parts.push(format!("Invariant '{}' violated", self.invariant_name));
        parts.push(format!("Expected: {}", self.invariant_description));

        if let Some(block_id) = self.block_id {
            parts.push(format!("Block: {}", block_id));
        }

        if let Some(ref func) = self.function_name {
            parts.push(format!("Function: {}", func));
        }

        if !self.debug_info.is_empty() {
            let debug_str: Vec<String> = self
                .debug_info
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            parts.push(format!("Debug: {}", debug_str.join(", ")));
        }

        parts.join("; ")
    }
}

impl HirError {
    /// Adds validation context to the error
    pub fn with_validation_context(
        mut self,
        invariant_name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let invariant = invariant_name.into();
        let desc = description.into();

        // Add to additional_info in context
        self.context
            .additional_info
            .insert("invariant".to_string(), invariant.clone());
        self.context
            .additional_info
            .insert("invariant_description".to_string(), desc.clone());

        // Update suggestion to be more helpful
        self.suggestion = Some(format!(
            "This is a compiler_frontend bug - the HIR builder violated the '{}' invariant. Please report this issue.",
            invariant
        ));

        self
    }

    /// Creates a validation error with full context
    pub fn validation_with_context(
        kind: HirErrorKind,
        location: Option<TextLocation>,
        validation_context: ValidationErrorContext,
    ) -> Self {
        let mut error = HirError::validation(kind, location);

        // Transfer validation context to error context
        error.context.additional_info.insert(
            "invariant".to_string(),
            validation_context.invariant_name.clone(),
        );
        error.context.additional_info.insert(
            "invariant_description".to_string(),
            validation_context.invariant_description.clone(),
        );

        if let Some(block_id) = validation_context.block_id {
            error.context.current_block = Some(block_id);
        }

        if let Some(func) = validation_context.function_name {
            error.context.current_function = Some(func);
        }

        for (key, value) in validation_context.debug_info {
            error.context.additional_info.insert(key, value);
        }

        error.suggestion = Some(format!(
            "This is a compiler_frontend bug - the HIR builder violated the '{}' invariant. Please report this issue.",
            validation_context.invariant_name
        ));

        error
    }

    /// Gets the invariant name if this is a validation error
    pub fn get_invariant_name(&self) -> Option<&str> {
        self.context
            .additional_info
            .get("invariant")
            .map(|s| s.as_str())
    }

    /// Gets the invariant description if this is a validation error
    pub fn get_invariant_description(&self) -> Option<&str> {
        self.context
            .additional_info
            .get("invariant_description")
            .map(|s| s.as_str())
    }
}

// ============================================================================
// Helper Macros for HIR Error Creation
// ============================================================================

/// Creates an HIR error for unsupported constructs
#[macro_export]
macro_rules! hir_unsupported {
    ($construct:expr, $location:expr) => {
        $crate::compiler_frontend::hir::errors::HirError::transformation(
            $crate::compiler_frontend::hir::errors::HirErrorKind::UnsupportedConstruct(
                $construct.to_string(),
            ),
            $location,
            $crate::compiler_frontend::hir::errors::HirErrorContext::default(),
        )
    };
    ($construct:expr, $location:expr, $context:expr) => {
        $crate::compiler_frontend::hir::errors::HirError::transformation(
            $crate::compiler_frontend::hir::errors::HirErrorKind::UnsupportedConstruct(
                $construct.to_string(),
            ),
            $location,
            $context,
        )
    };
}

/// Creates an HIR error for transformation failures
#[macro_export]
macro_rules! hir_transform_failed {
    ($node_type:expr, $reason:expr, $location:expr) => {
        $crate::compiler_frontend::hir::errors::HirError::transformation(
            $crate::compiler_frontend::hir::errors::HirErrorKind::TransformationFailed {
                node_type: $node_type.to_string(),
                reason: $reason.to_string(),
            },
            $location,
            $crate::compiler_frontend::hir::errors::HirErrorContext::default(),
        )
    };
    ($node_type:expr, $reason:expr, $location:expr, $context:expr) => {
        $crate::compiler_frontend::hir::errors::HirError::transformation(
            $crate::compiler_frontend::hir::errors::HirErrorKind::TransformationFailed {
                node_type: $node_type.to_string(),
                reason: $reason.to_string(),
            },
            $location,
            $context,
        )
    };
}

/// Creates an HIR internal error (compiler_frontend bug)
#[macro_export]
macro_rules! hir_internal_error {
    ($msg:expr) => {
        $crate::compiler_frontend::hir::errors::HirError::new(
            $crate::compiler_frontend::hir::errors::HirErrorKind::InternalError($msg.to_string()),
            $crate::compiler_frontend::compiler_errors::ErrorLocation::default(),
            $crate::compiler_frontend::hir::errors::HirErrorContext::default(),
        )
        .with_suggestion("This is a compiler_frontend bug - please report it")
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::compiler_frontend::hir::errors::HirError::new(
            $crate::compiler_frontend::hir::errors::HirErrorKind::InternalError(format!($fmt, $($arg)*)),
            $crate::compiler_frontend::compiler_errors::ErrorLocation::default(),
            $crate::compiler_frontend::hir::errors::HirErrorContext::default(),
        )
        .with_suggestion("This is a compiler_frontend bug - please report it")
    };
}

// ============================================================================
// Result Type Alias
// ============================================================================

/// Result type for HIR operations
#[allow(dead_code)]
pub type HirResult<T> = Result<T, HirError>;
