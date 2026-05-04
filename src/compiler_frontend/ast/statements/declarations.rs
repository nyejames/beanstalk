use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::function_calls::parse_function_call;
use crate::compiler_frontend::ast::field_access::parse_field_access;
use crate::compiler_frontend::ast::function_body_to_ast;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::{
    ast_nodes::Declaration, expressions::parse_expression::create_expression,
};
use crate::compiler_frontend::builtins::error_type::is_reserved_builtin_symbol;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::declaration_syntax::declaration_shell::{
    DeclarationSyntax, parse_declaration_syntax,
};
use crate::compiler_frontend::declaration_syntax::r#struct::{
    parse_struct_shell, validate_struct_default_values,
};
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeResolutionContext, TypeResolutionContextInputs, resolve_type,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::identifier_policy::{
    IdentifierNamingKind, ensure_not_keyword_shadow_identifier, naming_warning_for_identifier,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::syntax_errors::signature_position::check_signature_common_mistake;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, Token, TokenKind};
use crate::compiler_frontend::type_coercion::compatibility::is_declaration_compatible;
use crate::compiler_frontend::type_coercion::diagnostics::{
    expected_found_clause, offending_value_clause, regular_division_int_context_guidance,
    should_report_regular_division_int_context,
};
use crate::compiler_frontend::type_coercion::numeric::coerce_expression_to_declared_type;
use crate::compiler_frontend::type_coercion::parse_context::parse_expectation_for_target_type;
use crate::{ast_log, return_rule_error, return_syntax_error, return_type_error};

pub fn create_reference(
    token_stream: &mut FileTokens,
    reference_arg: &Declaration,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    // Move past the name
    token_stream.advance();

    match reference_arg.value.data_type {
        // Function Call
        DataType::Function(_, ref signature) => parse_function_call(
            token_stream,
            &reference_arg.id,
            context,
            signature,
            true,
            None,
            string_table,
        ),

        _ => {
            // This either becomes a reference or field access
            parse_field_access(token_stream, reference_arg, context, string_table)
        }
    }
}

