use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::function_calls::parse_function_call;
use crate::compiler_frontend::ast::field_access::parse_field_access;
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::ast::statements::declaration_syntax::{
    DeclarationSyntax, parse_declaration_syntax,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::statements::structs::create_struct_definition;
use crate::compiler_frontend::ast::{
    ast_nodes::Declaration, expressions::parse_expression::create_expression,
};
use crate::compiler_frontend::builtins::error_type::is_reserved_builtin_symbol;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, Token, TokenKind};
use crate::compiler_frontend::traits::ContainsReferences;
use crate::compiler_frontend::type_coercion::compatibility::is_declaration_compatible;
use crate::compiler_frontend::type_coercion::diagnostics::expected_found_clause;
use crate::compiler_frontend::type_coercion::numeric::coerce_expression_to_declared_type;
use crate::compiler_frontend::type_coercion::parse_context::parse_expectation_for_target_type;
use crate::compiler_frontend::type_syntax::resolve_named_types_in_data_type;
use crate::{ast_log, return_rule_error, return_type_error};

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
    if is_reserved_builtin_symbol(string_table.resolve(id)) {
        return_rule_error!(
            format!(
                "'{}' is reserved as a builtin language type.",
                string_table.resolve(id)
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
        let func_sig = FunctionSignature::new(token_stream, string_table, &full_name, context)?;
        let func_context = context.new_child_function(id, func_sig.to_owned(), string_table);

        let function_body = function_body_to_ast(
            token_stream,
            func_context.to_owned(),
            warnings,
            string_table,
        )?;

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

    let declaration_syntax = parse_declaration_syntax(token_stream, id, string_table)?;
    resolve_declaration_syntax(declaration_syntax, full_name, context, string_table)
}

pub fn resolve_declaration_syntax(
    declaration_syntax: DeclarationSyntax,
    full_name: InternedPath,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Declaration, CompilerError> {
    let ownership = if declaration_syntax.mutable_marker {
        if context.kind.is_constant_context() {
            return_rule_error!(
                "Constants can't be mutable!",
                declaration_syntax.location, {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Remove the '~' symbol from the variable declaration",
                }
            )
        }
        Ownership::MutableOwned
    } else {
        Ownership::ImmutableOwned
    };

    let mut data_type = declaration_syntax.to_data_type(&ownership);

    let declaration_location = declaration_syntax.location.clone();
    data_type = resolve_named_types_in_data_type(
        &data_type,
        &declaration_location,
        &mut |type_name| {
            context
                .get_reference(&type_name)
                .map(|declaration| declaration.value.data_type.to_owned())
        },
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
            let params =
                create_struct_definition(&mut initializer_stream, &const_context, string_table)?;

            Expression::struct_definition(
                params,
                initializer_stream.current_location(),
                ownership.to_owned(),
            )
        }

        _ => {
            // Save the annotated type so the compatibility and coercion steps
            // below can reference it after the expression has resolved its own
            // natural type.
            //
            // Pass parse-time context only where syntax requires it: Option(_)
            // targets need the context so that `none` literals can extract their
            // inner type during parsing. Everything else uses Inferred so that
            // eval_expression stays strict (Exact context) and coercion remains
            // explicit and post-parse.
            let declared_type = data_type.clone();
            let mut expr_type = parse_expectation_for_target_type(&declared_type);
            let expr = create_expression(
                &mut initializer_stream,
                context,
                &mut expr_type,
                &ownership,
                false,
                string_table,
            )?;

            // Reject incompatible types (e.g. Bool → Float, Float → Int).
            // is_declaration_compatible accepts exact matches and Int → Float;
            // it does not reuse ReturnSlot semantics.
            if !matches!(declared_type, DataType::Inferred)
                && !is_declaration_compatible(&declared_type, &expr.data_type)
            {
                return_type_error!(
                    format!(
                        "Type mismatch in expression. {}",
                        expected_found_clause(&declared_type, &expr.data_type, string_table)
                    ),
                    expr.location.clone(),
                    {
                        CompilationStage => "Expression Evaluation",
                        PrimarySuggestion => "Ensure the expression produces the declared type",
                    }
                );
            }

            // Apply contextual numeric coercion (e.g. Int → Float) when the
            // declared type requires it.
            coerce_expression_to_declared_type(expr, &declared_type)
        }
    };

    // WHAT: the binding marker (`~=` / `=`) is the single source of truth for whether the
    // stored declaration is a mutable place.
    // WHY: rvalue initializers (for example struct literals and collections) inherit ownership
    // from type defaults; preserving that ownership would incorrectly allow writes through
    // immutable bindings and mutable receiver calls.
    parsed_expr.ownership = ownership.to_owned();

    ast_log!(
        "Created new ",
        Cyan #ownership,
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
