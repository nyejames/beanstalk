//! Generic identity bridge types for diagnostics and HIR lowering.
//!
//! WHAT: owns the legacy `DataType`/HIR-facing keys that can describe generic instances
//!      outside the canonical `TypeEnvironment`.
//! WHY: HIR still registers generic nominal layouts through a lowering-local side table,
//!      and diagnostics still need source-like spelling. These keys must not decide
//!      semantic equality in AST or HIR; use `TypeId` in `TypeEnvironment` for that.

use super::DataType;
use super::display::format_fallible_signature_parts;
use super::environment::TypeEnvironment;
use super::ids::TypeId;
use crate::compiler_frontend::external_packages::ExternalTypeId;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap, StringTable};

// -----------------------------------------------------------
//  Identity Keys (HIR / Diagnostic Bridge)
// -----------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GenericBaseType {
    Named(StringId),
    ResolvedNominal(InternedPath),
    #[allow(dead_code)] // Deferred until external generic type metadata exists.
    External(ExternalTypeId),
    Builtin(BuiltinGenericType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinGenericType {
    Collection { fixed_capacity: Option<usize> },
    Map,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinTypeKey {
    Bool,
    Int,
    Float,
    Decimal,
    String,
    Char,
    Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericInstantiationKey {
    pub base_path: InternedPath,
    pub arguments: Vec<TypeIdentityKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeIdentityKey {
    Builtin(BuiltinTypeKey),
    Nominal(InternedPath),
    External(ExternalTypeId),
    Collection {
        element: Box<TypeIdentityKey>,
        fixed_capacity: Option<usize>,
    },
    Map {
        key: Box<TypeIdentityKey>,
        value: Box<TypeIdentityKey>,
    },
    Option(Box<TypeIdentityKey>),
    FallibleCarrier {
        success: Box<TypeIdentityKey>,
        error: Box<TypeIdentityKey>,
    },
    GenericInstance(GenericInstantiationKey),
}

impl GenericBaseType {
    /// Remap interned names and paths in this generic base type.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            GenericBaseType::Named(name) => {
                *name = remap.get(*name);
            }

            GenericBaseType::ResolvedNominal(path) => {
                path.remap_string_ids(remap);
            }

            GenericBaseType::External(_) | GenericBaseType::Builtin(_) => {}
        }
    }
}

impl GenericInstantiationKey {
    /// Remap the base path and every argument key recursively.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.base_path.remap_string_ids(remap);
        for argument in &mut self.arguments {
            argument.remap_string_ids(remap);
        }
    }
}

impl TypeIdentityKey {
    /// Remap interned paths in this identity key recursively.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            TypeIdentityKey::Builtin(_) | TypeIdentityKey::External(_) => {}

            TypeIdentityKey::Nominal(path) => {
                path.remap_string_ids(remap);
            }

            TypeIdentityKey::Collection { element: inner, .. } => {
                inner.remap_string_ids(remap);
            }

            TypeIdentityKey::Map { key, value } => {
                key.remap_string_ids(remap);
                value.remap_string_ids(remap);
            }

            TypeIdentityKey::Option(inner) => {
                inner.remap_string_ids(remap);
            }

            TypeIdentityKey::FallibleCarrier { success, error } => {
                success.remap_string_ids(remap);
                error.remap_string_ids(remap);
            }

            TypeIdentityKey::GenericInstance(instance) => {
                instance.remap_string_ids(remap);
            }
        }
    }
}

// -----------------------------------------------------------
//  Display Helpers
// -----------------------------------------------------------

pub fn display_generic_instantiation_key(
    key: &GenericInstantiationKey,
    string_table: &StringTable,
) -> String {
    let base_name = key.base_path.name_str(string_table).unwrap_or("<generic>");
    let args = key
        .arguments
        .iter()
        .map(|arg| display_type_identity_key(arg, string_table))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{base_name} of {args}")
}