pub fn new_declaration(
    token_stream: &mut FileTokens,
    id: StringId,
    context: &ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<Declaration, CompilerError> {
    let declaration_name = string_table.resolve(id).to_owned();
    ensure_not_keyword_shadow_identifier(
        &declaration_name,
        token_stream.current_location(),
        "Variable Declaration",
    )?;

    if is_reserved_builtin_symbol(&declaration_name) {
        return_rule_error!(
            format!(
                "'{}' is reserved as a builtin language type.",
                declaration_name
            ),
            token_stream.current_location(),
            {
                CompilationStage => "Variable Declaration",
                PrimarySuggestion => "Use a different symbol name for user declarations",
            }
        );
    }

    // Move past the name
    token_stream.advance();

    let full_name = context.scope.to_owned().append(id);

    // Function declarations are parsed eagerly here because they use
    // a dedicated signature/body syntax that does not fit value declarations.
    if token_stream.current_token_kind() == &TokenKind::TypeParameterBracket {
        if let Some(warning) = naming_warning_for_identifier(
            &declaration_name,
            token_stream.current_location(),
            IdentifierNamingKind::ValueLike,
        ) {
            context.emit_warning(warning);
        }

        let func_sig =
            FunctionSignature::new(token_stream, warnings, string_table, &full_name, context)?;
        let func_context = context.new_child_function(id, func_sig.to_owned(), string_table);

        let function_body =
            function_body_to_ast(token_stream, func_context, warnings, string_table)?;

        return Ok(Declaration {
            id: full_name,
            value: Expression::function(
                None,
                func_sig,
                function_body,
                token_stream.current_location(),
            ),
        });
    }

    if let Some(error) = check_signature_common_mistake(token_stream) {
        return Err(error);
    }

    let declaration_syntax = parse_declaration_syntax(token_stream, id, string_table)?;
    let naming_kind = if matches!(
        declaration_syntax
            .initializer_tokens
            .first()
            .map(|token| &token.kind),
        Some(TokenKind::TypeParameterBracket)
    ) {
        IdentifierNamingKind::TypeLike
    } else {
        IdentifierNamingKind::ValueLike
    };
    if let Some(warning) = naming_warning_for_identifier(
        &declaration_name,
        declaration_syntax.location.to_owned(),
        naming_kind,
    ) {
        context.emit_warning(warning);
    }

    resolve_declaration_syntax(declaration_syntax, full_name, context, string_table)
}

pub fn resolve_declaration_syntax(
    declaration_syntax: DeclarationSyntax,
    full_name: InternedPath,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Declaration, CompilerError> {
    let value_mode = declaration_syntax.value_mode();
    if declaration_syntax.mutable_marker && context.kind.is_constant_context() {
        return_rule_error!(
            "Constants can't be mutable!",
            declaration_syntax.location, {
                CompilationStage => "Variable Declaration",
                PrimarySuggestion => "Remove the '~' symbol from the variable declaration",
            }
        )
    }

    let mut data_type = declaration_syntax.semantic_type();

    let declaration_location = declaration_syntax.location.clone();
    let type_resolution_context = TypeResolutionContext::from_inputs(TypeResolutionContextInputs {
        declaration_table: &context.top_level_declarations,
        visible_declaration_ids: context.visible_declaration_ids.as_ref(),
        visible_external_symbols: context
            .file_visibility
            .as_ref()
            .map(|fv| &fv.visible_external_symbols),
        visible_source_bindings: context
            .file_visibility
            .as_ref()
            .map(|fv| &fv.visible_source_names),
        visible_type_aliases: context
            .file_visibility
            .as_ref()
            .map(|fv| &fv.visible_type_alias_names),
        resolved_type_aliases: context.resolved_type_aliases.as_deref(),
        generic_declarations_by_path: context.generic_declarations_by_path.as_deref(),
        resolved_struct_fields_by_path: context.resolved_struct_fields_by_path.as_deref(),
        generic_nominal_instantiations: context.generic_nominal_instantiations.as_deref(),
    });
    data_type = resolve_type(
        &data_type,
        &declaration_location,
        &type_resolution_context,
        string_table,
    )?;

    let mut initializer_tokens = declaration_syntax.initializer_tokens;
    initializer_tokens.push(Token::new(
        TokenKind::Eof,
        declaration_syntax.location.to_owned(),
    ));
    let mut initializer_stream = FileTokens::new(full_name.to_owned(), initializer_tokens);

    // Check if this whole expression is nested in brackets.
    // This is just so we don't wastefully call create_expression recursively right away.
    let const_context = ScopeContext::new_constant(initializer_stream.src_path.to_owned(), context);
    let mut parsed_expr = match initializer_stream.current_token_kind() {
        // Struct Definition
        TokenKind::TypeParameterBracket => {
            let owner_path = initializer_stream.src_path.to_owned();
            let params = parse_struct_shell(
                &mut initializer_stream,
                &const_context,
                string_table,
                &owner_path,
            )?;
            validate_struct_default_values(&params, string_table)?;

            Expression::struct_definition(
                params,
                initializer_stream.current_location(),
                value_mode.to_owned(),
            )
        }

        _ => {
            // Save the annotated type so the compatibility and coercion steps
            // below can reference it after the expression has resolved its own
            // natural type.
            //
            // Pass parse-time context only where syntax requires it, such as
            // `none` and empty collection literals. Other expressions resolve
            // their natural type before this declaration boundary validates and
            // coerces them.
            let declared_type = data_type.clone();
            let mut expr_type = parse_expectation_for_target_type(&declared_type);
            let mut expr_context =
                context.new_child_expression(if matches!(declared_type, DataType::Inferred) {
                    context.expected_result_types.clone()
                } else {
                    vec![declared_type.clone()]
                });
            expr_context.kind = context.kind.clone();
            let expr = create_expression(
                &mut initializer_stream,
                &expr_context,
                &mut expr_type,
                &value_mode,
                false,
                string_table,
            )?;

            // Reject incompatible types (e.g. Bool → Float, Float → Int).
            // is_declaration_compatible accepts exact matches and Int → Float;
            // it does not reuse ReturnSlot semantics.
            if !matches!(declared_type, DataType::Inferred)
                && !is_declaration_compatible(&declared_type, &expr.data_type)
            {
                let declaration_name = full_name.name_str(string_table).unwrap_or("<value>");
                let suggestion = if should_report_regular_division_int_context(
                    &declared_type,
                    &expr.data_type,
                    &expr,
                ) {
                    regular_division_int_context_guidance()
                } else {
                    "Update the initializer so it matches the declared variable type, or cast explicitly"
                };
                return_type_error!(
                    format!(
                        "Declaration '{}' has incompatible initializer type. {} {}",
                        declaration_name,
                        expected_found_clause(&declared_type, &expr.data_type, string_table),
                        offending_value_clause(&expr, string_table)
                    ),
                    expr.location.clone(),
                    {
                        CompilationStage => "Expression Evaluation",
                        ExpectedType => declared_type.display_with_table(string_table),
                        FoundType => expr.data_type.display_with_table(string_table),
                        PrimarySuggestion => suggestion,
                    }
                );
            }

            // Apply contextual numeric coercion (e.g. Int → Float) when the
            // declared type requires it.
            coerce_expression_to_declared_type(expr, &declared_type)
        }
    };

    // Defensive: ensure the initializer parser consumed all tokens.
    // If tokens remain, the parser stopped early (e.g. a newline broke the
    // expression before it was complete). This prevents silent truncation.
    initializer_stream.skip_newlines();
    if initializer_stream.current_token_kind() != &TokenKind::Eof {
        return_syntax_error!(
            format!(
                "Unexpected token '{:?}' in declaration initializer for '{}'.",
                initializer_stream.current_token_kind(),
                full_name.name_str(string_table).unwrap_or("<value>")
            ),
            initializer_stream.current_location(),
            {
                CompilationStage => "Variable Declaration",
                PrimarySuggestion => "Remove the extra token or complete the expression before it",
            }
        );
    }

    // WHAT: the binding marker (`~=` / `=`) is the single source of truth for whether the
    // stored declaration is a mutable place.
    // WHY: rvalue initializers (for example struct literals and collections) inherit ownership
    // from type defaults; preserving that ownership would incorrectly allow writes through
    // immutable bindings and mutable receiver calls.
    parsed_expr.value_mode = value_mode.to_owned();

    ast_log!(
        "Created new ",
        Cyan #value_mode,
        " ",
        data_type.display_with_table(string_table)
    );

    Ok(Declaration {
        id: full_name,
        value: parsed_expr,
    })
}

#[cfg(test)]
#[path = "tests/declaration_tests.rs"]
mod declaration_tests;
