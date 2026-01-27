//! Unit tests for HIR Error Handling
//!
//! These tests validate the HIR error types, context, and conversion to CompilerError.
//! Tests are organized by functionality: error creation, context handling, and validation errors.

use crate::compiler::compiler_errors::{ErrorLocation, ErrorType};
use crate::compiler::hir::build_hir::HirValidationError;
use crate::compiler::hir::errors::{
    HirError, HirErrorContext, HirErrorKind, HirTransformationStage, ValidationErrorContext,
};
use crate::compiler::parsers::tokenizer::tokens::TextLocation;

// ============================================================================
// Basic Error Creation Tests
// ============================================================================

#[test]
fn test_hir_error_display() {
    let error = HirError::new(
        HirErrorKind::UnsupportedConstruct("match expression".to_string()),
        ErrorLocation::default(),
        HirErrorContext::expression_linearization(),
    );

    let display = format!("{}", error);
    assert!(display.contains("match expression"));
}

#[test]
fn test_hir_error_with_suggestion() {
    let error = HirError::new(
        HirErrorKind::UndefinedVariable("x".to_string()),
        ErrorLocation::default(),
        HirErrorContext::default(),
    )
    .with_suggestion("Did you mean 'y'?");

    let display = format!("{}", error);
    assert!(display.contains("Did you mean 'y'?"));
}

#[test]
fn test_hir_error_to_compiler_error() {
    let hir_error = HirError::new(
        HirErrorKind::UnsupportedConstruct("test".to_string()),
        ErrorLocation::default(),
        HirErrorContext::function_transformation(),
    );

    let compiler_error: crate::compiler::compiler_errors::CompilerError = hir_error.into();
    assert_eq!(compiler_error.error_type, ErrorType::HirTransformation);
}

#[test]
fn test_internal_error_is_compiler_bug() {
    let error = HirError::new(
        HirErrorKind::InternalError("test".to_string()),
        ErrorLocation::default(),
        HirErrorContext::default(),
    );

    assert!(error.is_compiler_bug());

    let compiler_error: crate::compiler::compiler_errors::CompilerError = error.into();
    assert_eq!(compiler_error.error_type, ErrorType::Compiler);
}

#[test]
fn test_validation_error_is_compiler_bug() {
    let error = HirError::new(
        HirErrorKind::ValidationFailure {
            invariant: "no_nested_expressions".to_string(),
            description: "test".to_string(),
        },
        ErrorLocation::default(),
        HirErrorContext::validation(),
    );

    assert!(error.is_compiler_bug());
}

// ============================================================================
// Error Context Tests
// ============================================================================

#[test]
fn test_hir_error_context() {
    let context = HirErrorContext::function_transformation()
        .with_function("my_function")
        .with_block(5)
        .with_scope_depth(3)
        .with_info("extra", "info");

    assert_eq!(
        context.stage,
        HirTransformationStage::FunctionTransformation
    );
    assert_eq!(context.current_function, Some("my_function".to_string()));
    assert_eq!(context.current_block, Some(5));
    assert_eq!(context.scope_depth, 3);
    assert_eq!(
        context.additional_info.get("extra"),
        Some(&"info".to_string())
    );
}

#[test]
fn test_hir_validation_error_conversion() {
    let validation_error = HirValidationError::MissingTerminator {
        block_id: 5,
        location: None,
    };

    let hir_error: HirError = validation_error.into();
    assert!(matches!(hir_error.kind, HirErrorKind::MissingTerminator(5)));
}

// ============================================================================
// Validation Error Context Tests
// ============================================================================

#[test]
fn test_validation_error_context_creation() {
    let context =
        ValidationErrorContext::new("no_nested_expressions", "HIR expressions must be flat");

    assert_eq!(context.invariant_name, "no_nested_expressions");
    assert_eq!(
        context.invariant_description,
        "HIR expressions must be flat"
    );
    assert!(context.block_id.is_none());
    assert!(context.function_name.is_none());
}

#[test]
fn test_validation_error_context_with_block() {
    let context = ValidationErrorContext::new("test", "description").with_block(42);

    assert_eq!(context.block_id, Some(42));
}

