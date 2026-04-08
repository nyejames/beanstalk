//! AST type-resolution helpers for signatures and struct fields.
//!
//! WHAT: resolves AST `NamedType` placeholders to concrete declaration-backed `DataType`s.
//! WHY: AST emission and receiver-method validation require fully resolved types up front.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, ReceiverKey};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::type_syntax::resolve_named_types_in_data_type;
use crate::return_rule_error;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Clone)]
/// Function signature after resolving all named types and receiver metadata.
pub(crate) struct ResolvedFunctionSignature {
    pub(crate) receiver: Option<ReceiverKey>,
    pub(crate) signature: FunctionSignature,
}

fn visible_declaration_by_name<'a>(
    declarations: &'a [Declaration],
    visible_declaration_ids: Option<&FxHashSet<InternedPath>>,
    name: StringId,
) -> Option<&'a Declaration> {
    declarations.iter().rfind(|declaration| {
        declaration.id.name() == Some(name)
            && match visible_declaration_ids {
                Some(visible) => visible.contains(&declaration.id),
                None => true,
            }
    })
}

/// Resolve a declaration type, replacing `NamedType` placeholders recursively.
pub(crate) fn resolve_named_signature_type(
    data_type: &DataType,
    location: &SourceLocation,
    declarations: &[Declaration],
    visible_declaration_ids: Option<&FxHashSet<InternedPath>>,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    resolve_named_types_in_data_type(
        data_type,
        location,
        &mut |type_name| {
            visible_declaration_by_name(declarations, visible_declaration_ids, type_name)
                .map(|declaration| declaration.value.data_type.to_owned())
        },
        string_table,
    )
}

/// Resolve a function signature and extract receiver metadata for method cataloging.
pub(crate) fn resolve_function_signature(
    function_path: &InternedPath,
    signature: &FunctionSignature,
    declarations: &[Declaration],
    visible_declaration_ids: Option<&FxHashSet<InternedPath>>,
    string_table: &mut StringTable,
) -> Result<ResolvedFunctionSignature, CompilerError> {
    let this_name = string_table.intern("this");
    let function_name = function_path.name_str(string_table).unwrap_or("<function>");
    let function_location = declarations
        .iter()
        .find(|declaration| declaration.id == *function_path)
        .map(|declaration| declaration.value.location.clone())
        .unwrap_or_default();

    let mut resolved_parameters = Vec::with_capacity(signature.parameters.len());
    let mut receiver = None;

    for (index, parameter) in signature.parameters.iter().enumerate() {
        let mut resolved_parameter = parameter.to_owned();
        resolved_parameter.value.data_type = resolve_named_signature_type(
            &parameter.value.data_type,
            &parameter.value.location,
            declarations,
            visible_declaration_ids,
            string_table,
        )?;

        if resolved_parameter.id.name() == Some(this_name) {
            if receiver.is_some() {
                return_rule_error!(
                    format!(
                        "Function '{}' declares 'this' more than once. Receiver parameters can only appear once.",
                        function_name
                    ),
                    parameter.value.location.clone(),
                    {
                        CompilationStage => "AST Construction",
                        PrimarySuggestion => "Keep exactly one 'this' parameter at the start of the signature",
                    }
                );
            }

            if index != 0 {
                return_rule_error!(
                    format!(
                        "Function '{}' uses 'this' as a receiver parameter, but it is not the first parameter.",
                        function_name
                    ),
                    parameter.value.location.clone(),
                    {
                        CompilationStage => "AST Construction",
                        PrimarySuggestion => "Move 'this' to the first parameter position to declare a receiver method",
                    }
                );
            }

            let Some(receiver_key) = resolved_parameter.value.data_type.receiver_key_from_type()
            else {
                return_rule_error!(
                    format!(
                        "Function '{}' uses unsupported receiver type '{}'. Receiver methods must target a user-defined struct or built-in scalar type.",
                        function_name,
                        resolved_parameter
                            .value
                            .data_type
                            .display_with_table(string_table)
                    ),
                    parameter.value.location.clone(),
                    {
                        CompilationStage => "AST Construction",
                        PrimarySuggestion => "Use a user-defined struct type or one of the supported scalar receivers: Int, Float, Bool, or String",
                    }
                );
            };

            receiver = Some(receiver_key);
        }

        resolved_parameters.push(resolved_parameter);
    }

    let mut resolved_returns = Vec::with_capacity(signature.returns.len());
    for return_slot in &signature.returns {
        let resolved_value = match &return_slot.value {
            FunctionReturn::Value(data_type) => {
                FunctionReturn::Value(resolve_named_signature_type(
                    data_type,
                    &function_location,
                    declarations,
                    visible_declaration_ids,
                    string_table,
                )?)
            }
            FunctionReturn::AliasCandidates {
                parameter_indices,
                data_type,
            } => FunctionReturn::AliasCandidates {
                parameter_indices: parameter_indices.to_owned(),
                data_type: resolve_named_signature_type(
                    data_type,
                    &function_location,
                    declarations,
                    visible_declaration_ids,
                    string_table,
                )?,
            },
        };

        resolved_returns.push(ReturnSlot {
            value: resolved_value,
            channel: return_slot.channel,
        });
    }

    Ok(ResolvedFunctionSignature {
        receiver,
        signature: FunctionSignature {
            parameters: resolved_parameters,
            returns: resolved_returns,
        },
    })
}

