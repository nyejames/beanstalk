//! AST type-resolution helpers for signatures and struct fields.
//!
//! WHAT: resolves AST `NamedType` placeholders to concrete declaration-backed `DataType`s.
//! WHY: AST emission and receiver-method validation require fully resolved types up front.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::eval_expression::evaluate_expression;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::{ContextKind, ScopeContext, TopLevelDeclarationIndex};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::generics::{
    GenericBaseType, GenericParameterList, GenericParameterScope, TypeParameterId,
};
use crate::compiler_frontend::datatypes::{DataType, ReceiverKey};
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeResolutionContext, resolve_type,
};
use crate::compiler_frontend::external_packages::{ExternalPackageRegistry, ExternalSymbolId};
use crate::compiler_frontend::headers::module_symbols::{
    GenericDeclarationKind, GenericDeclarationMetadata,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_rule_error;
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

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
    declarations.iter().find(|declaration| {
        declaration.id.name() == Some(name)
            && match visible_declaration_ids {
                Some(visible) => visible.contains(&declaration.id),
                None => true,
            }
    })
}

/// Resolve a declaration type with the shared type-resolution context.
pub(crate) fn resolve_named_signature_type(
    data_type: &DataType,
    location: &SourceLocation,
    type_resolution_context: &TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    resolve_type(data_type, location, type_resolution_context, string_table)
}

pub(crate) fn build_generic_parameter_scope(
    generic_parameters: &GenericParameterList,
    visible_source_bindings: &FxHashMap<StringId, InternedPath>,
    visible_type_aliases: &FxHashMap<StringId, InternedPath>,
    visible_external_symbols: &FxHashMap<StringId, ExternalSymbolId>,
    declarations: &[Declaration],
    generic_declarations_by_path: &FxHashMap<InternedPath, GenericDeclarationMetadata>,
    string_table: &StringTable,
) -> Result<Option<GenericParameterScope>, CompilerError> {
    if generic_parameters.is_empty() {
        return Ok(None);
    }

    let mut forbidden_names = FxHashSet::default();
    forbidden_names.extend(visible_type_aliases.keys().copied());

    for (name, symbol_id) in visible_external_symbols {
        if matches!(symbol_id, ExternalSymbolId::Type(_)) {
            forbidden_names.insert(*name);
        }
    }

    for (name, path) in visible_source_bindings {
        if path_is_visible_type(path, declarations, generic_declarations_by_path) {
            forbidden_names.insert(*name);
        }
    }

    GenericParameterScope::from_parameter_list(
        generic_parameters,
        &forbidden_names,
        string_table,
        "AST Construction",
    )
    .map(Some)
}

fn path_is_visible_type(
    path: &InternedPath,
    declarations: &[Declaration],
    generic_declarations_by_path: &FxHashMap<InternedPath, GenericDeclarationMetadata>,
) -> bool {
    if let Some(metadata) = generic_declarations_by_path.get(path) {
        return matches!(
            metadata.kind,
            GenericDeclarationKind::Struct
                | GenericDeclarationKind::Choice
                | GenericDeclarationKind::TypeAlias
        );
    }

    declarations
        .iter()
        .find(|declaration| declaration.id == *path)
        .is_some_and(|declaration| {
            matches!(
                declaration.value.data_type,
                DataType::Struct { .. } | DataType::Choices { .. }
            )
        })
}

pub(crate) fn validate_generic_parameters_used(
    generic_parameters: &GenericParameterList,
    used_parameters: &FxHashSet<TypeParameterId>,
    declaration_path: &InternedPath,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for parameter in &generic_parameters.parameters {
        if !used_parameters.contains(&parameter.id) {
            let declaration_name = declaration_path
                .name_str(string_table)
                .unwrap_or("<declaration>");
            return_rule_error!(
                format!(
                    "Generic parameter '{}' is declared but never used in the public type shape for '{}'.",
                    string_table.resolve(parameter.name),
                    declaration_name
                ),
                location.to_owned(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Remove the unused generic parameter or use it in a parameter, return, field, or variant payload type",
                }
            );
        }
    }

    Ok(())
}

