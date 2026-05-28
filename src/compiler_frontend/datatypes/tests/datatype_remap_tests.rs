//! String-ID remapping tests for DataType, generic bridge keys, and receiver keys.
//!
//! WHAT: verifies that `DataType`, `GenericBaseType`, `GenericInstantiationKey`,
//!      `TypeIdentityKey`, and `ReceiverKey` can be remapped from local string tables
//!      into a merged global table.
//! WHY: per-file header parsing produces type metadata using local string tables;
//!      remapping must preserve all nested names, paths, and generic arguments.

use crate::compiler_frontend::compiler_messages::source_location::{CharPosition, SourceLocation};
use crate::compiler_frontend::datatypes::generic_identity_bridge::{
    BuiltinTypeKey, GenericBaseType, GenericInstantiationKey, TypeIdentityKey,
};
use crate::compiler_frontend::datatypes::{DataType, ReceiverKey};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

fn make_location(string_table: &mut StringTable) -> SourceLocation {
    let path = InternedPath::from_single_str("test.bst", string_table);
    SourceLocation::new(path, CharPosition::default(), CharPosition::default())
}

#[test]
fn receiver_key_struct_remaps_path() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let path = InternedPath::from_single_str("MyStruct", &mut local);
    let mut receiver = ReceiverKey::Struct(path);

    let remap = global.merge_from(&local);
    receiver.remap_string_ids(&remap);

    match receiver {
        ReceiverKey::Struct(remapped) => {
            assert_eq!(remapped.to_portable_string(&global), "MyStruct");
        }
        _ => panic!("expected Struct receiver"),
    }
}

#[test]
fn receiver_key_builtin_scalar_is_unchanged() {
    let local = StringTable::new();
    let mut global = StringTable::new();

    let mut receiver =
        ReceiverKey::BuiltinScalar(crate::compiler_frontend::datatypes::BuiltinScalarReceiver::Int);

    let remap = global.merge_from(&local);
    receiver.remap_string_ids(&remap);

    assert!(matches!(
        receiver,
        ReceiverKey::BuiltinScalar(crate::compiler_frontend::datatypes::BuiltinScalarReceiver::Int)
    ));
}

#[test]
fn data_type_named_type_remaps_name() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let name = local.intern("UserType");
    let mut data_type = DataType::NamedType(name);

    let remap = global.merge_from(&local);
    data_type.remap_string_ids(&remap);

    match data_type {
        DataType::NamedType(remapped) => {
            assert_eq!(global.resolve(remapped), "UserType");
        }
        _ => panic!("expected NamedType"),
    }
}

#[test]
fn data_type_type_parameter_remaps_name() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let name = local.intern("T");
    let mut data_type = DataType::TypeParameter {
        id: crate::compiler_frontend::datatypes::generic_parameters::TypeParameterId(0),
        canonical_id: None,
        name,
    };

    let remap = global.merge_from(&local);
    data_type.remap_string_ids(&remap);

    match data_type {
        DataType::TypeParameter { name: remapped, .. } => {
            assert_eq!(global.resolve(remapped), "T");
        }
        _ => panic!("expected TypeParameter"),
    }
}

#[test]
fn data_type_generic_instance_remaps_base_and_arguments() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let base_name = local.intern("Collection");
    let arg_name = local.intern("Int");

    let mut data_type = DataType::GenericInstance {
        base: GenericBaseType::Named(base_name),
        arguments: vec![DataType::NamedType(arg_name)],
    };

    let remap = global.merge_from(&local);
    data_type.remap_string_ids(&remap);

    match data_type {
        DataType::GenericInstance { base, arguments } => {
            match base {
                GenericBaseType::Named(remapped) => {
                    assert_eq!(global.resolve(remapped), "Collection");
                }
                _ => panic!("expected Named base"),
            }
            assert_eq!(arguments.len(), 1);
            match &arguments[0] {
                DataType::NamedType(remapped) => {
                    assert_eq!(global.resolve(*remapped), "Int");
                }
                _ => panic!("expected NamedType argument"),
            }
        }
        _ => panic!("expected GenericInstance"),
    }
}

