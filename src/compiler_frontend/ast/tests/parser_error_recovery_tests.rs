//! Malformed-input parser regression tests.
//!
//! WHAT: asserts that common malformed inputs fail with stable typed diagnostic facts.
//! WHY: parser changes should not silently degrade recovery paths or produce vague errors.

use crate::compiler_frontend::compiler_messages::{
    DeferredFeatureReason, DiagnosticKind, DiagnosticPayload, InvalidControlFlowStatementReason,
    InvalidFunctionSignatureReason, InvalidMatchPatternReason, InvalidMultiBindReason,
    InvalidStatementPositionReason, InvalidTypeAnnotationReason, RuleDiagnosticKind,
    SyntaxDiagnosticKind, TypeAnnotationContext,
};
use crate::compiler_frontend::tests::test_support::parse_single_file_ast_diagnostic;

#[test]
fn reports_missing_signature_colon() {
    let diagnostic = parse_single_file_ast_diagnostic("f|| -> Int\n;\n");

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Rule(RuleDiagnosticKind::InvalidFunctionSignature)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidFunctionSignature {
            reason: InvalidFunctionSignatureReason::MissingColonAfterReturns
        }
    );
}

#[test]
fn reports_stray_comma_in_function_body() {
    let diagnostic = parse_single_file_ast_diagnostic("broken||:\n    value = 1\n    ,\n;\n");

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Syntax(SyntaxDiagnosticKind::InvalidStatementPosition)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidStatementPosition {
            reason: InvalidStatementPositionReason::UnexpectedComma
        }
    );
}

#[test]
fn reports_wildcard_match_arms_as_permanent_rule_errors() {
    let diagnostic =
        parse_single_file_ast_diagnostic("value = 1\nif value is:\n    _ => io(\"one\")\n;\n");

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Rule(RuleDiagnosticKind::InvalidMatchPattern)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidMatchPattern {
            reason: InvalidMatchPatternReason::WildcardNotSupported,
            variant_name: None,
            scrutinee_name: None,
        }
    );
    assert!(diagnostic.primary_location.start_pos.char_column > 0);
}

#[test]
fn rejects_bare_labeled_blocks_with_declaration_guidance() {
    let diagnostic = parse_single_file_ast_diagnostic("label:\n    io(\"x\")\n;\n");

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Syntax(SyntaxDiagnosticKind::InvalidTypeAnnotation)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTypeAnnotation {
            context: TypeAnnotationContext::DeclarationTarget,
            reason: InvalidTypeAnnotationReason::UnexpectedColon,
        }
    );
    assert!(diagnostic.primary_location.start_pos.char_column > 0);
}

#[test]
fn reports_unterminated_match_scope_at_end_of_file() {
    let diagnostic =
        parse_single_file_ast_diagnostic("value = 1\nif value is:\n    1 => io(\"one\")\n");

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Rule(RuleDiagnosticKind::InvalidControlFlowStatement)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidControlFlowStatement {
            reason: InvalidControlFlowStatementReason::UnexpectedEndOfFileInMatch
        }
    );
}

#[test]
fn reports_case_outside_match_scope() {
    let diagnostic = parse_single_file_ast_diagnostic("case 1 => io(\"one\")\n");

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Syntax(SyntaxDiagnosticKind::InvalidStatementPosition)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidStatementPosition {
            reason: InvalidStatementPositionReason::UnexpectedCase
        }
    );
}

#[test]
fn reports_multi_bind_malformed_comma_sequence() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "pair || -> Int, Int:\n    return 1, 2\n;\n\na, , b = pair()\n",
    );

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Syntax(SyntaxDiagnosticKind::UnexpectedToken)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidMultiBind {
            reason: InvalidMultiBindReason::MissingTargetAfterComma,
            target_name: None,
        }
    );
}