#[test]
fn test_validation_error_context_with_function() {
    let context = ValidationErrorContext::new("test", "description").with_function("my_function");

    assert_eq!(context.function_name, Some("my_function".to_string()));
}

#[test]
fn test_validation_error_context_with_debug_info() {
    let context = ValidationErrorContext::new("test", "description")
        .with_debug_info("key1", "value1")
        .with_debug_info("key2", "value2");

    assert_eq!(context.debug_info.get("key1"), Some(&"value1".to_string()));
    assert_eq!(context.debug_info.get("key2"), Some(&"value2".to_string()));
}

#[test]
fn test_validation_error_context_format_for_display() {
    let context = ValidationErrorContext::new(
        "explicit_terminators",
        "Every block must end with a terminator",
    )
    .with_block(5)
    .with_function("process_data");

    let display = context.format_for_display();

    assert!(display.contains("explicit_terminators"));
    assert!(display.contains("Every block must end with a terminator"));
    assert!(display.contains("Block: 5"));
    assert!(display.contains("Function: process_data"));
}

#[test]
fn test_hir_error_with_validation_context() {
    let error = HirError::new(
        HirErrorKind::MissingTerminator(5),
        ErrorLocation::default(),
        HirErrorContext::validation(),
    )
    .with_validation_context(
        "explicit_terminators",
        "Every block must end with a terminator",
    );

    assert!(error.has_validation_context());
    assert_eq!(error.get_invariant_name(), Some("explicit_terminators"));
    assert_eq!(
        error.get_invariant_description(),
        Some("Every block must end with a terminator")
    );
    assert!(error.suggestion.is_some());
    assert!(error.suggestion.as_ref().unwrap().contains("compiler bug"));
}

#[test]
fn test_validation_with_context_full() {
    let validation_context =
        ValidationErrorContext::new("block_connectivity", "All blocks must be reachable")
            .with_block(10)
            .with_function("main")
            .with_debug_info("unreachable_count", "3");

    let error = HirError::validation_with_context(
        HirErrorKind::UnreachableBlock(10),
        None,
        validation_context,
    );

    assert!(error.has_validation_context());
    assert_eq!(error.context.current_block, Some(10));
    assert_eq!(error.context.current_function, Some("main".to_string()));
    assert_eq!(
        error.context.additional_info.get("unreachable_count"),
        Some(&"3".to_string())
    );
}

#[test]
fn test_validation_error_conversion_includes_context() {
    let validation_error = HirValidationError::MissingTerminator {
        block_id: 7,
        location: None,
    };

    let hir_error: HirError = validation_error.into();

    // Should have validation context from the conversion
    assert!(hir_error.has_validation_context());
    assert_eq!(hir_error.get_invariant_name(), Some("explicit_terminators"));
    assert!(hir_error.suggestion.is_some());
}

#[test]
fn test_nested_expression_validation_error_has_context() {
    let validation_error = HirValidationError::NestedExpression {
        location: TextLocation::default(),
        expression: "BinOp { ... }".to_string(),
    };

    let hir_error: HirError = validation_error.into();

    assert!(hir_error.has_validation_context());
    assert_eq!(
        hir_error.get_invariant_name(),
        Some("no_nested_expressions")
    );
}

#[test]
fn test_unreachable_block_validation_error_has_context() {
    let validation_error = HirValidationError::UnreachableBlock { block_id: 42 };

    let hir_error: HirError = validation_error.into();

    assert!(hir_error.has_validation_context());
    assert_eq!(hir_error.get_invariant_name(), Some("block_connectivity"));
}

#[test]
fn test_invalid_branch_target_validation_error_has_context() {
    let validation_error = HirValidationError::InvalidBranchTarget {
        source_block: 1,
        target_block: 999,
    };

    let hir_error: HirError = validation_error.into();

    assert!(hir_error.has_validation_context());
    assert_eq!(hir_error.get_invariant_name(), Some("terminator_targets"));
}

#[test]
fn test_validation_error_is_compiler_bug_with_context() {
    let error = HirError::validation(HirErrorKind::MissingTerminator(5), None)
        .with_validation_context(
            "explicit_terminators",
            "Every block must end with a terminator",
        );

    // Validation errors with context should be compiler bugs
    assert!(error.is_compiler_bug());
}
