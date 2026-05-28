//! Unit tests for generic function diagnostic helpers.
//!
//! WHAT: asserts that `with_generic_instantiation_context` rebuilds diagnostics correctly.
//! WHY: the helper is the single point of truth for call-site-primary generic instantiation
//! diagnostics and should be tested in isolation.

use crate::compiler_frontend::ast::generic_functions::diagnostics::with_generic_instantiation_context;
use crate::compiler_frontend::compiler_messages::source_location::{CharPosition, SourceLocation};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticKind, DiagnosticLabel, DiagnosticLabelMessage,
    DiagnosticLabelStyle, DiagnosticPayload, RuleDiagnosticKind, TypeMismatchContext,
};
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

fn make_location(
    path: &str,
    line: i32,
    col: i32,
    string_table: &mut StringTable,
) -> SourceLocation {
    let interned_path = InternedPath::from_single_str(path, string_table);
    SourceLocation::new(
        interned_path,
        CharPosition {
            line_number: line,
            char_column: col,
        },
        CharPosition {
            line_number: line,
            char_column: col + 1,
        },
    )
}

#[test]
fn with_generic_instantiation_context_changes_primary_location_to_call_site() {
    let mut string_table = StringTable::new();
    let body_location = make_location("body.bst", 10, 5, &mut string_table);
    let call_location = make_location("call.bst", 20, 8, &mut string_table);

    let diagnostic = CompilerDiagnostic::type_mismatch(
        builtin_type_ids::INT,
        builtin_type_ids::STRING,
        TypeMismatchContext::FunctionArgument,
        body_location.clone(),
    );

    let transformed = with_generic_instantiation_context(diagnostic, call_location.clone());

    assert_eq!(transformed.primary_location, call_location);
}

#[test]
fn with_generic_instantiation_context_first_label_is_primary_call_site() {
    let mut string_table = StringTable::new();
    let body_location = make_location("body.bst", 10, 5, &mut string_table);
    let call_location = make_location("call.bst", 20, 8, &mut string_table);

    let diagnostic = CompilerDiagnostic::type_mismatch(
        builtin_type_ids::INT,
        builtin_type_ids::STRING,
        TypeMismatchContext::FunctionArgument,
        body_location,
    );

    let transformed = with_generic_instantiation_context(diagnostic, call_location.clone());

    let first_label = transformed
        .labels
        .first()
        .expect("expected at least one label");
    assert_eq!(first_label.location, call_location);
    assert_eq!(first_label.style, DiagnosticLabelStyle::Primary);
    assert_eq!(
        first_label.message,
        Some(DiagnosticLabelMessage::GenericInstantiationCallSite)
    );
}

#[test]
fn with_generic_instantiation_context_adds_secondary_body_site_label() {
    let mut string_table = StringTable::new();
    let body_location = make_location("body.bst", 10, 5, &mut string_table);
    let call_location = make_location("call.bst", 20, 8, &mut string_table);

    let diagnostic = CompilerDiagnostic::type_mismatch(
        builtin_type_ids::INT,
        builtin_type_ids::STRING,
        TypeMismatchContext::FunctionArgument,
        body_location.clone(),
    );

    let transformed = with_generic_instantiation_context(diagnostic, call_location);

    let body_label = transformed
        .labels
        .iter()
        .find(|label| {
            label.style == DiagnosticLabelStyle::Secondary
                && label.location == body_location
                && label.message == Some(DiagnosticLabelMessage::GenericInstantiationBodySite)
        })
        .expect("expected a secondary body-site label");

    assert_eq!(body_label.location, body_location);
}

#[test]
fn with_generic_instantiation_context_preserves_existing_secondary_labels() {
    let mut string_table = StringTable::new();
    let body_location = make_location("body.bst", 10, 5, &mut string_table);
    let call_location = make_location("call.bst", 20, 8, &mut string_table);
    let extra_location = make_location("extra.bst", 15, 3, &mut string_table);

    let diagnostic = CompilerDiagnostic::new(
        DiagnosticKind::Rule(RuleDiagnosticKind::UnknownName),
        body_location.clone(),
        DiagnosticPayload::UnknownName {
            name: string_table.intern("x"),
            namespace: crate::compiler_frontend::compiler_messages::NameNamespace::Value,
        },
    )
    .with_labels(vec![DiagnosticLabel::secondary(
        extra_location.clone(),
        Some(DiagnosticLabelMessage::PreviousDeclaration),
    )]);

    let transformed = with_generic_instantiation_context(diagnostic, call_location);

    let extra_label = transformed
        .labels
        .iter()
        .find(|label| {
            label.style == DiagnosticLabelStyle::Secondary
                && label.location == extra_location
                && label.message == Some(DiagnosticLabelMessage::PreviousDeclaration)
        })
        .expect("expected the existing secondary label to be preserved");

    assert_eq!(extra_label.location, extra_location);
}

#[test]
fn with_generic_instantiation_context_avoids_duplicate_body_secondary_label() {
    let mut string_table = StringTable::new();
    let body_location = make_location("body.bst", 10, 5, &mut string_table);
    let call_location = make_location("call.bst", 20, 8, &mut string_table);

    // Create a diagnostic where the body location is already a secondary label.
    let diagnostic = CompilerDiagnostic::new(
        DiagnosticKind::Rule(RuleDiagnosticKind::UnknownName),
        body_location.clone(),
        DiagnosticPayload::UnknownName {
            name: string_table.intern("x"),
            namespace: crate::compiler_frontend::compiler_messages::NameNamespace::Value,
        },
    )
    .with_labels(vec![DiagnosticLabel::secondary(
        body_location.clone(),
        Some(DiagnosticLabelMessage::PreviousDeclaration),
    )]);

    let transformed = with_generic_instantiation_context(diagnostic, call_location);

    let body_secondary_count = transformed
        .labels
        .iter()
        .filter(|label| {
            label.style == DiagnosticLabelStyle::Secondary && label.location == body_location
        })
        .count();

    assert_eq!(
        body_secondary_count, 1,
        "body location should appear as secondary only once"
    );
}

#[test]
fn with_generic_instantiation_context_drops_original_primary_label() {
    let mut string_table = StringTable::new();
    let body_location = make_location("body.bst", 10, 5, &mut string_table);
    let call_location = make_location("call.bst", 20, 8, &mut string_table);

    // Create a diagnostic that already has a primary label at the body location.
    let diagnostic = CompilerDiagnostic::new(
        DiagnosticKind::Rule(RuleDiagnosticKind::UnknownName),
        body_location.clone(),
        DiagnosticPayload::UnknownName {
            name: string_table.intern("x"),
            namespace: crate::compiler_frontend::compiler_messages::NameNamespace::Value,
        },
    )
    .with_labels(vec![DiagnosticLabel {
        location: body_location.clone(),
        style: DiagnosticLabelStyle::Primary,
        message: Some(DiagnosticLabelMessage::PreviousDeclaration),
    }]);

    let transformed = with_generic_instantiation_context(diagnostic, call_location.clone());

    // No primary label should remain at the old body location.
    assert!(
        !transformed.labels.iter().any(|label| {
            label.style == DiagnosticLabelStyle::Primary && label.location == body_location
        }),
        "original primary label at body location should be removed"
    );

    // The first label should be the new primary at call_location.
    let first_label = transformed.labels.first().unwrap();
    assert_eq!(first_label.location, call_location);
    assert_eq!(first_label.style, DiagnosticLabelStyle::Primary);
}
