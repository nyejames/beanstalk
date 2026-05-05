use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallDiagnosticContext, expectations_from_struct_fields, resolve_call_arguments,
};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::function_calls::parse_call_arguments;
use crate::compiler_frontend::ast::expressions::generic_nominal_inference::{
    GenericNominalConstructorInput, GenericNominalTemplate, infer_generic_nominal_constructor,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::generics::substitute_type_parameters;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::numeric::coerce_expression_to_declared_type;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::{return_compiler_error, return_rule_error};

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

    let (resolved_fields, generic_instance_key) = if let Some(generic_decls) =
        &context.generic_declarations_by_path
        && let Some(metadata) = generic_decls.get(struct_path)
        && !metadata.parameters.is_empty()
    {
        let inference = infer_generic_nominal_constructor(
            GenericNominalConstructorInput {
                nominal_path: struct_path,
                display_name: &struct_name_str,
                metadata,
                template: GenericNominalTemplate::StructFields(fields),
                constructor_fields: Some(fields),
                raw_args: Some(&raw_args),
                diagnostics: CallDiagnosticContext::struct_constructor(&struct_name_str),
                location: constructor_location.clone(),
            },
            context,
            string_table,
        )?;

        let instantiated_fields: Vec<Declaration> = fields
            .iter()
            .map(|field| {
                let mut instantiated_field = field.clone();
                instantiated_field.value.data_type =
                    substitute_type_parameters(&field.value.data_type, &inference.substitution);
                instantiated_field
            })
            .collect();

        (instantiated_fields, inference.instance_key)
    } else {
        (fields.to_owned(), None)
    };

    let expectations = expectations_from_struct_fields(&resolved_fields);
    let resolved_args = resolve_call_arguments(
        CallDiagnosticContext::struct_constructor(&struct_name_str),
        &raw_args,
        &expectations,
        constructor_location.clone(),
        string_table,
    )?;

    let enforce_const_record = context.kind.allows_const_record_coercion();
    let mut struct_fields = Vec::with_capacity(fields.len());

    for (field, arg) in resolved_fields.iter().zip(resolved_args.iter()) {
        let field_type = &field.value.data_type;
        // Apply contextual numeric coercion (Int → Float) post-resolution, consistent with
        // declaration sites. resolve_call_arguments has already validated type compatibility.
        let mut value = coerce_expression_to_declared_type(arg.value.clone(), field_type);

        if enforce_const_record {
            // Header-stage struct shells may carry placeholder references until AST environment
            // construction resolves constants in graph order.
            let is_placeholder_reference = if let ExpressionKind::Reference(path) = &value.kind {
                path.name().is_some_and(|name| {
                    context
                        .get_reference(&name)
                        .is_some_and(Declaration::is_unresolved_constant_placeholder)
                })
            } else {
                false
            };

            if !value.is_compile_time_constant() && !is_placeholder_reference {
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
