use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::Ownership;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::{return_compiler_error, return_rule_error, return_syntax_error, return_type_error};

/// Parse `StructName(...)` and return a finalized struct instance expression.
///
/// WHAT:
/// - Parses positional constructor arguments in source order.
/// - Validates arity and per-field types.
/// - Fills trailing fields from struct defaults when arguments are omitted.
/// - Produces a canonical `Expression::struct_instance` with definition-order fields.
///
/// WHY:
/// - Keeping constructor synthesis centralized makes struct-instance behavior
///   discoverable and easier to extend (named args, diagnostics, etc.).
/// - This function is the single place where const-record coercion rules are
///   enforced for top-level `#` constants.
pub(crate) fn parse_struct_constructor_expression(
    token_stream: &mut FileTokens,
    struct_name: StringId,
    fields: &[Declaration],
    struct_ownership: &Ownership,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let constructor_location = token_stream.current_location();

    // We are called while the stream points at the struct symbol.
    // Advance to "(" and then to the first argument token so expression parsing
    // can consume arguments with the normal expression pipeline.
    token_stream.advance();
    if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
        return_compiler_error!("Struct constructor parser called without an opening parenthesis");
    }
    token_stream.advance();

    let mut provided_values = Vec::with_capacity(fields.len());
    let mut field_index = 0usize;

    while token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
        if field_index >= fields.len() {
            return_type_error!(
                format!(
                    "Struct constructor '{}' received too many arguments. Expected at most {}, but more were provided.",
                    string_table.resolve(struct_name),
                    fields.len()
                ),
                token_stream.current_location().to_error_location(string_table),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Remove extra struct constructor arguments so they match the declared fields",
                }
            );
        }

        // Parse each argument using the destination field type.
        // This reuses existing type-checking behavior and keeps constructor
        // argument semantics aligned with all other expression assignments.
        let mut expected_type = fields[field_index].value.data_type.to_owned();
        let value = create_expression(
            token_stream,
            context,
            &mut expected_type,
            &Ownership::ImmutableOwned,
            false,
            string_table,
        )?;
        provided_values.push(value);
        field_index += 1;

        match token_stream.current_token_kind() {
            TokenKind::Comma => {
                token_stream.advance();
                if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
                    return_syntax_error!(
                        "Trailing commas in struct constructor arguments are not supported.",
                        token_stream.current_location().to_error_location(string_table),
                        {
                            CompilationStage => "Expression Parsing",
                            PrimarySuggestion => "Remove the trailing comma in this constructor call",
                        }
                    );
                }
            }
            TokenKind::CloseParenthesis => {}
            _ => {
                return_syntax_error!(
                    format!(
                        "Expected ',' or ')' after struct constructor argument, found '{:?}'",
                        token_stream.current_token_kind()
                    ),
                    token_stream.current_location().to_error_location(string_table),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Separate constructor arguments with ',' and close with ')'",
                    }
                );
            }
        }
    }

    // Consume ')' so callers continue from the token after constructor syntax.
    token_stream.advance();

    // Missing values are only legal when the remaining fields have defaults.
    // This enables partial constructor calls while keeping required fields strict.
    let missing_required = fields
        .iter()
        .skip(provided_values.len())
        .filter(|field| matches!(field.value.kind, ExpressionKind::None))
        .count();
    if missing_required > 0 {
        return_syntax_error!(
            format!(
                "Struct constructor for '{}' is missing {missing_required} required field argument(s) without defaults.",
                string_table.resolve(struct_name)
            ),
            constructor_location.to_error_location(string_table),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Provide all required field arguments or add defaults for those struct fields",
            }
        );
    }

    let enforce_const_record = context.kind.allows_const_record_coercion();
    let mut struct_fields = Vec::with_capacity(fields.len());

    for (index, field) in fields.iter().enumerate() {
        // WHAT: combine explicit arguments + trailing defaults.
        // WHY: struct instances must always materialize every field explicitly
        // before later AST/HIR passes; downstream phases should never infer
        // omitted fields themselves.
        let mut value = if let Some(provided) = provided_values.get(index) {
            provided.to_owned()
        } else {
            field.value.to_owned()
        };

        if enforce_const_record {
            if !value.is_compile_time_constant() {
                let field_name = field.id.name_str(string_table).unwrap_or("<field>");
                return_rule_error!(
                    format!(
                        "Const struct coercion requires compile-time field values. Field '{}' in '{}' is not compile-time constant.",
                        field_name,
                        string_table.resolve(struct_name)
                    ),
                    value.location.to_error_location(string_table),
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
        struct_fields,
        constructor_location,
        instance_ownership,
    ))
}