pub(crate) fn collect_type_parameter_ids_from_type(
    data_type: &DataType,
    used_parameters: &mut FxHashSet<TypeParameterId>,
) {
    match data_type {
        DataType::TypeParameter { id, .. } => {
            used_parameters.insert(*id);
        }
        DataType::GenericInstance { arguments, .. } => {
            for argument in arguments {
                collect_type_parameter_ids_from_type(argument, used_parameters);
            }
        }
        DataType::Option(inner) | DataType::Reference(inner) => {
            collect_type_parameter_ids_from_type(inner, used_parameters)
        }
        DataType::Result { ok, err } => {
            collect_type_parameter_ids_from_type(ok, used_parameters);
            collect_type_parameter_ids_from_type(err, used_parameters);
        }
        DataType::Returns(values) => {
            for value in values {
                collect_type_parameter_ids_from_type(value, used_parameters);
            }
        }
        DataType::Function(_, signature) => {
            for parameter in &signature.parameters {
                collect_type_parameter_ids_from_type(&parameter.value.data_type, used_parameters);
            }
            for return_slot in &signature.returns {
                collect_type_parameter_ids_from_type(return_slot.data_type(), used_parameters);
            }
        }
        DataType::Struct { fields, .. } | DataType::Parameters(fields) => {
            collect_type_parameter_ids_from_declarations(fields, used_parameters);
        }
        DataType::Choices { variants, .. } => {
            collect_type_parameter_ids_from_choice_variants(variants, used_parameters);
        }
        _ => {}
    }
}

pub(crate) fn collect_type_parameter_ids_from_declarations(
    declarations: &[Declaration],
    used_parameters: &mut FxHashSet<TypeParameterId>,
) {
    for declaration in declarations {
        collect_type_parameter_ids_from_type(&declaration.value.data_type, used_parameters);
    }
}

pub(crate) fn collect_type_parameter_ids_from_choice_variants(
    variants: &[crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant],
    used_parameters: &mut FxHashSet<TypeParameterId>,
) {
    use crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload;

    for variant in variants {
        if let ChoiceVariantPayload::Record { fields } = &variant.payload {
            collect_type_parameter_ids_from_declarations(fields, used_parameters);
        }
    }
}

pub(crate) fn validate_no_recursive_generic_type(
    declaration_path: &InternedPath,
    data_type: &DataType,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if generic_type_references_nominal_path(data_type, declaration_path) {
        let declaration_name = declaration_path
            .name_str(string_table)
            .unwrap_or("<generic type>");
        return_rule_error!(
            format!(
                "Recursive generic types are not supported yet. Generic type '{declaration_name}' cannot contain itself."
            ),
            location.to_owned(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Use a non-recursive shape or split the recursive storage behind a future indirection type",
            }
        );
    }

    Ok(())
}

fn generic_type_references_nominal_path(
    data_type: &DataType,
    declaration_path: &InternedPath,
) -> bool {
    match data_type {
        DataType::GenericInstance { base, arguments } => {
            let base_matches = matches!(
                base,
                GenericBaseType::ResolvedNominal(path) if path == declaration_path
            );
            base_matches
                || arguments.iter().any(|argument| {
                    generic_type_references_nominal_path(argument, declaration_path)
                })
        }
        DataType::Option(inner) | DataType::Reference(inner) => {
            generic_type_references_nominal_path(inner, declaration_path)
        }
        DataType::Result { ok, err } => {
            generic_type_references_nominal_path(ok, declaration_path)
                || generic_type_references_nominal_path(err, declaration_path)
        }
        DataType::Returns(values) => values
            .iter()
            .any(|value| generic_type_references_nominal_path(value, declaration_path)),
        DataType::Function(_, signature) => {
            signature.parameters.iter().any(|parameter| {
                generic_type_references_nominal_path(&parameter.value.data_type, declaration_path)
            }) || signature.returns.iter().any(|return_slot| {
                generic_type_references_nominal_path(return_slot.data_type(), declaration_path)
            })
        }
        DataType::Struct { fields, .. } | DataType::Parameters(fields) => {
            fields.iter().any(|field| {
                generic_type_references_nominal_path(&field.value.data_type, declaration_path)
            })
        }
        DataType::Choices { variants, .. } => {
            use crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload;

            variants.iter().any(|variant| match &variant.payload {
                ChoiceVariantPayload::Unit => false,
                ChoiceVariantPayload::Record { fields } => fields.iter().any(|field| {
                    generic_type_references_nominal_path(&field.value.data_type, declaration_path)
                }),
            })
        }
        _ => false,
    }
}