#[test]
fn data_type_struct_remaps_nominal_path_and_generic_key() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let path = InternedPath::from_single_str("models/User", &mut local);
    let arg_path = InternedPath::from_single_str("String", &mut local);

    let mut data_type = DataType::Struct {
        nominal_path: path,
        type_id: crate::compiler_frontend::datatypes::ids::TypeId(1),
        const_record: false,
        generic_instance_key: Some(GenericInstantiationKey {
            base_path: arg_path.clone(),
            arguments: vec![TypeIdentityKey::Nominal(arg_path)],
        }),
    };

    let remap = global.merge_from(&local);
    data_type.remap_string_ids(&remap);

    match data_type {
        DataType::Struct {
            nominal_path,
            generic_instance_key,
            ..
        } => {
            assert_eq!(nominal_path.to_portable_string(&global), "models/User");
            let key = generic_instance_key.unwrap();
            assert_eq!(key.base_path.to_portable_string(&global), "String");
        }
        _ => panic!("expected Struct"),
    }
}

#[test]
fn data_type_reference_remaps_inner() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let name = local.intern("Int");
    let mut data_type = DataType::Reference(Box::new(DataType::NamedType(name)));

    let remap = global.merge_from(&local);
    data_type.remap_string_ids(&remap);

    match data_type {
        DataType::Reference(inner) => match *inner {
            DataType::NamedType(remapped) => {
                assert_eq!(global.resolve(remapped), "Int");
            }
            _ => panic!("expected NamedType inner"),
        },
        _ => panic!("expected Reference"),
    }
}

#[test]
fn data_type_returns_remaps_nested_types() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let name = local.intern("Bool");
    let mut data_type = DataType::Returns(vec![DataType::NamedType(name)]);

    let remap = global.merge_from(&local);
    data_type.remap_string_ids(&remap);

    match data_type {
        DataType::Returns(returns) => {
            assert_eq!(returns.len(), 1);
            match &returns[0] {
                DataType::NamedType(remapped) => {
                    assert_eq!(global.resolve(*remapped), "Bool");
                }
                _ => panic!("expected NamedType return"),
            }
        }
        _ => panic!("expected Returns"),
    }
}

#[test]
fn data_type_function_remaps_receiver_and_signature() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let receiver_path = InternedPath::from_single_str("MyStruct", &mut local);
    let _param_name = local.intern("value");
    let param_path = InternedPath::from_single_str("value", &mut local);

    let mut data_type = DataType::Function(
        Box::new(Some(ReceiverKey::Struct(receiver_path))),
        crate::compiler_frontend::ast::statements::functions::FunctionSignature {
            parameters: vec![crate::compiler_frontend::ast::ast_nodes::Declaration {
                id: param_path,
                value: crate::compiler_frontend::ast::expressions::expression::Expression::no_value(
                    make_location(&mut local),
                    DataType::Inferred,
                    crate::compiler_frontend::value_mode::ValueMode::ImmutableOwned,
                ),
            }],
            returns: vec![],
        },
    );

    let remap = global.merge_from(&local);
    data_type.remap_string_ids(&remap);

    match data_type {
        DataType::Function(receiver, signature) => {
            match receiver.as_ref() {
                Some(ReceiverKey::Struct(path)) => {
                    assert_eq!(path.to_portable_string(&global), "MyStruct");
                }
                _ => panic!("expected Struct receiver"),
            }
            assert_eq!(signature.parameters.len(), 1);
            assert_eq!(
                global.resolve(signature.parameters[0].id.name().unwrap()),
                "value"
            );
        }
        _ => panic!("expected Function"),
    }
}

#[test]
fn data_type_choices_remaps_nominal_path_and_generic_key() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let path = InternedPath::from_single_str("Result", &mut local);

    let mut data_type = DataType::Choices {
        nominal_path: path,
        type_id: crate::compiler_frontend::datatypes::ids::TypeId(1),
        generic_instance_key: None,
    };

    let remap = global.merge_from(&local);
    data_type.remap_string_ids(&remap);

    match data_type {
        DataType::Choices { nominal_path, .. } => {
            assert_eq!(nominal_path.to_portable_string(&global), "Result");
        }
        _ => panic!("expected Choices"),
    }
}