#[test]
fn reports_multi_bind_mutable_target_without_explicit_type() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "pair || -> Int, Int:\n    return 1, 2\n;\n\na, b ~= pair()\n",
    );

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Rule(RuleDiagnosticKind::InvalidMultiBind)
    );
    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::InvalidMultiBind {
                reason: InvalidMultiBindReason::MutableTargetNeedsExplicitType,
                target_name: Some(_),
            }
        ),
        "{:?}",
        diagnostic.payload
    );
}

#[test]
fn reports_multi_bind_with_variable_rhs_rejected() {
    let diagnostic = parse_single_file_ast_diagnostic("value ~= 1\na, b = value\n");

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Rule(RuleDiagnosticKind::InvalidMultiBind)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidMultiBind {
            reason: InvalidMultiBindReason::UnsupportedRhs,
            target_name: None,
        }
    );
}

#[test]
fn reports_multi_bind_with_literal_rhs_rejected() {
    let diagnostic = parse_single_file_ast_diagnostic("a, b = 1\n");

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Rule(RuleDiagnosticKind::InvalidMultiBind)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidMultiBind {
            reason: InvalidMultiBindReason::UnsupportedRhs,
            target_name: None,
        }
    );
}

#[test]
fn reports_multi_bind_with_field_access_rhs_rejected() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "Thing = |\n    x Int,\n    y Int,\n|\nthing ~= Thing(1, 2)\na, b = thing.x\n",
    );

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Rule(RuleDiagnosticKind::InvalidMultiBind)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidMultiBind {
            reason: InvalidMultiBindReason::UnsupportedRhs,
            target_name: None,
        }
    );
}

#[test]
fn reports_reserved_must_keyword_in_function_body() {
    let diagnostic = parse_single_file_ast_diagnostic("must = 1\n");

    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::DeferredFeature {
            reason: DeferredFeatureReason::ReservedTraitMustKeyword
        }
    );
    assert!(diagnostic.primary_location.start_pos.char_column > 0);
}

#[test]
fn reports_reserved_this_keyword_in_function_body_statement_position() {
    let diagnostic = parse_single_file_ast_diagnostic("f||:\n    This\n;\n");

    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::DeferredFeature {
            reason: DeferredFeatureReason::ReservedTraitThisKeyword
        }
    );
}

#[test]
fn reports_reserved_this_keyword_in_declaration_type_position() {
    let diagnostic = parse_single_file_ast_diagnostic("value This = 1\n");

    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::DeferredFeature {
            reason: DeferredFeatureReason::ReservedTraitThisKeyword
        }
    );
}

#[test]
fn reports_reserved_must_keyword_in_expression_position() {
    let diagnostic = parse_single_file_ast_diagnostic("value = must\n");

    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::DeferredFeature {
            reason: DeferredFeatureReason::ReservedTraitMustKeyword
        }
    );
}

#[test]
fn reports_reserved_this_keyword_in_expression_position() {
    let diagnostic = parse_single_file_ast_diagnostic("value = This\n");

    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::DeferredFeature {
            reason: DeferredFeatureReason::ReservedTraitThisKeyword
        }
    );
}

#[test]
fn reports_reserved_must_keyword_in_copy_place_position() {
    let diagnostic = parse_single_file_ast_diagnostic("value = copy must\n");

    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::DeferredFeature {
            reason: DeferredFeatureReason::ReservedTraitMustKeyword
        }
    );
}

#[test]
fn reports_reserved_must_keyword_in_signature_member_position() {
    let diagnostic = parse_single_file_ast_diagnostic("sum|must Int| -> Int:\n    return 1\n;\n");

    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::DeferredFeature {
            reason: DeferredFeatureReason::ReservedTraitMustKeyword
        }
    );
}

#[test]
fn reports_reserved_must_keyword_in_postfix_member_position() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "Point = |\n    value Int = 1,\n|\n\npoint ~= Point()\nvalue = point.must\n",
    );

    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::DeferredFeature {
            reason: DeferredFeatureReason::ReservedTraitMustKeyword
        }
    );
}