fn display_type_identity_key(key: &TypeIdentityKey, string_table: &StringTable) -> String {
    match key {
        TypeIdentityKey::Builtin(builtin) => match builtin {
            BuiltinTypeKey::Bool => "Bool".to_owned(),
            BuiltinTypeKey::Int => "Int".to_owned(),
            BuiltinTypeKey::Float => "Float".to_owned(),
            BuiltinTypeKey::Decimal => "Decimal".to_owned(),
            BuiltinTypeKey::String => "String".to_owned(),
            BuiltinTypeKey::Char => "Char".to_owned(),
            BuiltinTypeKey::Range => "Range".to_owned(),
        },
        TypeIdentityKey::Nominal(path) => path
            .name_str(string_table)
            .unwrap_or("<nominal>")
            .to_owned(),
        TypeIdentityKey::External(type_id) => format!("External({})", type_id.0),
        TypeIdentityKey::Collection {
            element: inner,
            fixed_capacity,
        } => match fixed_capacity {
            Some(cap) => format!(
                "{{{cap} {}}}",
                display_type_identity_key(inner, string_table)
            ),
            None => format!("{{{}}}", display_type_identity_key(inner, string_table)),
        },
        TypeIdentityKey::Map { key, value } => {
            format!(
                "{{{key_display} = {value_display}}}",
                key_display = display_type_identity_key(key, string_table),
                value_display = display_type_identity_key(value, string_table)
            )
        }
        TypeIdentityKey::Option(inner) => {
            format!("{}?", display_type_identity_key(inner, string_table))
        }
        TypeIdentityKey::FallibleCarrier { success, error } => format_fallible_signature_parts(
            vec![display_type_identity_key(success, string_table)],
            display_type_identity_key(error, string_table),
        ),
        TypeIdentityKey::GenericInstance(instance) => {
            display_generic_instantiation_key(instance, string_table)
        }
    }
}

// -----------------------------------------------------------
//  DataType -> Identity Key Bridge
// -----------------------------------------------------------

/// Converts a diagnostic `DataType` into a stable `TypeIdentityKey`.
///
/// WHAT: diagnostic/HIR compatibility bridge for generic instance registration.
/// WHY: HIR generic struct/choice registration and parse resolution still use
///      `TypeIdentityKey` as a lowering-local key. Prefer `TypeEnvironment::type_id_to_type_identity_key`
///      for new code that starts from a canonical `TypeId`.
///
/// Returns `None` for unresolved or unsupported types (e.g., `TypeParameter`, `Inferred`).
pub fn data_type_to_type_identity_key(data_type: &DataType) -> Option<TypeIdentityKey> {
    match data_type {
        DataType::Bool => Some(TypeIdentityKey::Builtin(BuiltinTypeKey::Bool)),
        DataType::Int => Some(TypeIdentityKey::Builtin(BuiltinTypeKey::Int)),
        DataType::Float => Some(TypeIdentityKey::Builtin(BuiltinTypeKey::Float)),
        DataType::Decimal => Some(TypeIdentityKey::Builtin(BuiltinTypeKey::Decimal)),
        DataType::StringSlice => Some(TypeIdentityKey::Builtin(BuiltinTypeKey::String)),
        DataType::Char => Some(TypeIdentityKey::Builtin(BuiltinTypeKey::Char)),
        DataType::Range => Some(TypeIdentityKey::Builtin(BuiltinTypeKey::Range)),
        DataType::Struct {
            nominal_path,
            generic_instance_key: None,
            ..
        }
        | DataType::Choices {
            nominal_path,
            generic_instance_key: None,
            ..
        } => Some(TypeIdentityKey::Nominal(nominal_path.to_owned())),
        DataType::External { type_id } => Some(TypeIdentityKey::External(*type_id)),
        DataType::GenericInstance {
            base: GenericBaseType::ResolvedNominal(path),
            arguments,
        } => {
            let argument_keys = arguments
                .iter()
                .map(data_type_to_type_identity_key)
                .collect::<Option<Vec<_>>>()?;

            Some(TypeIdentityKey::GenericInstance(GenericInstantiationKey {
                base_path: path.to_owned(),
                arguments: argument_keys,
            }))
        }
        DataType::Struct {
            generic_instance_key: Some(key),
            ..
        }
        | DataType::Choices {
            generic_instance_key: Some(key),
            ..
        } => Some(TypeIdentityKey::GenericInstance(key.to_owned())),
        DataType::GenericInstance {
            base: GenericBaseType::Builtin(BuiltinGenericType::Collection { fixed_capacity }),
            arguments,
        } => match arguments.as_slice() {
            [element] => {
                data_type_to_type_identity_key(element).map(|key| TypeIdentityKey::Collection {
                    element: Box::new(key),
                    fixed_capacity: *fixed_capacity,
                })
            }
            _ => None,
        },
        DataType::GenericInstance {
            base: GenericBaseType::Builtin(BuiltinGenericType::Map),
            arguments,
        } => match arguments.as_slice() {
            [key, value] => {
                let key_id = data_type_to_type_identity_key(key)?;
                let value_id = data_type_to_type_identity_key(value)?;
                Some(TypeIdentityKey::Map {
                    key: Box::new(key_id),
                    value: Box::new(value_id),
                })
            }
            _ => None,
        },
        DataType::Option(inner) => {
            data_type_to_type_identity_key(inner).map(|key| TypeIdentityKey::Option(Box::new(key)))
        }
        DataType::FallibleCarrier { success, error } => {
            let ok_key = data_type_to_type_identity_key(success)?;
            let err_key = data_type_to_type_identity_key(error)?;
            Some(TypeIdentityKey::FallibleCarrier {
                success: Box::new(ok_key),
                error: Box::new(err_key),
            })
        }
        _ => None,
    }
}

