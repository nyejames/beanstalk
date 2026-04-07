//! Compatibility check tests for `type_coercion::compatibility`.

use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::type_coercion::CompatibilityContext;
use crate::compiler_frontend::type_coercion::compatibility::is_type_compatible;

#[test]
fn exact_context_int_vs_float_is_incompatible() {
    assert!(!is_type_compatible(
        &DataType::Float,
        &DataType::Int,
        CompatibilityContext::Exact
    ));
}

#[test]
fn promotion_context_int_vs_float_is_compatible() {
    // Declaration-site coercion is now applied by coerce_expression_to_declared_type
    // before is_type_compatible is called, so only ReturnSlot needs to allow this.
    assert!(is_type_compatible(
        &DataType::Float,
        &DataType::Int,
        CompatibilityContext::ReturnSlot
    ));
}

#[test]
fn return_slot_context_int_vs_float_is_compatible() {
    assert!(is_type_compatible(
        &DataType::Float,
        &DataType::Int,
        CompatibilityContext::ReturnSlot
    ));
}

#[test]
fn float_to_int_is_never_compatible() {
    for context in [CompatibilityContext::Exact, CompatibilityContext::ReturnSlot] {
        assert!(
            !is_type_compatible(&DataType::Int, &DataType::Float, context),
            "expected Int not to accept Float in {context:?}"
        );
    }
}

#[test]
fn bool_to_float_is_never_compatible() {
    for context in [CompatibilityContext::Exact, CompatibilityContext::ReturnSlot] {
        assert!(
            !is_type_compatible(&DataType::Float, &DataType::Bool, context),
            "expected Float not to accept Bool in {context:?}"
        );
    }
}

#[test]
fn inferred_accepts_any_type() {
    assert!(is_type_compatible(
        &DataType::Inferred,
        &DataType::Int,
        CompatibilityContext::Exact
    ));
    assert!(is_type_compatible(
        &DataType::Float,
        &DataType::Inferred,
        CompatibilityContext::Exact
    ));
}

#[test]
fn option_accepts_none() {
    assert!(is_type_compatible(
        &DataType::Option(Box::new(DataType::Int)),
        &DataType::None,
        CompatibilityContext::Exact
    ));
}

#[test]
fn option_accepts_inner_type() {
    assert!(is_type_compatible(
        &DataType::Option(Box::new(DataType::Int)),
        &DataType::Int,
        CompatibilityContext::Exact
    ));
}

#[test]
fn builtin_error_kind_accepts_string_slice() {
    assert!(is_type_compatible(
        &DataType::BuiltinErrorKind,
        &DataType::StringSlice,
        CompatibilityContext::Exact
    ));
}

#[test]
fn identical_types_are_always_compatible() {
    for context in [CompatibilityContext::Exact, CompatibilityContext::ReturnSlot] {
        assert!(is_type_compatible(&DataType::Int, &DataType::Int, context));
        assert!(is_type_compatible(
            &DataType::Float,
            &DataType::Float,
            context
        ));
        assert!(is_type_compatible(&DataType::Bool, &DataType::Bool, context));
    }
}
