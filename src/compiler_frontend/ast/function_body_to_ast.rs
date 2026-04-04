use super::ast_nodes::{Declaration, NodeKind};
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::mutation::{
    handle_mutation, handle_mutation_target,
};
use crate::compiler_frontend::ast::expressions::parse_expression::{
    create_expression, create_multiple_expressions,
};
use crate::compiler_frontend::ast::field_access::parse_field_access;
use crate::compiler_frontend::ast::receiver_methods::free_function_receiver_method_call_error;
use crate::compiler_frontend::builtins::error_type::is_reserved_builtin_symbol;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};

use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::statements::branching::create_branch;
use crate::compiler_frontend::ast::statements::declarations::new_declaration;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot, parse_function_call,
};
use crate::compiler_frontend::ast::statements::loops::create_loop;
use crate::compiler_frontend::ast::statements::multi_bind::parse_multi_bind_statement;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::traits::ContainsReferences;
use crate::projects::settings;
use crate::projects::settings::TOP_LEVEL_TEMPLATE_NAME;
use crate::{ast_log, return_rule_error, return_syntax_error, return_type_error};

fn is_return_terminator(token: &TokenKind) -> bool {
    matches!(token, TokenKind::Newline | TokenKind::End | TokenKind::Eof)
}

fn is_assignment_operator(token: &TokenKind) -> bool {
    matches!(
        token,
        TokenKind::Assign
            | TokenKind::AddAssign
            | TokenKind::SubtractAssign
            | TokenKind::MultiplyAssign
            | TokenKind::DivideAssign
            | TokenKind::ExponentAssign
            | TokenKind::RootAssign
    )
}

fn is_expression_statement(expr: &Expression) -> bool {
    match &expr.kind {
        ExpressionKind::FunctionCall(..)
        | ExpressionKind::ResultHandledFunctionCall { .. }
        | ExpressionKind::HandledResult { .. }
        | ExpressionKind::HostFunctionCall(..) => true,
        ExpressionKind::Runtime(nodes) => nodes.iter().any(|node| {
            matches!(
                node.kind,
                NodeKind::MethodCall { .. }
                    | NodeKind::FunctionCall { .. }
                    | NodeKind::HostFunctionCall { .. }
            )
        }),
        _ => false,
    }
}

fn normalize_return_expression_type(data_type: &DataType) -> DataType {
    // Runtime templates lower into string-producing functions.
    // Treat them as string returns during signature validation.
    match data_type {
        DataType::Template | DataType::TemplateWrapper => DataType::StringSlice,
        _ => data_type.to_owned(),
    }
}

fn parse_expression_statement_candidate(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let mut inferred = DataType::Inferred;
    let expr = create_expression(
        token_stream,
        context,
        &mut inferred,
        &Ownership::ImmutableOwned,
        false,
        string_table,
    )?;

    if !is_expression_statement(&expr) {
        return_syntax_error!(
            "Standalone expression is not a valid statement in this position.",
            token_stream.current_location(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Use an assignment, call, control-flow statement, or declaration here",
            }
        );
    }

    Ok(expr)
}

fn parse_symbol_expression_statement_candidate(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    symbol_id: StringId,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let mut inferred = DataType::Inferred;
    let expr = create_expression(
        token_stream,
        context,
        &mut inferred,
        &Ownership::ImmutableOwned,
        false,
        string_table,
    )?;

    if !is_expression_statement(&expr) {
        return_syntax_error!(
            format!(
                "Unexpected token '{:?}' after variable reference '{}'. Expected an assignment or callable expression.",
                token_stream.current_token_kind(),
                string_table.resolve(symbol_id)
            ),
            token_stream.current_location(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Use an assignment operator, a function call, or a receiver method call in statement position",
            }
        );
    }

    Ok(expr)
}

fn push_accessed_symbol_statement(
    accessed: AstNode,
    ast: &mut Vec<AstNode>,
    context: &ScopeContext,
    token_stream: &FileTokens,
    symbol_id: StringId,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    match accessed.kind {
        NodeKind::MethodCall { .. } => {
            ast.push(AstNode {
                kind: NodeKind::Rvalue(accessed.get_expr()?),
                location: accessed.location,
                scope: context.scope.clone(),
            });
            Ok(())
        }
        NodeKind::FieldAccess { .. } => {
            return_syntax_error!(
                format!(
                    "Unexpected token '{:?}' after field access '{}'. Field reads are not valid standalone statements.",
                    token_stream.current_token_kind(),
                    string_table.resolve(symbol_id)
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Assign the field to a variable, mutate it, or call a method instead of leaving it as a standalone statement",
                }
            );
        }
        _ => {
            return_syntax_error!(
                "Standalone expression is not a valid statement in this position.",
                token_stream.current_location(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Use an assignment, call, control-flow statement, or declaration here",
                }
            );
        }
    }
}