#[test]
fn data_type_option_remaps_inner() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let name = local.intern("String");
    let mut data_type = DataType::Option(Box::new(DataType::NamedType(name)));

    let remap = global.merge_from(&local);
    data_type.remap_string_ids(&remap);

    match data_type {
        DataType::Option(inner) => match *inner {
            DataType::NamedType(remapped) => {
                assert_eq!(global.resolve(remapped), "String");
            }
            _ => panic!("expected NamedType inner"),
        },
        _ => panic!("expected Option"),
    }
}

#[test]
fn data_type_fallible_carrier_remaps_success_and_error() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let success_name = local.intern("Int");
    let error_name = local.intern("Error");

    let mut data_type = DataType::FallibleCarrier {
        success: Box::new(DataType::NamedType(success_name)),
        error: Box::new(DataType::NamedType(error_name)),
    };

    let remap = global.merge_from(&local);
    data_type.remap_string_ids(&remap);

    match data_type {
        DataType::FallibleCarrier { success, error } => {
            match success.as_ref() {
                DataType::NamedType(remapped) => {
                    assert_eq!(global.resolve(*remapped), "Int");
                }
                _ => panic!("expected NamedType success"),
            }
            match error.as_ref() {
                DataType::NamedType(remapped) => {
                    assert_eq!(global.resolve(*remapped), "Error");
                }
                _ => panic!("expected NamedType error"),
            }
        }
        _ => panic!("expected FallibleCarrier"),
    }
}

#[test]
fn generic_base_type_named_remaps_name() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let name = local.intern("T");
    let mut base = GenericBaseType::Named(name);

    let remap = global.merge_from(&local);
    base.remap_string_ids(&remap);

    match base {
        GenericBaseType::Named(remapped) => {
            assert_eq!(global.resolve(remapped), "T");
        }
        _ => panic!("expected Named"),
    }
}

#[test]
fn generic_base_type_resolved_nominal_remaps_path() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let path = InternedPath::from_single_str("MyType", &mut local);
    let mut base = GenericBaseType::ResolvedNominal(path);

    let remap = global.merge_from(&local);
    base.remap_string_ids(&remap);

    match base {
        GenericBaseType::ResolvedNominal(remapped) => {
            assert_eq!(remapped.to_portable_string(&global), "MyType");
        }
        _ => panic!("expected ResolvedNominal"),
    }
}

#[test]
fn generic_instantiation_key_remaps_base_and_arguments() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let base_path = InternedPath::from_single_str("Container", &mut local);
    let arg_path = InternedPath::from_single_str("Int", &mut local);

    let mut key = GenericInstantiationKey {
        base_path,
        arguments: vec![
            TypeIdentityKey::Nominal(arg_path),
            TypeIdentityKey::Builtin(BuiltinTypeKey::Bool),
        ],
    };

    let remap = global.merge_from(&local);
    key.remap_string_ids(&remap);

    assert_eq!(key.base_path.to_portable_string(&global), "Container");
    assert_eq!(key.arguments.len(), 2);
    match &key.arguments[0] {
        TypeIdentityKey::Nominal(path) => {
            assert_eq!(path.to_portable_string(&global), "Int");
        }
        _ => panic!("expected Nominal argument"),
    }
    assert!(matches!(
        key.arguments[1],
        TypeIdentityKey::Builtin(BuiltinTypeKey::Bool)
    ));
}

#[test]
fn type_identity_key_nominal_remaps_path() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let path = InternedPath::from_single_str("User", &mut local);
    let mut key = TypeIdentityKey::Nominal(path);

    let remap = global.merge_from(&local);
    key.remap_string_ids(&remap);

    match key {
        TypeIdentityKey::Nominal(remapped) => {
            assert_eq!(remapped.to_portable_string(&global), "User");
        }
        _ => panic!("expected Nominal"),
    }
}

