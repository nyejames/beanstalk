//! Type-resolution regression tests.
//!
//! WHAT: validates diagnostic-type syntax conversion into canonical type identity.
//! WHY: these paths sit at the boundary between diagnostic type syntax and canonical
//!      type identity; mistakes here produce misleading errors or silent wrong-types.

use crate::compiler_frontend::ast::type_resolution::{
    resolve_diagnostic_type_to_type_id_checked, resolve_diagnostic_type_to_type_id_opt,
};
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::compiler_messages::{
    DiagnosticPayload, InvalidTypeAnnotationReason, NameNamespace, TypeAnnotationContext,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::builtin_type_ids;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::TokenKind;

// ---------------------------------------------------------------
//  Checked / optional diagnostic-type-to-TypeId bridge tests
// ---------------------------------------------------------------
//
// WHAT: prove that unresolved parse placeholders cannot silently
//       fall back to builtin `none` in production paths.
// WHY: the frontend type boundary cleanup removed unchecked
//      `resolve_diagnostic_type_to_type_id` from constructor
//      shells; these tests guard against regression.

#[test]
fn checked_conversion_rejects_inferred_type() {
    let mut type_environment =
        crate::compiler_frontend::datatypes::environment::TypeEnvironment::new();
    let location = SourceLocation::default();

    let error = resolve_diagnostic_type_to_type_id_checked(
        &DataType::Inferred,
        &mut type_environment,
        &location,
    )
    .expect_err("checked conversion should reject Inferred placeholder");

    assert!(
        matches!(
            &error.payload,
            DiagnosticPayload::InvalidTypeAnnotation {
                context: TypeAnnotationContext::DeclarationTarget,
                reason: InvalidTypeAnnotationReason::ExpectedTypeAnnotation {
                    found: TokenKind::Eof
                },
            }
        ),
        "expected InvalidTypeAnnotation for Inferred, got {:?}",
        error.payload
    );
}

#[test]
fn checked_conversion_rejects_unresolved_namespaced_type() {
    let mut string_table = StringTable::new();
    let mut type_environment =
        crate::compiler_frontend::datatypes::environment::TypeEnvironment::new();
    let location = SourceLocation::default();
    let namespace = string_table.intern("missing");
    let name = string_table.intern("Type");

    let error = resolve_diagnostic_type_to_type_id_checked(
        &DataType::NamespacedType { namespace, name },
        &mut type_environment,
        &location,
    )
    .expect_err("checked conversion should reject unresolved namespaced type");

    assert!(
        matches!(
            &error.payload,
            DiagnosticPayload::UnknownName {
                name: n,
                namespace: NameNamespace::Type,
            } if *n == name
        ),
        "expected UnknownName(type) for namespaced type, got {:?}",
        error.payload
    );
}

#[test]
fn optional_conversion_returns_none_for_unresolved_named_type() {
    let mut string_table = StringTable::new();
    let mut type_environment =
        crate::compiler_frontend::datatypes::environment::TypeEnvironment::new();
    let missing = string_table.intern("Missing");

    let result = resolve_diagnostic_type_to_type_id_opt(
        &DataType::NamedType(missing),
        &mut type_environment,
    );

    assert_eq!(
        result, None,
        "optional conversion must return None for unresolved named type"
    );
}

#[test]
fn optional_conversion_returns_none_for_inferred_type() {
    let mut type_environment =
        crate::compiler_frontend::datatypes::environment::TypeEnvironment::new();

    let result = resolve_diagnostic_type_to_type_id_opt(&DataType::Inferred, &mut type_environment);

    assert_eq!(
        result, None,
        "optional conversion must return None for Inferred placeholder"
    );
}

#[test]
fn optional_conversion_returns_some_for_resolved_builtin() {
    let mut type_environment =
        crate::compiler_frontend::datatypes::environment::TypeEnvironment::new();

    let result = resolve_diagnostic_type_to_type_id_opt(&DataType::Int, &mut type_environment);

    assert_eq!(
        result,
        Some(builtin_type_ids::INT),
        "optional conversion must return Some for resolved builtin"
    );
}
