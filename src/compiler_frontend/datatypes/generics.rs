//! Generic type-system substrate for frontend type resolution.
//!
//! WHAT: defines generic parameter metadata, type-identity keys, and substitution helpers.
//! WHY: generics must use structural compiler data, not stringly-typed placeholders.
//!
//! Phase 1 scope:
//! - Generic declarations and type applications parse into frontend metadata.
//! - Executable generic instantiations are still resolved before HIR in later phases.

use super::DataType;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::statements::functions::FunctionReturn;
use crate::compiler_frontend::builtins::error_type::is_reserved_builtin_symbol;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload;
use crate::compiler_frontend::external_packages::ExternalTypeId;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::identifier_policy::is_camel_case_type_name;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeParameterId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericParameter {
    pub id: TypeParameterId,
    pub name: StringId,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GenericParameterList {
    pub parameters: Vec<GenericParameter>,
}

impl GenericParameterList {
    pub(crate) fn is_empty(&self) -> bool {
        self.parameters.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.parameters.len()
    }

    pub(crate) fn contains_name(&self, name: StringId) -> bool {
        self.parameters
            .iter()
            .any(|parameter| parameter.name == name)
    }
}

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
    Collection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinTypeKey {
    Bool,
    Int,
    Float,
    Decimal,
    String,
    Char,
    ErrorKind,
    Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericInstantiationKey {
    pub base_path: InternedPath,
    pub arguments: Vec<TypeIdentityKey>,
}

#[derive(Debug, Default)]
pub(crate) struct GenericNominalInstantiationCache {
    instances: RefCell<FxHashMap<GenericInstantiationKey, DataType>>,
}

impl GenericNominalInstantiationCache {
    pub(crate) fn new() -> Self {
        Self {
            instances: RefCell::new(FxHashMap::default()),
        }
    }

    pub(crate) fn get(&self, key: &GenericInstantiationKey) -> Option<DataType> {
        self.instances.borrow().get(key).cloned()
    }

    pub(crate) fn insert(&self, key: GenericInstantiationKey, data_type: DataType) {
        self.instances.borrow_mut().insert(key, data_type);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeIdentityKey {
    Builtin(BuiltinTypeKey),
    Nominal(InternedPath),
    External(ExternalTypeId),
    Collection(Box<TypeIdentityKey>),
    Option(Box<TypeIdentityKey>),
    Result {
        ok: Box<TypeIdentityKey>,
        err: Box<TypeIdentityKey>,
    },
    GenericInstance(GenericInstantiationKey),
}

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
            BuiltinTypeKey::ErrorKind => "ErrorKind".to_owned(),
            BuiltinTypeKey::Range => "Range".to_owned(),
        },
        TypeIdentityKey::Nominal(path) => path
            .name_str(string_table)
            .unwrap_or("<nominal>")
            .to_owned(),
        TypeIdentityKey::External(type_id) => format!("External({})", type_id.0),
        TypeIdentityKey::Collection(inner) => {
            format!("{{{}}}", display_type_identity_key(inner, string_table))
        }
        TypeIdentityKey::Option(inner) => {
            format!("{}?", display_type_identity_key(inner, string_table))
        }
        TypeIdentityKey::Result { ok, err } => {
            format!(
                "Result of {}, {}",
                display_type_identity_key(ok, string_table),
                display_type_identity_key(err, string_table)
            )
        }
        TypeIdentityKey::GenericInstance(instance) => {
            display_generic_instantiation_key(instance, string_table)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct GenericParameterScope {
    parameters_by_name: FxHashMap<StringId, GenericParameter>,
}

impl GenericParameterScope {
    pub(crate) fn empty() -> Self {
        Self {
            parameters_by_name: FxHashMap::default(),
        }
    }

    pub(crate) fn from_parameter_list(
        parameter_list: &GenericParameterList,
        forbidden_names: &FxHashSet<StringId>,
        string_table: &StringTable,
        compilation_stage: &str,
    ) -> Result<Self, CompilerError> {
        let mut scope = Self::empty();

        for parameter in &parameter_list.parameters {
            if scope.parameters_by_name.contains_key(&parameter.name) {
                return Err(generic_scope_rule_error(
                    format!(
                        "Duplicate generic parameter '{}'. Parameter names must be unique.",
                        string_table.resolve(parameter.name)
                    ),
                    parameter.location.to_owned(),
                    compilation_stage,
                    "Rename one of the generic parameters so each declaration-local parameter name is unique",
                ));
            }

            if forbidden_names.contains(&parameter.name) {
                return Err(generic_scope_rule_error(
                    format!(
                        "Generic parameter '{}' collides with an existing visible type name.",
                        string_table.resolve(parameter.name)
                    ),
                    parameter.location.to_owned(),
                    compilation_stage,
                    "Choose a generic parameter name that does not collide with visible declarations, aliases, builtins, or external types",
                ));
            }

            let parameter_name = string_table.resolve(parameter.name);
            if is_reserved_generic_parameter_name(parameter_name) {
                return Err(generic_scope_rule_error(
                    format!(
                        "Generic parameter '{}' collides with a builtin type name.",
                        parameter_name
                    ),
                    parameter.location.to_owned(),
                    compilation_stage,
                    "Choose a generic parameter name that does not collide with builtin language types",
                ));
            }

            if !is_generic_parameter_name(parameter_name) {
                return Err(generic_scope_rule_error(
                    format!(
                        "Invalid generic parameter name '{}'. Generic parameter names must be PascalCase or a single uppercase letter.",
                        parameter_name
                    ),
                    parameter.location.to_owned(),
                    compilation_stage,
                    "Rename this parameter to PascalCase (for example 'ItemType') or a single uppercase letter such as 'T'",
                ));
            }

            scope
                .parameters_by_name
                .insert(parameter.name, parameter.to_owned());
        }

        Ok(scope)
    }

    pub(crate) fn resolve(&self, name: StringId) -> Option<&GenericParameter> {
        self.parameters_by_name.get(&name)
    }

    pub(crate) fn contains_name(&self, name: StringId) -> bool {
        self.parameters_by_name.contains_key(&name)
    }
}

fn generic_scope_rule_error(
    message: String,
    location: SourceLocation,
    compilation_stage: &str,
    suggestion: &str,
) -> CompilerError {
    let mut metadata = HashMap::new();
    metadata.insert(
        ErrorMetaDataKey::CompilationStage,
        compilation_stage.to_owned(),
    );
    metadata.insert(ErrorMetaDataKey::PrimarySuggestion, suggestion.to_owned());

    CompilerError::new_rule_error_with_metadata(message, location, metadata)
}

fn is_generic_parameter_name(name: &str) -> bool {
    if name.len() == 1 {
        return name
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_uppercase());
    }

    is_camel_case_type_name(name)
}

fn is_reserved_generic_parameter_name(name: &str) -> bool {
    matches!(
        name,
        "Int" | "Float" | "Bool" | "String" | "Char" | "ErrorKind"
    ) || is_reserved_builtin_symbol(name)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct TypeSubstitution {
    replacements: FxHashMap<TypeParameterId, DataType>,
}

impl TypeSubstitution {
    pub(crate) fn empty() -> Self {
        Self {
            replacements: FxHashMap::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_replacement(
        mut self,
        parameter_id: TypeParameterId,
        replacement: DataType,
    ) -> Self {
        self.replacements.insert(parameter_id, replacement);
        self
    }

    pub(crate) fn insert(&mut self, parameter_id: TypeParameterId, replacement: DataType) {
        self.replacements.insert(parameter_id, replacement);
    }

    fn replacement_for(&self, parameter_id: TypeParameterId) -> Option<&DataType> {
        self.replacements.get(&parameter_id)
    }
}

pub(crate) fn substitute_type_parameters(
    data_type: &DataType,
    substitution: &TypeSubstitution,
) -> DataType {
    match data_type {
        DataType::TypeParameter { id, .. } => substitution
            .replacement_for(*id)
            .cloned()
            .unwrap_or_else(|| data_type.to_owned()),
        DataType::GenericInstance { base, arguments } => DataType::GenericInstance {
            base: base.to_owned(),
            arguments: arguments
                .iter()
                .map(|argument| substitute_type_parameters(argument, substitution))
                .collect(),
        },
        DataType::Option(inner) => {
            DataType::Option(Box::new(substitute_type_parameters(inner, substitution)))
        }
        DataType::Result { ok, err } => DataType::Result {
            ok: Box::new(substitute_type_parameters(ok, substitution)),
            err: Box::new(substitute_type_parameters(err, substitution)),
        },
        DataType::Reference(inner) => {
            DataType::Reference(Box::new(substitute_type_parameters(inner, substitution)))
        }
        DataType::Returns(values) => DataType::Returns(
            values
                .iter()
                .map(|value| substitute_type_parameters(value, substitution))
                .collect(),
        ),
        DataType::Function(receiver, signature) => {
            let resolved_receiver = receiver
                .as_ref()
                .as_ref()
                .map(|receiver_key| receiver_key.to_owned());

            let mut resolved_signature = signature.to_owned();
            for parameter in &mut resolved_signature.parameters {
                parameter.value.data_type =
                    substitute_type_parameters(&parameter.value.data_type, substitution);
            }

            for return_slot in &mut resolved_signature.returns {
                match &mut return_slot.value {
                    FunctionReturn::Value(return_type) => {
                        *return_type = substitute_type_parameters(return_type, substitution);
                    }
                    FunctionReturn::AliasCandidates { data_type, .. } => {
                        *data_type = substitute_type_parameters(data_type, substitution);
                    }
                }
            }

            DataType::Function(Box::new(resolved_receiver), resolved_signature)
        }
        DataType::Struct {
            nominal_path,
            fields,
            const_record,
            generic_instance_key,
        } => DataType::Struct {
            nominal_path: nominal_path.to_owned(),
            fields: substitute_declaration_types(fields, substitution),
            const_record: *const_record,
            generic_instance_key: generic_instance_key.to_owned(),
        },
        DataType::Choices {
            nominal_path,
            variants,
            generic_instance_key,
        } => {
            let resolved_variants = variants
                .iter()
                .map(|variant| {
                    let payload = match &variant.payload {
                        ChoiceVariantPayload::Unit => ChoiceVariantPayload::Unit,
                        ChoiceVariantPayload::Record { fields } => ChoiceVariantPayload::Record {
                            fields: substitute_declaration_types(fields, substitution),
                        },
                    };

                    crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant {
                        id: variant.id,
                        payload,
                        location: variant.location.to_owned(),
                    }
                })
                .collect();

            DataType::Choices {
                nominal_path: nominal_path.to_owned(),
                variants: resolved_variants,
                generic_instance_key: generic_instance_key.to_owned(),
            }
        }
        DataType::Parameters(parameters) => {
            DataType::Parameters(substitute_declaration_types(parameters, substitution))
        }
        _ => data_type.to_owned(),
    }
}

fn substitute_declaration_types(
    declarations: &[Declaration],
    substitution: &TypeSubstitution,
) -> Vec<Declaration> {
    declarations
        .iter()
        .map(|declaration| {
            let mut resolved = declaration.to_owned();
            resolved.value.data_type =
                substitute_type_parameters(&declaration.value.data_type, substitution);
            resolved
        })
        .collect()
}

/// Attempts to unify a template type with a concrete type, collecting type-parameter bindings.
///
/// WHAT: recursively walks both types. When the template side is a `TypeParameter`, records
/// a binding `parameter_id -> concrete_type`. Returns `true` if unification succeeds.
///
/// WHY: generic constructor inference needs to map generic params to concrete types from
/// argument shapes and expected-type contexts.
pub fn collect_type_parameter_bindings(
    template_type: &DataType,
    concrete_type: &DataType,
    bindings: &mut FxHashMap<TypeParameterId, DataType>,
) -> bool {
    match (template_type, concrete_type) {
        // A type parameter in the template can bind to any concrete type.
        (DataType::TypeParameter { id, .. }, _) => {
            if matches!(concrete_type, DataType::Inferred) {
                return false;
            }
            // If already bound, require consistency.
            if let Some(existing) = bindings.get(id) {
                existing == concrete_type
            } else {
                bindings.insert(*id, concrete_type.clone());
                true
            }
        }

        // Recurse into matching structural shapes.
        (
            DataType::GenericInstance {
                base: GenericBaseType::Builtin(BuiltinGenericType::Collection),
                arguments: template_args,
            },
            DataType::GenericInstance {
                base: GenericBaseType::Builtin(BuiltinGenericType::Collection),
                arguments: concrete_args,
            },
        ) => generic_args_unify(template_args, concrete_args, bindings),

        (
            DataType::GenericInstance {
                base: GenericBaseType::ResolvedNominal(template_base),
                arguments: template_args,
            },
            DataType::GenericInstance {
                base: GenericBaseType::ResolvedNominal(concrete_base),
                arguments: concrete_args,
            },
        ) if template_base == concrete_base => {
            generic_args_unify(template_args, concrete_args, bindings)
        }

        (
            DataType::Struct {
                nominal_path: template_path,
                fields: template_fields,
                ..
            },
            DataType::Struct {
                nominal_path: concrete_path,
                fields: concrete_fields,
                ..
            },
        ) if template_path == concrete_path && template_fields.len() == concrete_fields.len() => {
            template_fields.iter().zip(concrete_fields.iter()).all(
                |(template_field, concrete_field)| {
                    collect_type_parameter_bindings(
                        &template_field.value.data_type,
                        &concrete_field.value.data_type,
                        bindings,
                    )
                },
            )
        }

        (DataType::Option(template_inner), DataType::Option(concrete_inner)) => {
            collect_type_parameter_bindings(template_inner, concrete_inner, bindings)
        }

        (DataType::Reference(template_inner), DataType::Reference(concrete_inner)) => {
            collect_type_parameter_bindings(template_inner, concrete_inner, bindings)
        }

        (
            DataType::Result {
                ok: t_ok,
                err: t_err,
            },
            DataType::Result {
                ok: c_ok,
                err: c_err,
            },
        ) => {
            collect_type_parameter_bindings(t_ok, c_ok, bindings)
                && collect_type_parameter_bindings(t_err, c_err, bindings)
        }

        // For everything else, require exact equality.
        _ => template_type == concrete_type,
    }
}

fn generic_args_unify(
    template_args: &[DataType],
    concrete_args: &[DataType],
    bindings: &mut FxHashMap<TypeParameterId, DataType>,
) -> bool {
    if template_args.len() != concrete_args.len() {
        return false;
    }
    template_args
        .iter()
        .zip(concrete_args.iter())
        .all(|(t, c)| collect_type_parameter_bindings(t, c, bindings))
}

/// Converts a concrete `DataType` into a stable `TypeIdentityKey`.
/// Returns `None` for unresolved or unsupported types (e.g., `TypeParameter`, `Inferred`).
pub fn data_type_to_type_identity_key(data_type: &DataType) -> Option<TypeIdentityKey> {
    match data_type {
        DataType::Bool => Some(TypeIdentityKey::Builtin(BuiltinTypeKey::Bool)),
        DataType::Int => Some(TypeIdentityKey::Builtin(BuiltinTypeKey::Int)),
        DataType::Float => Some(TypeIdentityKey::Builtin(BuiltinTypeKey::Float)),
        DataType::Decimal => Some(TypeIdentityKey::Builtin(BuiltinTypeKey::Decimal)),
        DataType::StringSlice => Some(TypeIdentityKey::Builtin(BuiltinTypeKey::String)),
        DataType::Char => Some(TypeIdentityKey::Builtin(BuiltinTypeKey::Char)),
        DataType::BuiltinErrorKind => Some(TypeIdentityKey::Builtin(BuiltinTypeKey::ErrorKind)),
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
            let arg_keys: Vec<_> = arguments
                .iter()
                .filter_map(data_type_to_type_identity_key)
                .collect();
            if arg_keys.len() != arguments.len() {
                return None;
            }
            Some(TypeIdentityKey::GenericInstance(GenericInstantiationKey {
                base_path: path.to_owned(),
                arguments: arg_keys,
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
            base: GenericBaseType::Builtin(BuiltinGenericType::Collection),
            arguments,
        } if let [element] = arguments.as_slice() => data_type_to_type_identity_key(element)
            .map(|key| TypeIdentityKey::Collection(Box::new(key))),
        DataType::Option(inner) => {
            data_type_to_type_identity_key(inner).map(|key| TypeIdentityKey::Option(Box::new(key)))
        }
        DataType::Result { ok, err } => {
            let ok_key = data_type_to_type_identity_key(ok)?;
            let err_key = data_type_to_type_identity_key(err)?;
            Some(TypeIdentityKey::Result {
                ok: Box::new(ok_key),
                err: Box::new(err_key),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location() -> SourceLocation {
        SourceLocation::default()
    }

    #[test]
    fn generic_scope_rejects_duplicate_parameter_names() {
        let mut string_table = StringTable::new();
        let name = string_table.intern("T");
        let parameter = GenericParameter {
            id: TypeParameterId(0),
            name,
            location: location(),
        };
        let list = GenericParameterList {
            parameters: vec![parameter.clone(), parameter],
        };

        let error = GenericParameterScope::from_parameter_list(
            &list,
            &FxHashSet::default(),
            &string_table,
            "AST Construction",
        )
        .expect_err("duplicate generic names should fail");

        assert!(error.msg.contains("Duplicate generic parameter"));
    }

    #[test]
    fn generic_scope_rejects_collisions_with_forbidden_names() {
        let mut string_table = StringTable::new();
        let name = string_table.intern("Item");
        let list = GenericParameterList {
            parameters: vec![GenericParameter {
                id: TypeParameterId(0),
                name,
                location: location(),
            }],
        };
        let mut forbidden = FxHashSet::default();
        forbidden.insert(name);

        let error = GenericParameterScope::from_parameter_list(
            &list,
            &forbidden,
            &string_table,
            "AST Construction",
        )
        .expect_err("generic collisions should fail");

        assert!(
            error
                .msg
                .contains("collides with an existing visible type name")
        );
    }

    #[test]
    fn generic_scope_rejects_non_type_style_parameter_names() {
        let mut string_table = StringTable::new();
        let list = GenericParameterList {
            parameters: vec![GenericParameter {
                id: TypeParameterId(0),
                name: string_table.intern("item_type"),
                location: location(),
            }],
        };

        let error = GenericParameterScope::from_parameter_list(
            &list,
            &FxHashSet::default(),
            &string_table,
            "AST Construction",
        )
        .expect_err("non-type-style names should fail");

        assert!(
            error
                .msg
                .contains("must be PascalCase or a single uppercase letter")
        );
    }

    #[test]
    fn generic_scope_accepts_pascal_case_and_single_uppercase_names() {
        let mut string_table = StringTable::new();
        let item_name = string_table.intern("ItemType");
        let t_name = string_table.intern("T");
        let list = GenericParameterList {
            parameters: vec![
                GenericParameter {
                    id: TypeParameterId(0),
                    name: item_name,
                    location: location(),
                },
                GenericParameter {
                    id: TypeParameterId(1),
                    name: t_name,
                    location: location(),
                },
            ],
        };

        let scope = GenericParameterScope::from_parameter_list(
            &list,
            &FxHashSet::default(),
            &string_table,
            "AST Construction",
        )
        .expect("valid generic names should be accepted");

        assert!(scope.contains_name(item_name));
        assert!(scope.contains_name(t_name));
    }

    #[test]
    fn type_identity_keys_distinguish_nominal_generic_arguments() {
        let mut string_table = StringTable::new();
        let box_path = InternedPath::from_single_str("Box", &mut string_table);
        let pair_path = InternedPath::from_single_str("Pair", &mut string_table);
        let int_key = TypeIdentityKey::Builtin(BuiltinTypeKey::Int);
        let string_key = TypeIdentityKey::Builtin(BuiltinTypeKey::String);

        let int_instance = TypeIdentityKey::GenericInstance(GenericInstantiationKey {
            base_path: box_path.to_owned(),
            arguments: vec![int_key.to_owned()],
        });
        let another_int_instance = TypeIdentityKey::GenericInstance(GenericInstantiationKey {
            base_path: box_path.to_owned(),
            arguments: vec![int_key],
        });
        let string_instance = TypeIdentityKey::GenericInstance(GenericInstantiationKey {
            base_path: box_path,
            arguments: vec![string_key],
        });
        let pair_int_string = TypeIdentityKey::GenericInstance(GenericInstantiationKey {
            base_path: pair_path.to_owned(),
            arguments: vec![
                TypeIdentityKey::Builtin(BuiltinTypeKey::Int),
                TypeIdentityKey::Builtin(BuiltinTypeKey::String),
            ],
        });
        let pair_string_int = TypeIdentityKey::GenericInstance(GenericInstantiationKey {
            base_path: pair_path,
            arguments: vec![
                TypeIdentityKey::Builtin(BuiltinTypeKey::String),
                TypeIdentityKey::Builtin(BuiltinTypeKey::Int),
            ],
        });

        assert_eq!(int_instance, another_int_instance);
        assert_ne!(int_instance, string_instance);
        assert_ne!(pair_int_string, pair_string_int);
    }

    #[test]
    fn substitution_replaces_type_parameters_in_nested_types() {
        let mut string_table = StringTable::new();
        let t_name = string_table.intern("T");
        let t = DataType::TypeParameter {
            id: TypeParameterId(0),
            name: t_name,
        };

        let substitution =
            TypeSubstitution::empty().with_replacement(TypeParameterId(0), DataType::Int);

        let collection = DataType::collection(t.to_owned());
        let optional = DataType::Option(Box::new(t.to_owned()));
        let result = DataType::Result {
            ok: Box::new(t.to_owned()),
            err: Box::new(DataType::NamedType(string_table.intern("Error"))),
        };

        assert_eq!(substitute_type_parameters(&t, &substitution), DataType::Int);
        assert_eq!(
            substitute_type_parameters(&collection, &substitution),
            DataType::collection(DataType::Int)
        );
        assert_eq!(
            substitute_type_parameters(&optional, &substitution),
            DataType::Option(Box::new(DataType::Int))
        );
        assert_eq!(
            substitute_type_parameters(&result, &substitution),
            DataType::Result {
                ok: Box::new(DataType::Int),
                err: Box::new(DataType::NamedType(string_table.intern("Error"))),
            }
        );
    }

    #[test]
    fn substitution_replaces_generic_instance_arguments() {
        let mut string_table = StringTable::new();
        let box_name = string_table.intern("Box");
        let pair_name = string_table.intern("Pair");
        let t_name = string_table.intern("T");
        let u_name = string_table.intern("U");
        let parameter = DataType::TypeParameter {
            id: TypeParameterId(0),
            name: t_name,
        };
        let other_parameter = DataType::TypeParameter {
            id: TypeParameterId(1),
            name: u_name,
        };

        let generic_box = DataType::GenericInstance {
            base: GenericBaseType::Named(box_name),
            arguments: vec![parameter],
        };
        let generic_pair = DataType::GenericInstance {
            base: GenericBaseType::Named(pair_name),
            arguments: vec![
                other_parameter,
                DataType::TypeParameter {
                    id: TypeParameterId(0),
                    name: t_name,
                },
            ],
        };

        let substitution = TypeSubstitution::empty()
            .with_replacement(TypeParameterId(0), DataType::StringSlice)
            .with_replacement(TypeParameterId(1), DataType::Int);

        assert_eq!(
            substitute_type_parameters(&generic_box, &substitution),
            DataType::GenericInstance {
                base: GenericBaseType::Named(box_name),
                arguments: vec![DataType::StringSlice],
            }
        );
        assert_eq!(
            substitute_type_parameters(&generic_pair, &substitution),
            DataType::GenericInstance {
                base: GenericBaseType::Named(pair_name),
                arguments: vec![DataType::Int, DataType::StringSlice],
            }
        );
    }

    #[test]
    fn generic_display_uses_beanstalk_surface_style() {
        let mut string_table = StringTable::new();
        let t_name = string_table.intern("T");
        let box_name = string_table.intern("Box");
        let pair_name = string_table.intern("Pair");
        let error_name = string_table.intern("Error");

        let t = DataType::TypeParameter {
            id: TypeParameterId(0),
            name: t_name,
        };
        let box_of_int = DataType::GenericInstance {
            base: GenericBaseType::Named(box_name),
            arguments: vec![DataType::Int],
        };
        let pair_of_string_int = DataType::GenericInstance {
            base: GenericBaseType::Named(pair_name),
            arguments: vec![DataType::StringSlice, DataType::Int],
        };
        let collection_of_box_string = DataType::collection(DataType::GenericInstance {
            base: GenericBaseType::Named(box_name),
            arguments: vec![DataType::StringSlice],
        });
        let optional_box_int = DataType::Option(Box::new(box_of_int.to_owned()));
        let result_of_box_int_and_error = DataType::Result {
            ok: Box::new(box_of_int.to_owned()),
            err: Box::new(DataType::NamedType(error_name)),
        };

        assert_eq!(t.display_with_table(&string_table), "T");
        assert_eq!(box_of_int.display_with_table(&string_table), "Box of Int");
        assert_eq!(
            pair_of_string_int.display_with_table(&string_table),
            "Pair of String, Int"
        );
        assert_eq!(
            collection_of_box_string.display_with_table(&string_table),
            "{Box of String}"
        );
        assert_eq!(
            optional_box_int.display_with_table(&string_table),
            "Box of Int?"
        );
        assert_eq!(
            result_of_box_int_and_error.display_with_table(&string_table),
            "Result of Box of Int, Error"
        );
    }
}
