//! Compatibility check tests for `type_coercion::compatibility`.

use crate::compiler_frontend::datatypes::DataType;
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
fn collection_type_identity_is_element_type_only() {
    let left = DataType::Collection(Box::new(DataType::Int));
    let right = DataType::Collection(Box::new(DataType::Int));

    assert_eq!(left, right);
    assert!(is_type_compatible(&left, &right));
}

#[test]
fn struct_type_identity_is_nominal_and_const_record_sensitive_only() {
    let mut string_table = StringTable::new();
    let path = InternedPath::from_single_str("User", &mut string_table);

    let runtime = DataType::runtime_struct(path.clone(), vec![]);
    let same_runtime = DataType::runtime_struct(path.clone(), vec![]);
    let const_record = DataType::const_struct_record(path, vec![]);

    assert_eq!(runtime, same_runtime);
    assert_ne!(runtime, const_record);
    assert!(is_type_compatible(&runtime, &same_runtime));
}