fn parse_symbol_statement(
    token_stream: &mut FileTokens,
    ast: &mut Vec<AstNode>,
    context: &mut ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    let TokenKind::Symbol(id) = token_stream.current_token_kind().to_owned() else {
        return_syntax_error!(
            "Expected a symbol-led statement.",
            token_stream.current_location(),
            {
                CompilationStage => "AST Construction",
            }
        );
    };

    if is_reserved_builtin_symbol(string_table.resolve(id)) {
        return_rule_error!(
            format!(
                "'{}' is reserved as a builtin language type.",
                string_table.resolve(id)
            ),
            token_stream.current_location(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Use a different symbol name for user variables and declarations",
            }
        );
    }

    let full_path = context.scope.append(id);

    if let Some(multi_bind) = parse_multi_bind_statement(token_stream, context, string_table)? {
        ast.push(multi_bind);
        return Ok(());
    }

    if let Some(start_target) = context.resolve_start_import(&id) {
        token_stream.advance();

        match token_stream.current_token_kind() {
            TokenKind::OpenParenthesis => {
                ast.push(parse_function_call(
                    token_stream,
                    start_target,
                    context,
                    &FunctionSignature {
                        parameters: vec![],
                        returns: vec![ReturnSlot::success(FunctionReturn::Value(
                            DataType::StringSlice,
                        ))],
                    },
                    false,
                    Some(warnings),
                    string_table,
                )?);
                return Ok(());
            }

            TokenKind::Dot => {
                return_rule_error!(
                    format!(
                        "Imported file '{}' is callable only as '{}()'. File-struct member access is no longer supported.",
                        string_table.resolve(id),
                        string_table.resolve(id),
                    ),
                    token_stream.current_location(), {
                        CompilationStage => "AST Construction",
                        PrimarySuggestion => "Import exports directly with '@path/to/file/symbol' or '@path/to/file {a, b}'",
                    }
                );
            }

            _ => {
                return_rule_error!(
                    format!(
                        "Imported file '{}' can only be used as a callable start import ('{}()').",
                        string_table.resolve(id),
                        string_table.resolve(id),
                    ),
                    token_stream.current_location(), {
                        CompilationStage => "AST Construction",
                        PrimarySuggestion => "Call the file start function with 'file()' or import specific exports directly",
                    }
                );
            }
        }
    }

    if let Some(arg) = context.get_reference(&id) {
        match token_stream.peek_next_token() {
            Some(TokenKind::Assign)
            | Some(TokenKind::AddAssign)
            | Some(TokenKind::SubtractAssign)
            | Some(TokenKind::MultiplyAssign)
            | Some(TokenKind::DivideAssign)
            | Some(TokenKind::ExponentAssign)
            | Some(TokenKind::RootAssign) => {
                token_stream.advance();
                ast.push(handle_mutation(token_stream, arg, context, string_table)?);
                return Ok(());
            }

            Some(TokenKind::Dot) => {
                token_stream.advance();
                let accessed = parse_field_access(token_stream, arg, context, string_table)?;

                if is_assignment_operator(token_stream.current_token_kind()) {
                    ast.push(handle_mutation_target(
                        token_stream,
                        arg,
                        accessed,
                        context,
                        string_table,
                    )?);
                    return Ok(());
                }

                push_accessed_symbol_statement(
                    accessed,
                    ast,
                    context,
                    token_stream,
                    id,
                    string_table,
                )?;
                return Ok(());
            }

            Some(TokenKind::DatatypeInt)
            | Some(TokenKind::DatatypeFloat)
            | Some(TokenKind::DatatypeBool)
            | Some(TokenKind::DatatypeString)
            | Some(TokenKind::Mutable) => {
                return_rule_error!(
                    format!("Variable '{}' is already declared. Shadowing is not supported in Beanstalk. Use '=' to mutate its value or choose a different variable name", string_table.resolve(id)),
                    token_stream.current_location(), {
                        CompilationStage => "AST Construction",
                        PrimarySuggestion => "Use '=' to mutate the existing variable or choose a different name",
                    }
                );
            }

            _ => {
                let expr = parse_symbol_expression_statement_candidate(
                    token_stream,
                    context,
                    id,
                    string_table,
                )?;

                ast.push(AstNode {
                    kind: NodeKind::Rvalue(expr),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
                return Ok(());
            }
        }
    }

    if let Some(host_func_call) = context.host_registry.get_function(string_table.resolve(id)) {
        token_stream.advance();
        let signature = host_func_call.params_to_signature(string_table);

        ast.push(parse_function_call(
            token_stream,
            &full_path,
            context,
            &signature,
            false,
            Some(warnings),
            string_table,
        )?);
        return Ok(());
    }

    if token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis) {
        if let Some(method_entry) = context.lookup_visible_receiver_method_by_name(id) {
            return Err(free_function_receiver_method_call_error(
                id,
                method_entry,
                token_stream.current_location(),
                "AST Construction",
                string_table,
            ));
        }

        return_rule_error!(
            format!(
                "Call target '{}' is not declared in this scope and is not a registered host function.",
                string_table.resolve(id)
            ),
            token_stream.current_location(), {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Declare/import this function before calling it, or check the function name spelling",
                AlternativeSuggestion => "If this should be a host function, register it in the host registry for this backend",
            }
        );
    }

    let arg = new_declaration(token_stream, id, context, warnings, string_table)?;

    match arg.value.kind {
        ExpressionKind::StructDefinition(ref params) => {
            ast.push(AstNode {
                kind: NodeKind::StructDefinition(arg.id.to_owned(), params.to_owned()),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
        }

        ExpressionKind::Function(ref signature, ref body) => {
            ast.push(AstNode {
                kind: NodeKind::Function(arg.id.to_owned(), signature.to_owned(), body.to_owned()),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
        }

        _ => {
            ast.push(AstNode {
                kind: NodeKind::VariableDeclaration(arg.to_owned()),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
        }
    }

    context.add_var(arg);
    Ok(())
}

fn unexpected_function_body_token_error(
    token: &TokenKind,
    token_stream: &FileTokens,
) -> CompilerError {
    match token {
        TokenKind::Comma => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected ',' in function body. Commas only separate items in lists, arguments, or return declarations.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from("Remove the comma or place it inside a list/argument context"),
            );
            error
        }

        TokenKind::CloseParenthesis => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected ')' in function body. This usually means an earlier '(' was not parsed in a valid expression or call.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from(
                    "Remove the stray ')' or complete the expression/call before this point",
                ),
            );
            error
        }

        TokenKind::CloseCurly => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected '}' in function body. Curly braces are only valid for collection syntax.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from(
                    "Remove the stray '}' or use collection syntax in a valid expression context",
                ),
            );
            error
        }

        TokenKind::TypeParameterBracket => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected '|' in function body. '|' is only used in function signatures and struct field/type declarations.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from("Remove the stray '|' or move it into a declaration signature"),
            );
            error
        }

        TokenKind::Arrow => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected '->' in function body. Arrow syntax is only valid in function signatures.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from("Use '->' only in a function signature like '|args| -> Type:'"),
            );
            error
        }

        TokenKind::Wildcard => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected wildcard '_' in function body. Wildcards are not standalone statements.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from("Use '_' only in supported pattern positions, or use 'else:' for default match arms"),
            );
            error
        }

        other => {
            let mut error = CompilerError::new_syntax_error(
                format!("Unexpected token '{other:?}' in a function body."),
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from("Use a valid statement such as a declaration, assignment, call, control-flow block, or template"),
            );
            error
        }
    }
}

