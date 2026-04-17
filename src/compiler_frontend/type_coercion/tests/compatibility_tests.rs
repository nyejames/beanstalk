//! Compatibility check tests for `type_coercion::compatibility`.

use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::type_coercion::compatibility::{
    is_declaration_compatible, is_type_compatible,
};

#[test]
fn type_compatibility_int_vs_float_is_incompatible() {
    assert!(!is_type_compatible(&DataType::Float, &DataType::Int));
}

#[test]
fn declaration_compatibility_int_vs_float_is_compatible() {
    assert!(is_declaration_compatible(&DataType::Float, &DataType::Int));
}

#[test]
fn float_to_int_is_never_compatible() {
    assert!(!is_type_compatible(&DataType::Int, &DataType::Float));
    assert!(!is_declaration_compatible(&DataType::Int, &DataType::Float));
}

#[test]
fn bool_to_float_is_never_compatible() {
    assert!(!is_type_compatible(&DataType::Float, &DataType::Bool));
}

#[test]
fn inferred_accepts_any_type() {
    assert!(is_type_compatible(&DataType::Inferred, &DataType::Int));
    assert!(is_type_compatible(&DataType::Float, &DataType::Inferred));
}

#[test]
fn option_accepts_none() {
    assert!(is_type_compatible(
        &DataType::Option(Box::new(DataType::Int)),
        &DataType::None
    ));
}

#[test]
fn option_accepts_inner_type() {
    assert!(is_type_compatible(
        &DataType::Option(Box::new(DataType::Int)),
        &DataType::Int
    ));
}

#[test]
fn builtin_error_kind_accepts_string_slice() {
    assert!(is_type_compatible(
        &DataType::BuiltinErrorKind,
        &DataType::StringSlice
    ));
}

#[test]
fn identical_types_are_always_compatible() {
    assert!(is_type_compatible(&DataType::Int, &DataType::Int));
    assert!(is_type_compatible(&DataType::Float, &DataType::Float));
    assert!(is_type_compatible(&DataType::Bool, &DataType::Bool));
}

#[test]
fn struct_nominal_compatibility_ignores_ownership() {
    let mut string_table = StringTable::new();
    let nominal_path = InternedPath::from_single_str("Inner", &mut string_table);

    let expected =
        DataType::runtime_struct(nominal_path.to_owned(), vec![], Ownership::ImmutableOwned);
    let actual = DataType::runtime_struct(nominal_path, vec![], Ownership::MutableOwned);

    assert!(is_type_compatible(&expected, &actual));
}
