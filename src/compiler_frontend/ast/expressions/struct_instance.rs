use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallDiagnosticContext, expectations_from_struct_fields, resolve_call_arguments,
};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::function_calls::parse_call_arguments;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::generics::{
    GenericInstantiationKey, TypeParameterId, TypeSubstitution, collect_type_parameter_bindings,
    data_type_to_type_identity_key, substitute_type_parameters,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::numeric::coerce_expression_to_declared_type;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::{return_compiler_error, return_rule_error};
use rustc_hash::FxHashMap;

/// Parse `StructName(...)` and return a finalized struct instance expression.
///
/// WHAT:
/// - Parses constructor arguments (positional and named) using the shared call-argument model.
/// - Validates arity, named-target lookup, duplicate detection, positional-before-named ordering,
///   default filling, missing required-field detection, and per-field type compatibility.
/// - Fills trailing fields from struct defaults when arguments are omitted.
/// - Produces a canonical `Expression::struct_instance` with definition-order fields.
///
/// WHY:
/// - Constructor syntax is syntactically identical to function call syntax; sharing the same
///   argument-resolution machinery keeps the two forms consistent and avoids a parallel
///   resolution system.
/// - Const-record coercion for top-level `#` constants is applied after resolution.
pub(crate) fn parse_struct_constructor_expression(
    token_stream: &mut FileTokens,
    struct_path: &InternedPath,
    struct_name: StringId,
    fields: &[Declaration],
    struct_value_mode: &ValueMode,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let constructor_location = token_stream.current_location();
    let struct_name_str = string_table.resolve(struct_name).to_owned();

    // The stream is positioned on the struct symbol when called.
    // Advance past it to '(' so parse_call_arguments can take over.
    token_stream.advance();
    if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
        return_compiler_error!("Struct constructor parser called without an opening parenthesis");
    }

    let raw_args = parse_call_arguments(token_stream, context, string_table)?;

    // --- Generic struct constructor inference ---
    // If the struct is a generic declaration, infer type arguments from the expected type
    // context and constructor arguments, then substitute before validating.
    let (fields, generic_instance_key) = if let Some(generic_decls) =
        &context.generic_declarations_by_path
        && let Some(metadata) = generic_decls.get(struct_path)
        && !metadata.parameters.is_empty()
    {
        let mut bindings: FxHashMap<TypeParameterId, DataType> = FxHashMap::default();

        // 1. Collect bindings from expected type(s).
        for expected in &context.expected_result_types {
            match expected {
                DataType::Struct {
                    nominal_path,
                    fields: expected_fields,
                    ..
                } if nominal_path == struct_path && expected_fields.len() == fields.len() => {
                    for (template_field, expected_field) in
                        fields.iter().zip(expected_fields.iter())
                    {
                        let _ = collect_type_parameter_bindings(
                            &template_field.value.data_type,
                            &expected_field.value.data_type,
                            &mut bindings,
                        );
                    }
                }
                DataType::GenericInstance {
                    base:
                        crate::compiler_frontend::datatypes::generics::GenericBaseType::ResolvedNominal(
                            path,
                        ),
                    arguments,
                } if path == struct_path => {
                    for (param, arg) in metadata.parameters.parameters.iter().zip(arguments.iter())
                    {
                        let _ = collect_type_parameter_bindings(
                            &DataType::TypeParameter {
                                id: param.id,
                                name: param.name,
                            },
                            arg,
                            &mut bindings,
                        );
                    }
                }
                _ => {}
            }
        }

        // 2. Collect bindings from constructor arguments.
        for (template_field, arg) in fields.iter().zip(raw_args.iter()) {
            let arg_type = &arg.value.data_type;
            let _ = collect_type_parameter_bindings(
                &template_field.value.data_type,
                arg_type,
                &mut bindings,
            );
        }

        // 3. Build concrete arguments in parameter order.
        let mut concrete_args = Vec::with_capacity(metadata.parameters.len());
        let mut missing_params = Vec::new();
        for param in &metadata.parameters.parameters {
            if let Some(concrete) = bindings.get(&param.id).cloned() {
                concrete_args.push(concrete);
            } else {
                missing_params.push(string_table.resolve(param.name).to_owned());
            }
        }

        if !missing_params.is_empty() {
            return_rule_error!(
                format!(
                    "Cannot infer type argument(s) for generic struct '{}': {}. Provide an explicit type annotation or constructor arguments with concrete types.",
                    struct_name_str,
                    missing_params.join(", ")
                ),
                constructor_location.clone(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Add an explicit type annotation (e.g. 'Box of Int = Box(...)' ) or use arguments with unambiguous types",
                }
            );
        }

        // 4. Substitute into template fields.
        let mut substitution = TypeSubstitution::empty();
        for (param, arg) in metadata
            .parameters
            .parameters
            .iter()
            .zip(concrete_args.iter())
        {
            substitution.insert(param.id, arg.clone());
        }
        let instantiated_fields: Vec<Declaration> = fields
            .iter()
            .map(|field| {
                let mut resolved = field.clone();
                resolved.value.data_type =
                    substitute_type_parameters(&field.value.data_type, &substitution);
                resolved
            })
            .collect();

        // 5. Build generic instance key.
        let arg_keys: Vec<_> = concrete_args
            .iter()
            .filter_map(data_type_to_type_identity_key)
            .collect();
        let key = if arg_keys.len() == concrete_args.len() {
            Some(GenericInstantiationKey {
                base_path: struct_path.to_owned(),
                arguments: arg_keys,
            })
        } else {
            None
        };

        (instantiated_fields, key)
    } else {
        (fields.to_owned(), None)
    };

    let expectations = expectations_from_struct_fields(&fields);
    let resolved_args = resolve_call_arguments(
        CallDiagnosticContext::struct_constructor(&struct_name_str),
        &raw_args,
        &expectations,
        constructor_location.clone(),
        string_table,
    )?;

    let enforce_const_record = context.kind.allows_const_record_coercion();
    let mut struct_fields = Vec::with_capacity(fields.len());

    for (field, arg) in fields.iter().zip(resolved_args.iter()) {
        let field_type = &field.value.data_type;
        // Apply contextual numeric coercion (Int → Float) post-resolution, consistent with
        // declaration sites. resolve_call_arguments has already validated type compatibility.
        let mut value = coerce_expression_to_declared_type(arg.value.clone(), field_type);

        if enforce_const_record {
            // Allow unresolved constant placeholders to pass through so the fixed-point
            // loop can retry after dependencies are resolved.
            let is_deferred_reference = if let ExpressionKind::Reference(path) = &value.kind
                && path.name().is_some_and(|name| {
                    context
                        .get_reference(&name)
                        .is_some_and(Declaration::is_unresolved_constant_placeholder)
                }) {
                true
            } else {
                false
            };

            if !value.is_compile_time_constant() && !is_deferred_reference {
                let field_name = field.id.name_str(string_table).unwrap_or("<field>");
                return_rule_error!(
                    format!(
                        "Const struct coercion requires compile-time field values. Field '{}' in '{}' is not compile-time constant.",
                        field_name,
                        struct_name_str
                    ),
                    value.location,
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Use only compile-time values when constructing structs for top-level '#' constants",
                    }
                );
            }
            // Const records are data-only exports, so mutable ownership is
            // removed to keep constant semantics explicit in later stages.
            value.value_mode = ValueMode::ImmutableOwned;
        }

        struct_fields.push(Declaration {
            id: field.id.to_owned(),
            value,
        });
    }

    let instance_ownership = if enforce_const_record {
        ValueMode::ImmutableOwned
    } else {
        struct_value_mode.as_owned()
    };

    Ok(Expression::struct_instance(
        struct_path.to_owned(),
        struct_fields,
        constructor_location,
        instance_ownership,
        enforce_const_record,
        generic_instance_key,
    ))
}