pub fn function_body_to_ast(
    token_stream: &mut FileTokens,
    mut context: ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<Vec<AstNode>, CompilerError> {
    let mut ast: Vec<AstNode> =
        Vec::with_capacity(token_stream.length / settings::TOKEN_TO_NODE_RATIO);

    while token_stream.index < token_stream.length {
        // This should be starting after the imports
        let current_token = token_stream.current_token_kind().to_owned();

        ast_log!("Parsing Token: ", #current_token);

        match current_token {
            TokenKind::ModuleStart => {
                // Module start token is only used for naming; skip it.
                token_stream.advance();
            }

            TokenKind::Symbol(_) => parse_symbol_statement(
                token_stream,
                &mut ast,
                &mut context,
                warnings,
                string_table,
            )?,

            // Control Flow
            TokenKind::Loop => {
                token_stream.advance();

                ast.push(create_loop(
                    token_stream,
                    context.new_child_control_flow(ContextKind::Loop, string_table),
                    warnings,
                    string_table,
                )?);
            }

            TokenKind::If => {
                token_stream.advance();

                // This is extending as it might get folded into a vec of nodes
                ast.extend(create_branch(
                    token_stream,
                    &mut context.new_child_control_flow(ContextKind::Branch, string_table),
                    warnings,
                    string_table,
                )?);
            }

            TokenKind::Else => {
                // If we are inside an if / match statement, break out
                if context.kind == ContextKind::Branch {
                    break;
                } else {
                    return_rule_error!(
                        "Unexpected use of 'else' keyword. It can only be used inside an if statement or match statement",
                        token_stream.current_location(), {
                            CompilationStage => "AST Construction",
                            PrimarySuggestion => "Remove the 'else' or place it inside an if/match statement",
                        }
                    )
                }
            }

            // IGNORED TOKENS
            TokenKind::Newline => {
                // Skip standalone newlines / empty tokens
                token_stream.advance();
            }

            TokenKind::Return => {
                if context.expected_result_types.is_empty()
                    && !matches!(context.kind, ContextKind::Function)
                {
                    return_rule_error!(
                        "Return statements can only be used inside functions",
                        token_stream.current_location()
                    )
                }

                token_stream.advance();

                if token_stream.current_token_kind() == &TokenKind::Bang {
                    let Some(expected_error_type) = context.expected_error_type.as_ref() else {
                        return_rule_error!(
                            "return! can only be used inside functions that declare an error return slot",
                            token_stream.current_location(),
                            {
                                CompilationStage => "AST Construction",
                                PrimarySuggestion => "Use plain 'return' or add an error slot like 'Error!' to the function signature",
                            }
                        );
                    };

                    token_stream.advance();
                    if is_return_terminator(token_stream.current_token_kind()) {
                        return_type_error!(
                            "return! requires an error value",
                            token_stream.current_location(),
                            {
                                CompilationStage => "AST Construction",
                                PrimarySuggestion => "Provide one value that matches the function error return type",
                            }
                        );
                    }

                    let mut expected_error = expected_error_type.to_owned();
                    let returned_error = create_expression(
                        token_stream,
                        &context,
                        &mut expected_error,
                        &Ownership::ImmutableOwned,
                        false,
                        string_table,
                    )?;

                    let normalized_actual =
                        normalize_return_expression_type(&returned_error.data_type);
                    if &normalized_actual != expected_error_type {
                        return_type_error!(
                            format!(
                                "return! value has incorrect type. Expected '{}', got '{}'.",
                                expected_error_type.display_with_table(string_table),
                                normalized_actual.display_with_table(string_table)
                            ),
                            returned_error.location.clone(),
                            {
                                CompilationStage => "AST Construction",
                                PrimarySuggestion => "Return an error value that exactly matches the function error slot type",
                            }
                        );
                    }

                    ast.push(AstNode {
                        kind: NodeKind::ReturnError(returned_error),
                        location: token_stream.current_location(),
                        scope: context.scope.clone(),
                    });

                    continue;
                }

                let return_values = if context.expected_result_types.is_empty() {
                    if is_return_terminator(token_stream.current_token_kind()) {
                        Vec::new()
                    } else {
                        return_type_error!(
                            "This function has no return signature, so 'return' must be bare (no return values).",
                            token_stream
                                .current_location()
                                ,
                            {
                                CompilationStage => "AST Construction",
                                PrimarySuggestion => "Use bare 'return' with no value in this function",
                                AlternativeSuggestion => "If you intended to return a value, add a return signature (for example '|args| -> String:')",
                            }
                        )
                    }
                } else {
                    if is_return_terminator(token_stream.current_token_kind()) {
                        let expected_count = context.expected_result_types.len();
                        return_type_error!(
                            format!(
                                "This function must return {} value{}, but this return statement is bare.",
                                expected_count,
                                if expected_count == 1 { "" } else { "s" }
                            ),
                            token_stream
                                .current_location()
                                ,
                            {
                                CompilationStage => "AST Construction",
                                PrimarySuggestion => "Provide return values that match the function signature",
                            }
                        )
                    }

                    let parsed_returns =
                        create_multiple_expressions(token_stream, &context, false, string_table)?;

                    if token_stream.current_token_kind() == &TokenKind::Comma {
                        let expected_count = context.expected_result_types.len();
                        return_type_error!(
                            format!(
                                "This function signature declares {} return value{}, but this return statement provides more.",
                                expected_count,
                                if expected_count == 1 { "" } else { "s" }
                            ),
                            token_stream
                                .current_location()
                                ,
                            {
                                CompilationStage => "AST Construction",
                                PrimarySuggestion => "Remove extra return values or update the function return signature",
                            }
                        );
                    }

                    for (index, (returned_value, expected_type)) in parsed_returns
                        .iter()
                        .zip(context.expected_result_types.iter())
                        .enumerate()
                    {
                        let normalized_actual =
                            normalize_return_expression_type(&returned_value.data_type);

                        if &normalized_actual != expected_type {
                            return_type_error!(
                                format!(
                                    "Return value {} has incorrect type. Expected '{}', got '{}'. Return values must match the function signature exactly.",
                                    index + 1,
                                    expected_type.display_with_table(string_table),
                                    normalized_actual.display_with_table(string_table)
                                ),
                                returned_value.location.clone(),
                                {
                                    CompilationStage => "AST Construction",
                                    PrimarySuggestion => "Update the returned expression to match the declared return type",
                                    AlternativeSuggestion => "If this value is intended, change the function return signature to the correct type",
                                }
                            );
                        }
                    }

                    parsed_returns
                };

                // if !return_value.is_pure() {
                //     *pure = false;
                // }

                ast.push(AstNode {
                    kind: NodeKind::Return(return_values),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }

            TokenKind::Break => {
                if !context.is_inside_loop() {
                    return_rule_error!(
                        "Break statements can only be used inside loops",
                        token_stream
                            .current_location()
                            ,
                        {
                            CompilationStage => "AST Construction",
                            PrimarySuggestion => "Move this break statement inside a loop body",
                        }
                    );
                }

                ast.push(AstNode {
                    kind: NodeKind::Break,
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
                token_stream.advance();
            }

            TokenKind::Continue => {
                if !context.is_inside_loop() {
                    return_rule_error!(
                        "Continue statements can only be used inside loops",
                        token_stream
                            .current_location()
                            ,
                        {
                            CompilationStage => "AST Construction",
                            PrimarySuggestion => "Move this continue statement inside a loop body",
                        }
                    );
                }

                ast.push(AstNode {
                    kind: NodeKind::Continue,
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
                token_stream.advance();
            }

            TokenKind::End => {
                // Check that this is a valid scope for a scope to close
                // Module scope should not have an 'end' anywhere
                match context.kind {
                    ContextKind::Expression => {
                        return_syntax_error!(
                            "Unexpected scope close. Expressions are not terminated like this.
                            Surround the expression with brackets if you need it to be multi-line. This might just be a compiler_frontend bug.",
                            token_stream.current_location()
                        );
                    }
                    ContextKind::Template => {
                        return_syntax_error!(
                            "Unexpected use of ';' inside a template. Templates are not closed with ';'.
                            If you are seeing this error, this might be a compiler_frontend bug instead.",
                            token_stream.current_location()
                        )
                    }
                    _ => {
                        // Consume the end token
                        token_stream.advance();
                        break;
                    }
                }
            }

            // String template at the top level of the start function.
            TokenKind::TemplateHead => {
                // If this isn't the top level of the module, this should be an error
                // Only top level scope can have top level templates
                if context.kind != ContextKind::Module {
                    return_rule_error!(
                        "Templates can only be used like this at the top level. Not inside the body of a function",
                        token_stream.current_location()
                    )
                }

                // Each top-level template statement is emitted as a standalone declaration.
                // `top_level_templates` later lifts and orders these declarations when synthesizing
                // start fragments, so this stage intentionally preserves one declaration per template.
                let template = Template::new(token_stream, &context, vec![], string_table)?;
                let expr = Expression::template(template, Ownership::MutableOwned);

                let template_var = Declaration {
                    id: InternedPath::from_single_str(TOP_LEVEL_TEMPLATE_NAME, string_table),
                    value: expr,
                };

                ast.push(AstNode {
                    kind: NodeKind::VariableDeclaration(template_var),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                })
            }

            TokenKind::Eof => {
                break;
            }

            TokenKind::OpenParenthesis
            | TokenKind::FloatLiteral(_)
            | TokenKind::IntLiteral(_)
            | TokenKind::StringSliceLiteral(_)
            | TokenKind::BoolLiteral(_)
            | TokenKind::CharLiteral(_)
            | TokenKind::Copy => {
                let expr =
                    parse_expression_statement_candidate(token_stream, &context, string_table)?;

                ast.push(AstNode {
                    kind: NodeKind::Rvalue(expr),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }

            // Or stuff that hasn't been implemented yet
            _ => {
                return Err(unexpected_function_body_token_error(
                    token_stream.current_token_kind(),
                    token_stream,
                ));
            }
        }
    }

    Ok(ast)
}

#[cfg(test)]
#[path = "tests/parser_error_recovery_tests.rs"]
mod parser_error_recovery_tests;