/// Resolve all declared struct field types against visible declarations.
pub(crate) fn resolve_struct_field_types(
    struct_path: &InternedPath,
    fields: &[Declaration],
    declarations: &[Declaration],
    visible_declaration_ids: Option<&FxHashSet<InternedPath>>,
    string_table: &StringTable,
) -> Result<Vec<Declaration>, CompilerError> {
    // WHAT: resolves field types against the declaration table visible to this struct header.
    // WHY: struct fields must enter AST/HIR in fully resolved nominal form so later phases do not
    // carry unresolved `NamedType` placeholders.
    let mut resolved_fields = Vec::with_capacity(fields.len());

    for field in fields {
        let mut resolved_field = field.to_owned();
        resolved_field.value.data_type = resolve_named_signature_type(
            &field.value.data_type,
            &field.value.location,
            declarations,
            visible_declaration_ids,
            string_table,
        )?;
        resolved_fields.push(resolved_field);
    }

    if resolved_fields.is_empty() {
        return Ok(resolved_fields);
    }

    for field in &resolved_fields {
        let Some(parent) = field.id.parent() else {
            return_rule_error!(
                "Resolved struct field is missing its parent struct path.",
                field.value.location.clone(),
                {
                    CompilationStage => "AST Construction",
                }
            );
        };

        if parent != *struct_path {
            return_rule_error!(
                "Resolved struct field parent does not match the enclosing struct declaration.",
                field.value.location.clone(),
                {
                    CompilationStage => "AST Construction",
                }
            );
        }
    }

    Ok(resolved_fields)
}

fn collect_runtime_struct_dependencies(
    data_type: &DataType,
    dependencies: &mut FxHashSet<InternedPath>,
) {
    // WHAT: extracts nominal struct dependencies from a field type recursively.
    // WHY: cycle validation only cares about runtime struct-to-struct edges, not scalar/const data.
    match data_type {
        DataType::Struct {
            nominal_path,
            const_record,
            ..
        } => {
            if !const_record {
                dependencies.insert(nominal_path.to_owned());
            }
        }
        DataType::Collection(inner, _) | DataType::Reference(inner) | DataType::Option(inner) => {
            collect_runtime_struct_dependencies(inner, dependencies)
        }
        DataType::Returns(values) => {
            for value in values {
                collect_runtime_struct_dependencies(value, dependencies);
            }
        }
        _ => {}
    }
}

/// Reject runtime struct cycles that would make concrete layout impossible to lower.
pub(crate) fn validate_no_recursive_runtime_structs(
    struct_fields_by_path: &FxHashMap<InternedPath, Vec<Declaration>>,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    // WHAT: rejects recursive runtime struct cycles.
    // WHY: v1 runtime structs do not support recursive layout semantics yet, so these cycles must
    // fail in AST construction with a targeted rule error.
    fn visit(
        current: &InternedPath,
        struct_fields_by_path: &FxHashMap<InternedPath, Vec<Declaration>>,
        string_table: &StringTable,
        visiting: &mut Vec<InternedPath>,
        visited: &mut FxHashSet<InternedPath>,
    ) -> Result<(), CompilerError> {
        if visited.contains(current) {
            return Ok(());
        }

        if let Some(index) = visiting.iter().position(|path| path == current) {
            let cycle = visiting[index..]
                .iter()
                .map(|path| path.to_string(string_table))
                .collect::<Vec<_>>()
                .join(" -> ");
            return_rule_error!(
                format!(
                    "Recursive runtime struct definitions are not supported in v1. Cycle: {cycle}"
                ),
                struct_fields_by_path
                    .get(current)
                    .and_then(|fields| fields.first())
                    .map(|field| field.value.location.clone())
                    .unwrap_or_default(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Remove the recursive runtime struct field cycle or replace it with an indirect runtime representation",
                }
            );
        }

        visiting.push(current.to_owned());

        if let Some(fields) = struct_fields_by_path.get(current) {
            for field in fields {
                let mut dependencies = FxHashSet::default();
                collect_runtime_struct_dependencies(&field.value.data_type, &mut dependencies);
                for dependency in dependencies {
                    if struct_fields_by_path.contains_key(&dependency) {
                        visit(
                            &dependency,
                            struct_fields_by_path,
                            string_table,
                            visiting,
                            visited,
                        )?;
                    }
                }
            }
        }

        visiting.pop();
        visited.insert(current.to_owned());
        Ok(())
    }

    let mut visited = FxHashSet::default();
    let mut visiting = Vec::new();
    for struct_path in struct_fields_by_path.keys() {
        visit(
            struct_path,
            struct_fields_by_path,
            string_table,
            &mut visiting,
            &mut visited,
        )?;
    }

    Ok(())
}