#[test]
fn type_identity_key_collection_remaps_inner() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let path = InternedPath::from_single_str("Int", &mut local);
    let mut key = TypeIdentityKey::Collection(Box::new(TypeIdentityKey::Nominal(path)));

    let remap = global.merge_from(&local);
    key.remap_string_ids(&remap);

    match key {
        TypeIdentityKey::Collection(inner) => match inner.as_ref() {
            TypeIdentityKey::Nominal(remapped) => {
                assert_eq!(remapped.to_portable_string(&global), "Int");
            }
            _ => panic!("expected Nominal inner"),
        },
        _ => panic!("expected Collection"),
    }
}

#[test]
fn type_identity_key_option_remaps_inner() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let path = InternedPath::from_single_str("String", &mut local);
    let mut key = TypeIdentityKey::Option(Box::new(TypeIdentityKey::Nominal(path)));

    let remap = global.merge_from(&local);
    key.remap_string_ids(&remap);

    match key {
        TypeIdentityKey::Option(inner) => match inner.as_ref() {
            TypeIdentityKey::Nominal(remapped) => {
                assert_eq!(remapped.to_portable_string(&global), "String");
            }
            _ => panic!("expected Nominal inner"),
        },
        _ => panic!("expected Option"),
    }
}

#[test]
fn type_identity_key_fallible_carrier_remaps_success_and_error() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    let success_path = InternedPath::from_single_str("Int", &mut local);
    let error_path = InternedPath::from_single_str("Error", &mut local);

    let mut key = TypeIdentityKey::FallibleCarrier {
        success: Box::new(TypeIdentityKey::Nominal(success_path)),
        error: Box::new(TypeIdentityKey::Nominal(error_path)),
    };

    let remap = global.merge_from(&local);
    key.remap_string_ids(&remap);

    match key {
        TypeIdentityKey::FallibleCarrier { success, error } => {
            match success.as_ref() {
                TypeIdentityKey::Nominal(path) => {
                    assert_eq!(path.to_portable_string(&global), "Int");
                }
                _ => panic!("expected Nominal success"),
            }
            match error.as_ref() {
                TypeIdentityKey::Nominal(path) => {
                    assert_eq!(path.to_portable_string(&global), "Error");
                }
                _ => panic!("expected Nominal error"),
            }
        }
        _ => panic!("expected FallibleCarrier"),
    }
}

#[test]
fn remap_preserves_correct_ids_when_global_has_preexisting_strings() {
    let mut local = StringTable::new();
    let mut global = StringTable::new();

    // Preexisting strings in global ensure the merge is non-identity.
    global.intern("preexisting_a");
    global.intern("preexisting_b");

    let _name = local.intern("MyType");
    let path = InternedPath::from_single_str("path/to/MyType", &mut local);

    let mut data_type = DataType::Struct {
        nominal_path: path,
        type_id: crate::compiler_frontend::datatypes::ids::TypeId(1),
        const_record: false,
        generic_instance_key: Some(GenericInstantiationKey {
            base_path: InternedPath::from_single_str("Base", &mut local),
            arguments: vec![TypeIdentityKey::Nominal(InternedPath::from_single_str(
                "MyType", &mut local,
            ))],
        }),
    };

    let remap = global.merge_from(&local);
    data_type.remap_string_ids(&remap);

    match data_type {
        DataType::Struct {
            nominal_path,
            generic_instance_key,
            ..
        } => {
            assert_eq!(nominal_path.to_portable_string(&global), "path/to/MyType");
            let key = generic_instance_key.unwrap();
            assert_eq!(key.base_path.to_portable_string(&global), "Base");
            match &key.arguments[0] {
                TypeIdentityKey::Nominal(path) => {
                    assert_eq!(path.to_portable_string(&global), "MyType");
                }
                _ => panic!("expected Nominal argument"),
            }
        }
        _ => panic!("expected Struct"),
    }
}