/// Resolve a function signature and extract receiver metadata for method cataloging.
pub(crate) fn resolve_function_signature(
    function_path: &InternedPath,
    signature: &FunctionSignature,
    type_resolution_context: &TypeResolutionContext<'_>,
    string_table: &mut StringTable,
) -> Result<ResolvedFunctionSignature, CompilerError> {
    let this_name = string_table.intern("this");
    let function_name = function_path.name_str(string_table).unwrap_or("<function>");
    let function_location = type_resolution_context
        .declarations
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
            type_resolution_context,
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
                if resolved_parameter
                    .value
                    .data_type
                    .is_resolved_generic_nominal_instance()
                    || resolved_parameter
                        .value
                        .data_type
                        .is_unresolved_generic_application()
                {
                    return_rule_error!(
                        format!(
                            "Function '{}' uses generic receiver type '{}'. Receiver methods on generic types are not supported yet.",
                            function_name,
                            resolved_parameter
                                .value
                                .data_type
                                .display_with_table(string_table)
                        ),
                        parameter.value.location.clone(),
                        {
                            CompilationStage => "AST Construction",
                            PrimarySuggestion => "Use a free function that accepts the generic value as a normal parameter",
                        }
                    );
                }

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
                    type_resolution_context,
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
                    type_resolution_context,
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
    type_resolution_context: &TypeResolutionContext<'_>,
    string_table: &mut StringTable,
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
            type_resolution_context,
            string_table,
        )?;
        resolved_field.value = inline_visible_constant_references(
            &resolved_field.value,
            type_resolution_context.declarations,
            type_resolution_context.visible_declaration_ids,
            string_table,
        )?;
        if !matches!(resolved_field.value.kind, ExpressionKind::NoValue)
            && !resolved_field.value.is_compile_time_constant()
        {
            let field_name = resolved_field
                .id
                .name_str(string_table)
                .unwrap_or("<field>");
            return_rule_error!(
                format!(
                    "Struct field '{}' default value must fold to a single compile-time value.",
                    field_name
                ),
                resolved_field.value.location.clone(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Use only compile-time constants and constant expressions in struct default values",
                }
            );
        }
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

