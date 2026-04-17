use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallDiagnosticContext, expectations_from_struct_fields, resolve_call_arguments,
};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::function_calls::parse_call_arguments;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::Ownership;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::numeric::coerce_expression_to_declared_type;
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
    struct_ownership: &Ownership,
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
    let expectations = expectations_from_struct_fields(fields);
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
            if !value.is_compile_time_constant() {
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
            value.ownership = Ownership::ImmutableOwned;
        }

        struct_fields.push(Declaration {
            id: field.id.to_owned(),
            value,
        });
    }

    let instance_ownership = if enforce_const_record {
        Ownership::ImmutableOwned
    } else {
        struct_ownership.get_owned()
    };

    Ok(Expression::struct_instance(
        struct_path.to_owned(),
        struct_fields,
        constructor_location,
        instance_ownership,
        enforce_const_record,
    ))
}
