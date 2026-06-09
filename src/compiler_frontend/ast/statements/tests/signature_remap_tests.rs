//! String-ID remapping tests for function signatures, return slots, and declaration shells.
//!
//! WHAT: verifies that `FunctionSignature`, `ReturnSlot`, `FunctionReturn`, and nested
//!      `Declaration`/`Expression` values can be remapped from local string tables into a
//!      merged global table.
//! WHY: per-file header parsing produces function signature shells using local string tables;
//!      remapping must preserve parameter names, type spellings, and default expressions.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnChannel, ReturnSlot,
};
use crate::compiler_frontend::compiler_messages::source_location::{CharPosition, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::value_mode::ValueMode;

fn make_location(string_table: &mut StringTable) -> SourceLocation {
    let path = InternedPath::from_single_str("test.bst", string_table);
    SourceLocation::new(path, CharPosition::default(), CharPosition::default())
}

#[test]
fn function_signature_remaps_parameters() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let _param_name = local.intern("value");
    let param_path = InternedPath::from_single_str("value", &mut local);

    let mut signature = FunctionSignature {
        parameters: vec![Declaration {
            id: param_path,
            value: Expression::no_value(
                make_location(&mut local),
                DataType::Inferred,
                ValueMode::ImmutableOwned,
            ),
        }],
        returns: vec![],
    };

    let remap = global.merge_from(&local);
    signature.remap_string_ids(&remap);

    assert_eq!(signature.parameters.len(), 1);
    assert_eq!(
        global.resolve(signature.parameters[0].id.name().unwrap()),
        "value"
    );
}

#[test]
fn function_signature_remaps_return_slots() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let return_type_name = local.intern("String");

    let mut signature = FunctionSignature {
        parameters: vec![],
        returns: vec![ReturnSlot::success(FunctionReturn::Value(
            DataType::NamedType(return_type_name),
        ))],
    };

    let remap = global.merge_from(&local);
    signature.remap_string_ids(&remap);

    assert_eq!(signature.returns.len(), 1);
    match signature.returns[0].data_type() {
        DataType::NamedType(remapped) => {
            assert_eq!(global.resolve(*remapped), "String");
        }
        _ => panic!("expected NamedType return"),
    }
}

#[test]
fn function_signature_remaps_error_return_slot() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let error_type_name = local.intern("Error");

    let mut signature = FunctionSignature {
        parameters: vec![],
        returns: vec![ReturnSlot::error(FunctionReturn::Value(
            DataType::NamedType(error_type_name),
        ))],
    };

    let remap = global.merge_from(&local);
    signature.remap_string_ids(&remap);

    assert_eq!(signature.returns.len(), 1);
    assert_eq!(signature.returns[0].channel, ReturnChannel::Error);
    match signature.returns[0].data_type() {
        DataType::NamedType(remapped) => {
            assert_eq!(global.resolve(*remapped), "Error");
        }
        _ => panic!("expected NamedType error return"),
    }
}

#[test]
fn function_return_alias_candidates_remaps_data_type() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let type_name = local.intern("Int");

    let mut function_return = FunctionReturn::AliasCandidates {
        parameter_indices: vec![0, 1],
        data_type: DataType::NamedType(type_name),
    };

    let remap = global.merge_from(&local);
    function_return.remap_string_ids(&remap);

    match function_return {
        FunctionReturn::AliasCandidates {
            parameter_indices,
            data_type,
        } => {
            assert_eq!(parameter_indices, vec![0, 1]);
            match data_type {
                DataType::NamedType(remapped) => {
                    assert_eq!(global.resolve(remapped), "Int");
                }
                _ => panic!("expected NamedType"),
            }
        }
        _ => panic!("expected AliasCandidates"),
    }
}

#[test]
fn return_slot_remaps_value_but_preserves_type_id() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let type_name = local.intern("Bool");

    let mut slot = ReturnSlot::success(FunctionReturn::Value(DataType::NamedType(type_name)));
    slot.type_id = Some(crate::compiler_frontend::datatypes::ids::TypeId(42));

    let remap = global.merge_from(&local);
    slot.remap_string_ids(&remap);

    assert_eq!(
        slot.type_id,
        Some(crate::compiler_frontend::datatypes::ids::TypeId(42))
    );
    match slot.data_type() {
        DataType::NamedType(remapped) => {
            assert_eq!(global.resolve(*remapped), "Bool");
        }
        _ => panic!("expected NamedType"),
    }
}

#[test]
fn declaration_with_expression_default_remaps_id_and_value() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let id_path = InternedPath::from_single_str("count", &mut local);
    let default_value = local.intern("default_value");

    let mut declaration = Declaration {
        id: id_path,
        value: Expression::string_slice(
            default_value,
            make_location(&mut local),
            ValueMode::ImmutableOwned,
        ),
    };

    let remap = global.merge_from(&local);
    declaration.remap_string_ids(&remap);

    assert_eq!(declaration.id.to_portable_string(&global), "count");
    match declaration.value.kind {
        crate::compiler_frontend::ast::expressions::expression_kind::ExpressionKind::StringSlice(
            remapped,
        ) => {
            assert_eq!(global.resolve(remapped), "default_value");
        }
        _ => panic!("expected StringSlice expression"),
    }
}

#[test]
fn remap_preserves_correct_ids_when_global_has_preexisting_strings() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    // Preexisting strings in global ensure the merge is non-identity.
    global.intern("preexisting_a");
    global.intern("preexisting_b");

    let _param_name = local.intern("my_param");
    let param_path = InternedPath::from_single_str("my_param", &mut local);
    let return_type_name = local.intern("MyReturn");

    let mut signature = FunctionSignature {
        parameters: vec![Declaration {
            id: param_path,
            value: Expression::no_value(
                make_location(&mut local),
                DataType::Inferred,
                ValueMode::ImmutableOwned,
            ),
        }],
        returns: vec![ReturnSlot::success(FunctionReturn::Value(
            DataType::NamedType(return_type_name),
        ))],
    };

    let remap = global.merge_from(&local);
    signature.remap_string_ids(&remap);

    assert_eq!(
        global.resolve(signature.parameters[0].id.name().unwrap()),
        "my_param"
    );
    match signature.returns[0].data_type() {
        DataType::NamedType(remapped) => {
            assert_eq!(global.resolve(*remapped), "MyReturn");
        }
        _ => panic!("expected NamedType return"),
    }
}