/// Resolve choice payload field types, replacing `NamedType` placeholders in record variants.
pub(crate) fn resolve_choice_variant_payload_types(
    variants: &[crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant],
    type_resolution_context: &TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> Result<Vec<crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant>, CompilerError>
{
    use crate::compiler_frontend::declaration_syntax::choice::{
        ChoiceVariant, ChoiceVariantPayload,
    };

    let mut resolved_variants = Vec::with_capacity(variants.len());
    for variant in variants {
        let payload = match &variant.payload {
            ChoiceVariantPayload::Unit => ChoiceVariantPayload::Unit,
            ChoiceVariantPayload::Record { fields } => {
                let mut resolved_fields = Vec::with_capacity(fields.len());
                for field in fields {
                    let mut resolved_field = field.to_owned();
                    resolved_field.value.data_type = resolve_named_signature_type(
                        &field.value.data_type,
                        &field.value.location,
                        type_resolution_context,
                        string_table,
                    )?;
                    resolved_fields.push(resolved_field);
                }
                ChoiceVariantPayload::Record {
                    fields: resolved_fields,
                }
            }
        };
        resolved_variants.push(ChoiceVariant {
            id: variant.id,
            payload,
            location: variant.location.clone(),
        });
    }
    Ok(resolved_variants)
}

fn inline_visible_constant_references(
    expression: &Expression,
    declarations: &[Declaration],
    visible_declaration_ids: Option<&FxHashSet<InternedPath>>,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    inline_visible_constant_references_impl(
        expression,
        declarations,
        visible_declaration_ids,
        string_table,
    )
}

fn inline_visible_constant_references_impl(
    expression: &Expression,
    declarations: &[Declaration],
    visible_declaration_ids: Option<&FxHashSet<InternedPath>>,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    match &expression.kind {
        ExpressionKind::Reference(path) => Ok(declarations
            .iter()
            .find(|declaration| {
                declaration.id == *path
                    && !declaration.is_unresolved_constant_placeholder()
                    && declaration.value.is_compile_time_constant()
                    && match visible_declaration_ids {
                        Some(visible) => visible.contains(&declaration.id),
                        None => true,
                    }
            })
            .or_else(|| {
                path.name().and_then(|name| {
                    visible_declaration_by_name(declarations, visible_declaration_ids, name).filter(
                        |declaration| {
                            !declaration.is_unresolved_constant_placeholder()
                                && declaration.value.is_compile_time_constant()
                        },
                    )
                })
            })
            .map(|declaration| {
                let mut resolved = declaration.value.to_owned();
                resolved.location = expression.location.clone();
                resolved
            })
            .unwrap_or_else(|| expression.to_owned())),
        ExpressionKind::Runtime(nodes) => {
            let mut rewritten_nodes = Vec::with_capacity(nodes.len());
            for node in nodes {
                rewritten_nodes.push(inline_visible_constant_references_in_node(
                    node,
                    declarations,
                    visible_declaration_ids,
                    string_table,
                )?);
            }

            let mut current_type = expression.data_type.to_owned();
            let mut evaluation_context = ScopeContext::new(
                ContextKind::ConstantHeader,
                expression.location.scope.to_owned(),
                Rc::new(TopLevelDeclarationIndex::new(declarations.to_vec())),
                ExternalPackageRegistry::new(),
                Vec::new(),
            );
            if let Some(visible) = visible_declaration_ids {
                evaluation_context.visible_declaration_ids = Some(visible.to_owned());
            }

            evaluate_expression(
                &evaluation_context,
                rewritten_nodes,
                &mut current_type,
                &expression.value_mode,
                string_table,
            )
            .map_err(|error| {
                CompilerError::new_rule_error(
                    format!(
                        "Failed to fold struct field default value after inlining constants: {}",
                        error.msg
                    ),
                    expression.location.clone(),
                )
            })
        }
        ExpressionKind::Collection(items) => {
            let mut resolved_items = Vec::with_capacity(items.len());
            for item in items {
                resolved_items.push(inline_visible_constant_references_impl(
                    item,
                    declarations,
                    visible_declaration_ids,
                    string_table,
                )?);
            }
            Ok(Expression::new(
                ExpressionKind::Collection(resolved_items),
                expression.location.clone(),
                expression.data_type.to_owned(),
                expression.value_mode.to_owned(),
            ))
        }
        ExpressionKind::StructInstance(fields) => {
            let mut resolved_fields = Vec::with_capacity(fields.len());
            for field in fields {
                resolved_fields.push(Declaration {
                    id: field.id.to_owned(),
                    value: inline_visible_constant_references_impl(
                        &field.value,
                        declarations,
                        visible_declaration_ids,
                        string_table,
                    )?,
                });
            }
            Ok(Expression::new(
                ExpressionKind::StructInstance(resolved_fields),
                expression.location.clone(),
                expression.data_type.to_owned(),
                expression.value_mode.to_owned(),
            ))
        }
        ExpressionKind::Range(start, end) => Ok(Expression::new(
            ExpressionKind::Range(
                Box::new(inline_visible_constant_references(
                    start,
                    declarations,
                    visible_declaration_ids,
                    string_table,
                )?),
                Box::new(inline_visible_constant_references(
                    end,
                    declarations,
                    visible_declaration_ids,
                    string_table,
                )?),
            ),
            expression.location.clone(),
            expression.data_type.to_owned(),
            expression.value_mode.to_owned(),
        )),
        ExpressionKind::ResultConstruct { variant, value } => Ok(Expression::new(
            ExpressionKind::ResultConstruct {
                variant: *variant,
                value: Box::new(inline_visible_constant_references(
                    value,
                    declarations,
                    visible_declaration_ids,
                    string_table,
                )?),
            },
            expression.location.clone(),
            expression.data_type.to_owned(),
            expression.value_mode.to_owned(),
        )),
        ExpressionKind::Coerced { value, to_type } => Ok(Expression::new(
            ExpressionKind::Coerced {
                value: Box::new(inline_visible_constant_references(
                    value,
                    declarations,
                    visible_declaration_ids,
                    string_table,
                )?),
                to_type: to_type.to_owned(),
            },
            expression.location.clone(),
            expression.data_type.to_owned(),
            expression.value_mode.to_owned(),
        )),
        _ => Ok(expression.to_owned()),
    }
}

fn inline_visible_constant_references_in_node(
    node: &AstNode,
    declarations: &[Declaration],
    visible_declaration_ids: Option<&FxHashSet<InternedPath>>,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let mut rewritten = node.to_owned();
    rewritten.kind = match &node.kind {
        NodeKind::Rvalue(expression) => NodeKind::Rvalue(inline_visible_constant_references_impl(
            expression,
            declarations,
            visible_declaration_ids,
            string_table,
        )?),
        NodeKind::VariableDeclaration(declaration) => NodeKind::VariableDeclaration(Declaration {
            id: declaration.id.to_owned(),
            value: inline_visible_constant_references_impl(
                &declaration.value,
                declarations,
                visible_declaration_ids,
                string_table,
            )?,
        }),
        _ => node.kind.to_owned(),
    };
    Ok(rewritten)
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
            const_record: false,
            ..
        } => {
            dependencies.insert(nominal_path.to_owned());
        }
        DataType::Reference(inner) | DataType::Option(inner) => {
            collect_runtime_struct_dependencies(inner, dependencies)
        }
        DataType::Result { ok, err } => {
            collect_runtime_struct_dependencies(ok, dependencies);
            collect_runtime_struct_dependencies(err, dependencies);
        }
        DataType::GenericInstance { arguments, .. } => {
            for argument in arguments {
                collect_runtime_struct_dependencies(argument, dependencies);
            }
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
