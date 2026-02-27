use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::field_access::parse_field_access;
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::ast::statements::declaration_syntax::{
    DeclarationSyntax, parse_declaration_syntax,
};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionSignature, parse_function_call,
};
use crate::compiler_frontend::ast::statements::structs::create_struct_definition;
use crate::compiler_frontend::ast::{
    ast_nodes::Declaration, expressions::parse_expression::create_expression,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::Ownership;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, Token, TokenKind};
use crate::compiler_frontend::traits::ContainsReferences;
use crate::{ast_log, return_rule_error};

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
        crate::compiler_frontend::datatypes::DataType::Function(_, ref signature) => {
            parse_function_call(
                token_stream,
                &reference_arg.id,
                context,
                signature,
                string_table,
            )
        }

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
    // Move past the name
    token_stream.advance();

    let full_name = context.scope.to_owned().append(id);

    // Function declarations are parsed eagerly here because they use
    // a dedicated signature/body syntax that does not fit value declarations.
    if token_stream.current_token_kind() == &TokenKind::TypeParameterBracket {
        let func_sig = FunctionSignature::new(token_stream, string_table, &full_name)?;
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
                declaration_syntax.location.to_error_location(string_table), {
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

    if let Some(type_name) = declaration_syntax.explicit_named_type {
        let declared_type = context.get_reference(&type_name).ok_or_else(|| {
            CompilerError::new_rule_error(
                format!(
                    "Unknown type '{}'. Type names must be declared before use.",
                    string_table.resolve(type_name)
                ),
                declaration_syntax.location.to_error_location(string_table),
            )
        })?;

        data_type = declared_type.value.data_type.to_owned();
    }

    let mut initializer_tokens = declaration_syntax.initializer_tokens;
    initializer_tokens.push(Token::new(
        TokenKind::Eof,
        declaration_syntax.location.to_owned(),
    ));
    let mut initializer_stream = FileTokens::new(full_name.to_owned(), initializer_tokens);

    // Check if this whole expression is nested in brackets.
    // This is just so we don't wastefully call create_expression recursively right away.
    let mut parsed_expr = match initializer_stream.current_token_kind() {
        // Struct Definition
        TokenKind::TypeParameterBracket => {
            let params = create_struct_definition(&mut initializer_stream, string_table)?;

            Expression::struct_definition(
                params,
                initializer_stream.current_location(),
                ownership.to_owned(),
            )
        }

        _ => create_expression(
            &mut initializer_stream,
            context,
            &mut data_type,
            &ownership,
            false,
            string_table,
        )?,
    };

    // Declaration mutability is determined by the left-hand marker (`~=`) for direct references.
    if matches!(parsed_expr.kind, ExpressionKind::Reference(_)) {
        parsed_expr.ownership = ownership.to_owned();
    }

    ast_log!("Created new ", Cyan #ownership, " ", data_type);

    Ok(Declaration {
        id: full_name,
        value: parsed_expr,
    })
}