// -----------------------------------------------------------
//  Identity Key -> TypeId Bridge
// -----------------------------------------------------------

/// Converts a `TypeIdentityKey` into canonical `TypeId` identity.
///
/// WHAT: reverse lookup from the diagnostic/HIR bridge key to the module's canonical
///       `TypeEnvironment`.
/// WHY: HIR still has to register generic nominal layouts from bridge keys, but the
///      concrete instance it registers must still be the frontend `TypeId`.
pub(crate) fn type_identity_key_to_type_id(
    key: &TypeIdentityKey,
    type_environment: &mut TypeEnvironment,
) -> Option<TypeId> {
    match key {
        TypeIdentityKey::Builtin(builtin) => Some(match builtin {
            BuiltinTypeKey::Bool => type_environment.builtins().bool,
            BuiltinTypeKey::Int => type_environment.builtins().int,
            BuiltinTypeKey::Float => type_environment.builtins().float,
            BuiltinTypeKey::Decimal => type_environment.builtins().decimal,
            BuiltinTypeKey::String => type_environment.builtins().string,
            BuiltinTypeKey::Char => type_environment.builtins().char,
            BuiltinTypeKey::Range => type_environment.builtins().range,
        }),
        TypeIdentityKey::Nominal(path) => type_environment
            .nominal_id_for_path(path)
            .and_then(|nominal_id| type_environment.type_id_for_nominal_id(nominal_id)),
        TypeIdentityKey::External(type_id) => Some(type_environment.intern_external(*type_id)),
        TypeIdentityKey::Collection {
            element: inner,
            fixed_capacity,
        } => {
            let element_id = type_identity_key_to_type_id(inner, type_environment)?;
            Some(type_environment.intern_collection(element_id, *fixed_capacity))
        }
        TypeIdentityKey::Map { key, value } => {
            let key_id = type_identity_key_to_type_id(key, type_environment)?;
            let value_id = type_identity_key_to_type_id(value, type_environment)?;
            Some(type_environment.intern_map(key_id, value_id))
        }
        TypeIdentityKey::Option(inner) => {
            let inner_id = type_identity_key_to_type_id(inner, type_environment)?;
            Some(type_environment.intern_option(inner_id))
        }
        TypeIdentityKey::FallibleCarrier { success, error } => {
            let success_id = type_identity_key_to_type_id(success, type_environment)?;
            let error_id = type_identity_key_to_type_id(error, type_environment)?;
            Some(type_environment.intern_fallible_carrier(success_id, error_id))
        }
        TypeIdentityKey::GenericInstance(instance) => {
            let nominal_id = type_environment.nominal_id_for_path(&instance.base_path)?;
            let argument_ids =
                generic_instantiation_key_argument_type_ids(instance, type_environment)?;
            Some(type_environment.intern_generic_instance(nominal_id, argument_ids))
        }
    }
}

/// Converts every argument in a generic instantiation key to canonical `TypeId`s.
///
/// WHAT: returns `None` if any argument cannot be represented in the target environment.
/// WHY: generic instances must never be interned with a silently truncated argument list.
pub(crate) fn generic_instantiation_key_argument_type_ids(
    key: &GenericInstantiationKey,
    type_environment: &mut TypeEnvironment,
) -> Option<Box<[TypeId]>> {
    key.arguments
        .iter()
        .map(|argument| type_identity_key_to_type_id(argument, type_environment))
        .collect::<Option<Vec<_>>>()
        .map(Vec::into_boxed_slice)
}
