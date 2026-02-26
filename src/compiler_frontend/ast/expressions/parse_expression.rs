use super::eval_expression::evaluate_expression;
use crate::compiler_frontend::ast::ast::ContextKind;
use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator,
};
use crate::compiler_frontend::ast::statements::collections::new_collection;
use crate::compiler_frontend::ast::statements::declarations::create_reference;
use crate::compiler_frontend::ast::statements::functions::parse_function_call;
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TextLocation, Token, TokenKind};
use crate::compiler_frontend::traits::ContainsReferences;
use crate::{
    ast_log, new_template_context, return_compiler_error, return_rule_error, return_syntax_error,
    return_type_error,
};

// For multiple returns or function calls
// MUST know all the types
pub fn create_multiple_expressions(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    consume_closing_parenthesis: bool,
    string_table: &mut StringTable,
) -> Result<Vec<Expression>, CompilerError> {
    let mut expressions: Vec<Expression> = Vec::new();
    for (type_index, expected_type) in context.returns.iter().enumerate() {
        let mut expected_arg = expected_type.to_owned();
        let expression = create_expression(
            token_stream,
            context,
            &mut expected_arg,
            &Ownership::ImmutableOwned,
            false,
            string_table,
        )?;

        expressions.push(expression);

        if type_index + 1 < context.returns.len() {
            if token_stream.current_token_kind() != &TokenKind::Comma {
                return_type_error!(
                    format!(
                        "Too few arguments provided. Expected: {}. Provided: {}.",
                        context.returns.len(),
                        expressions.len()
                    ),
                    token_stream.current_location().to_error_location(&string_table),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Add missing arguments to match the expected count",
                    }
                )
            }

            token_stream.advance();
        }
    }

    if token_stream.current_token_kind() == &TokenKind::Comma {
        return_type_error!(
            format!(
                "Too many arguments provided. Expected: {}. Provided: {}.",
                context.returns.len(),
                expressions.len() + 1
            ),
            token_stream.current_location().to_error_location(&string_table),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Remove extra arguments to match the expected count",
            }
        )
    }

    if consume_closing_parenthesis {
        if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
            return_syntax_error!(
                format!(
                    "Expected closing parenthesis after arguments, found '{:?}'",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location().to_error_location(&string_table),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Add ')' after the final argument",
                    SuggestedInsertion => ")",
                }
            );
        }

        token_stream.advance();
    }

    Ok(expressions)
}

// If the datatype is a collection,
// the expression must only contain references to collections
// or collection literals.
pub fn create_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    data_type: &mut DataType,
    ownership: &Ownership,
    consume_closing_parenthesis: bool,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let mut expression: Vec<AstNode> = Vec::new();
    // let mut number_union = get_any_number_datatype(false);

    ast_log!(
        "Parsing ",
        #ownership,
        data_type.to_string(),
        " Expression"
    );

    // Loop through the expression and create the AST nodes
    // Figure out the type it should be from the data
    // DOES NOT MOVE TOKENS PAST THE CLOSING TOKEN
    let mut next_number_negative = false;
    while token_stream.index < token_stream.length {
        let token = token_stream.current_token_kind().to_owned();
        ast_log!("Parsing token (expression): ", #token);

        match token {
            TokenKind::CloseParenthesis => {
                if consume_closing_parenthesis {
                    token_stream.advance();
                }

                if expression.is_empty() {
                    return_syntax_error!(
                        "Empty expression found. Expected a value, variable, or expression.",
                        token_stream.current_location().to_error_location(string_table),
                        {
                            CompilationStage => "Expression Parsing",
                            PrimarySuggestion => "Add a value, variable reference, or expression inside the parentheses",
                        }
                    );
                }

                break;
            }

            TokenKind::OpenParenthesis => {
                // Move past the open parenthesis before calling this function again
                // Removed this at one point for a test caused a wonderful infinite loop
                token_stream.advance();

                let value = create_expression(
                    token_stream,
                    context,
                    data_type,
                    ownership,
                    true,
                    string_table,
                )?;

                expression.push(AstNode {
                    kind: NodeKind::Rvalue(value),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });

                // create_expression(..., consume_closing_parenthesis = true) already advanced
                // past the closing parenthesis, so do not advance again in this loop iteration.
                continue;
            }

            // COLLECTION
            TokenKind::OpenCurly => {
                match &data_type {
                    DataType::Collection(inner_type, _) => {
                        expression.push(AstNode {
                            kind: NodeKind::Rvalue(new_collection(
                                token_stream,
                                inner_type,
                                context,
                                ownership,
                                string_table,
                            )?),
                            location: token_stream.current_location(),
                            scope: context.scope.clone(),
                        });
                    }

                    DataType::Inferred => {
                        expression.push(AstNode {
                            kind: NodeKind::Rvalue(new_collection(
                                token_stream,
                                &DataType::Inferred,
                                context,
                                ownership,
                                string_table,
                            )?),
                            location: token_stream.current_location(),
                            scope: context.scope.clone(),
                        });
                    }

                    // Need to error here as a collection literal is being made with the wrong type declaration
                    _ => {
                        return_type_error!(
                            format!("Expected a collection, but assigned variable with a literal type of: {:?}", &data_type),
                            token_stream.current_location().to_error_location(&string_table),
                            {
                                ExpectedType => "Collection",
                                CompilationStage => "Expression Parsing",
                                PrimarySuggestion => "Change the variable type to a collection or use a different literal",
                            }
                        )
                    }
                };
            }

            TokenKind::CloseCurly
            | TokenKind::Comma
            | TokenKind::Eof
            | TokenKind::TemplateClose
            | TokenKind::Arrow
            | TokenKind::EndTemplateHead
            | TokenKind::Colon
            | TokenKind::End => {
                ast_log!("Breaking out of expression");

                if consume_closing_parenthesis {
                    return_syntax_error!(
                        format!("Unexpected token: '{:?}'. Seems to be missing a closing parenthesis at the end of this expression.", token),
                        token_stream.current_location().to_error_location(&string_table),
                        {
                            CompilationStage => "Expression Parsing",
                            PrimarySuggestion => "Add a closing parenthesis ')' at the end of the expression",
                            SuggestedInsertion => ")",
                        }
                    )
                }

                break;
            }

            TokenKind::Newline => {
                // Fine if inside parenthesis (not closed yet)
                // Or the previous token continues the expression
                // Otherwise break out of the expression
                if consume_closing_parenthesis
                    || token_stream.previous_token().continues_expression()
                {
                    token_stream.skip_newlines();
                    continue;
                }

                // No need to skip additional newlines, as the tokenizer removed duplicates?
                // If the next token also continues this expression after newlines
                // then don't break out of the expression yet
                while let Some(token) = token_stream.peek_next_token() {
                    if token.continues_expression() {
                        // Skip this newline
                        token_stream.skip_newlines();
                        continue;
                    }
                    break;
                }

                ast_log!("Breaking out of expression with newline");
                break;
            }

            // --------------------------------------------
            // REFERENCE OR FUNCTION CALL INSIDE EXPRESSION
            // --------------------------------------------
            TokenKind::Symbol(ref id, ..) => {
                let full_name = context.scope.to_owned().append(*id);

                if let Some(arg) = context.get_reference(id) {
                    if context.kind == ContextKind::Constant {
                        if !arg.value.is_compile_time_constant() {
                            return_rule_error!(
                                format!(
                                    "Constants can only reference other constants. '{}' resolves to a non-constant value.",
                                    string_table.resolve(*id)
                                ),
                                token_stream.current_location().to_error_location(&string_table),
                                {
                                    CompilationStage => "Expression Parsing",
                                    PrimarySuggestion => "Only reference constants in constant declarations and const templates",
                                }
                            );
                        }

                        let mut inlined_expression = arg.value.to_owned();
                        inlined_expression.ownership = Ownership::ImmutableOwned;
                        expression.push(AstNode {
                            kind: NodeKind::Rvalue(inlined_expression),
                            location: token_stream.current_location(),
                            scope: context.scope.clone(),
                        });
                        token_stream.advance();
                        continue;
                    }

                    match &arg.value.data_type {
                        DataType::Function(_, signature) => {
                            // Advance past the function name to position at the opening parenthesis
                            token_stream.advance();

                            // This is a function call - parse it using the function call parser
                            let function_call_node = parse_function_call(
                                token_stream,
                                &arg.id,
                                context,
                                &signature,
                                string_table,
                            )?;

                            // -------------------------------
                            // FUNCTION CALL INSIDE EXPRESSION
                            // -------------------------------
                            if let NodeKind::FunctionCall {
                                name,
                                args,
                                returns,
                                location,
                            } = function_call_node.kind
                            {
                                let func_call_expr =
                                    Expression::function_call(name, args, returns, location);

                                expression.push(AstNode {
                                    kind: NodeKind::Rvalue(func_call_expr),
                                    location: function_call_node.location,
                                    scope: context.scope.clone(),
                                });

                                continue;
                            }
                        }

                        DataType::Struct(fields, struct_ownership) => {
                            if token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis) {
                                let constructor_location = token_stream.current_location();

                                // Move to '(' then to first argument.
                                token_stream.advance();
                                token_stream.advance();

                                if token_stream.current_token_kind() == &TokenKind::CloseParenthesis
                                {
                                    let missing_required = fields
                                        .iter()
                                        .filter(|field| {
                                            matches!(field.value.kind, ExpressionKind::None)
                                        })
                                        .count();

                                    if missing_required > 0 {
                                        return_syntax_error!(
                                            format!(
                                                "Struct constructor requires {missing_required} field argument(s) without defaults, but no arguments were provided.",
                                            ),
                                            token_stream.current_location().to_error_location(string_table),
                                            {
                                                CompilationStage => "Expression Parsing",
                                                PrimarySuggestion => "Provide required struct field arguments or define defaults for them",
                                            }
                                        );
                                    }

                                    token_stream.advance();

                                    expression.push(AstNode {
                                        kind: NodeKind::Rvalue(Expression::struct_instance(
                                            fields.to_owned(),
                                            constructor_location,
                                            struct_ownership.get_owned(),
                                        )),
                                        location: token_stream.current_location(),
                                        scope: context.scope.clone(),
                                    });

                                    continue;
                                }

                                let required_field_types = fields
                                    .iter()
                                    .map(|field| field.value.data_type.to_owned())
                                    .collect::<Vec<_>>();
                                let constructor_context =
                                    context.new_child_expression(required_field_types);
                                let args = create_multiple_expressions(
                                    token_stream,
                                    &constructor_context,
                                    true,
                                    string_table,
                                )?;

                                let mut struct_fields = Vec::with_capacity(fields.len());
                                for (field, value) in fields.iter().zip(args.into_iter()) {
                                    struct_fields.push(
                                        crate::compiler_frontend::ast::ast_nodes::Declaration {
                                            id: field.id.to_owned(),
                                            value,
                                        },
                                    );
                                }

                                expression.push(AstNode {
                                    kind: NodeKind::Rvalue(Expression::struct_instance(
                                        struct_fields,
                                        constructor_location,
                                        struct_ownership.get_owned(),
                                    )),
                                    location: token_stream.current_location(),
                                    scope: context.scope.clone(),
                                });

                                continue;
                            }

                            // Fall through to normal reference behavior for non-constructor uses.
                            expression.push(create_reference(
                                token_stream,
                                arg,
                                context,
                                string_table,
                            )?);

                            continue;
                        }

                        // --------------------------
                        // VARIABLE INSIDE EXPRESSION
                        // --------------------------
                        _ => {
                            // If this is a constant,
                            // just copy the value even if its a reference
                            // TODO: is_constant currently does word size types, but may be extended to everything in the future
                            // This means a check needs to be done for whether this should be copied or not (to avoid bloated binary size)
                            // The copy should only happen in those cases if this is a coerse to string expression or word sized type
                            // Referencing preserves aliasing semantics for borrow checking.
                            expression.push(create_reference(
                                token_stream,
                                arg,
                                context,
                                string_table,
                            )?);

                            continue; // Will have moved onto the next token already
                        }
                    }
                }

                // ------------------------------------
                // HOST FUNCTION CALL INSIDE EXPRESSION
                // ------------------------------------
                if let Some(host_func_def) = context
                    .host_registry
                    .get_function(string_table.resolve(*id))
                {
                    if context.kind == ContextKind::Constant {
                        return_rule_error!(
                            format!(
                                "Constants cannot call host functions. '{}' is a runtime host call.",
                                string_table.resolve(*id)
                            ),
                            token_stream.current_location().to_error_location(&string_table),
                            {
                                CompilationStage => "Expression Parsing",
                                PrimarySuggestion => "Use only compile-time constant values inside constants and const templates",
                            }
                        );
                    }

                    // Convert return types to Arg format
                    let signature = host_func_def.params_to_signature(string_table);

                    // This is a function call - parse it using the function call parser
                    let function_call_node = parse_function_call(
                        token_stream,
                        &full_name,
                        context,
                        &signature,
                        string_table,
                    )?;

                    if let NodeKind::HostFunctionCall {
                        name: host_function_id,
                        args,
                        returns,
                        location,
                    } = function_call_node.kind
                    {
                        let func_call_expr = Expression::host_function_call(
                            host_function_id,
                            args.to_owned(),
                            signature.returns,
                            location,
                        );

                        expression.push(AstNode {
                            kind: NodeKind::Rvalue(func_call_expr),
                            location: TextLocation::default(),
                            scope: context.scope.clone(),
                        });

                        continue;
                    }
                }

                let var_name_static: &'static str =
                    Box::leak(string_table.resolve(*id).to_string().into_boxed_str());
                return_rule_error!(
                    format!("Undefined variable '{}'. Variable must be declared before use.", var_name_static),
                    token_stream.current_location().to_error_location(&string_table),
                    {
                        VariableName => var_name_static,
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Declare the variable before using it in this expression",
                    }
                )
            }

            // Check if is a literal
            TokenKind::FloatLiteral(mut float) => {
                if next_number_negative {
                    float = -float;
                    next_number_negative = false;
                }

                let location = token_stream.current_location();

                let float_expr =
                    Expression::float(float, location.to_owned(), ownership.to_owned());

                expression.push(AstNode {
                    kind: NodeKind::Rvalue(float_expr),
                    location,
                    scope: context.scope.clone(),
                });
            }

            TokenKind::IntLiteral(mut int) => {
                if next_number_negative {
                    next_number_negative = false;
                    int = -int;
                };

                let location = token_stream.current_location();

                let int_expr = Expression::int(int, location.to_owned(), ownership.to_owned());

                expression.push(AstNode {
                    kind: NodeKind::Rvalue(int_expr),
                    scope: context.scope.clone(),
                    location,
                });
            }

            TokenKind::StringSliceLiteral(ref string) => {
                let location = token_stream.current_location();

                let string_expr =
                    Expression::string_slice(*string, location.to_owned(), ownership.to_owned());

                expression.push(AstNode {
                    kind: NodeKind::Rvalue(string_expr),
                    scope: context.scope.clone(),
                    location,
                });
            }

            TokenKind::TemplateHead => {
                let template = Template::new(
                    token_stream,
                    new_template_context!(context),
                    None,
                    string_table,
                )?;

                match template.kind {
                    TemplateType::StringFunction => {
                        if context.kind == ContextKind::Constant {
                            return_rule_error!(
                                "Constants and const templates require compile-time template folding. This template is runtime.",
                                token_stream.current_location().to_error_location(&string_table),
                                {
                                    CompilationStage => "Expression Parsing",
                                    PrimarySuggestion => "Remove runtime values from this template so it can fold at compile time",
                                }
                            );
                        }

                        // Check if we need to consume a closing parenthesis after the template
                        if consume_closing_parenthesis
                            && token_stream.current_token_kind() == &TokenKind::CloseParenthesis
                        {
                            token_stream.advance();
                        }
                        return Ok(Expression::template(template, ownership.to_owned()));
                    }

                    TemplateType::String => {
                        ast_log!("Template is foldable. Folding...");

                        let folded_string = template.fold(&None, string_table)?;
                        let interned = folded_string;

                        // Check if we need to consume a closing parenthesis after the template
                        if consume_closing_parenthesis
                            && token_stream.current_token_kind() == &TokenKind::CloseParenthesis
                        {
                            token_stream.advance();
                        }

                        return Ok(Expression::string_slice(
                            interned,
                            token_stream.current_location(),
                            ownership.get_owned(),
                        ));
                    }

                    // Ignore comments
                    TemplateType::Comment => {}

                    // Error for anything else for now
                    TemplateType::Slot => {
                        return_compiler_error!(
                            "Slots are not supported in templates at the moment. They might be removed completely in the future."
                        );
                    }
                }
            }

            TokenKind::Hash => {
                if token_stream.peek_next_token() != Some(&TokenKind::TemplateHead) {
                    return_type_error!(
                        "Unexpected '#' in expression. '#' is only valid before a template head.",
                        token_stream.current_location().to_error_location(&string_table),
                        {
                            CompilationStage => "Expression Parsing",
                            PrimarySuggestion => "Remove '#' or place it directly before a template expression",
                        }
                    );
                }
            }

            TokenKind::BoolLiteral(value) => {
                let location = token_stream.current_location();

                let bool_expr = Expression::bool(value, location.to_owned(), ownership.to_owned());

                expression.push(AstNode {
                    kind: NodeKind::Rvalue(bool_expr),
                    location,
                    scope: context.scope.clone(),
                });
            }

            TokenKind::CharLiteral(value) => {
                let location = token_stream.current_location();

                let char_expr = Expression::char(value, location.to_owned(), ownership.to_owned());

                expression.push(AstNode {
                    kind: NodeKind::Rvalue(char_expr),
                    location,
                    scope: context.scope.clone(),
                });
            }

            // OPERATORS
            // Will push as a string, so shunting yard can handle it later just as a string
            TokenKind::Negative => {
                next_number_negative = true;
            }

            // Ranges and Loops
            TokenKind::In => {
                // Breaks out of the current expression and changes the type to Range
                token_stream.advance();
                return evaluate_expression(
                    &context.scope,
                    expression,
                    &mut DataType::Range,
                    &ownership.get_reference(),
                    string_table,
                );
            }

            // BINARY OPERATORS
            TokenKind::Add => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::Add),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }

            TokenKind::Subtract => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::Subtract),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }

            TokenKind::Multiply => expression.push(AstNode {
                kind: NodeKind::Operator(Operator::Multiply),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            }),

            TokenKind::Divide => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::Divide),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }

            TokenKind::Exponent => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::Exponent),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }

            TokenKind::Modulus => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::Modulus),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }

            // LOGICAL OPERATORS
            TokenKind::Is => {
                // Check if the next token is a "not" or the start of a match statement
                match token_stream.peek_next_token() {
                    // IS NOT
                    Some(TokenKind::Not) => {
                        token_stream.advance();
                        expression.push(AstNode {
                            kind: NodeKind::Operator(Operator::NotEqual),
                            location: token_stream.current_location(),
                            scope: context.scope.clone(),
                        });
                    }

                    // MATCH STATEMENTS
                    Some(TokenKind::Colon) => {
                        // Match statements have a colon right after the "is".
                        // Currently, this should only match on one value
                        // So, we should make sure if there is a colon now, there is just one valid expression being matched
                        if expression.len() > 1 {
                            return_type_error!(
                                format!("Match statements can only have one value to match against. Found: {}", expression.len()),
                                token_stream.current_location().to_error_location(&string_table),
                                {
                                    CompilationStage => "Expression Parsing",
                                    PrimarySuggestion => "Simplify the expression to a single value before the 'is:' match",
                                }
                            )
                        }

                        return evaluate_expression(
                            &context.scope,
                            expression,
                            data_type,
                            ownership,
                            string_table,
                        );
                    }

                    // IS
                    _ => expression.push(AstNode {
                        kind: NodeKind::Operator(Operator::Equality),
                        location: token_stream.current_location(),
                        scope: context.scope.clone(),
                    }),
                }
            }

            TokenKind::LessThan => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::LessThan),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }
            TokenKind::LessThanOrEqual => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::LessThanOrEqual),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }
            TokenKind::GreaterThan => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::GreaterThan),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }
            TokenKind::GreaterThanOrEqual => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::GreaterThanOrEqual),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }
            TokenKind::And => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::And),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }
            TokenKind::Or => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::Or),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }
            TokenKind::Not => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::Not),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }

            TokenKind::ExclusiveRange => expression.push(AstNode {
                kind: NodeKind::Operator(Operator::Range),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            }),

            // For mutating references
            TokenKind::AddAssign => {}

            _ => {
                return_type_error!(
                    format!("Invalid token used in expression: '{:?}'", token),
                    token_stream.current_location().to_error_location(&string_table),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Remove or replace this token with a valid expression element",
                    }
                )
            }
        }

        token_stream.advance();
    }

    evaluate_expression(
        &context.scope,
        expression,
        data_type,
        ownership,
        string_table,
    )
}

/// Parse an expression until one of the provided stop tokens is reached.
/// The stop token is not consumed from the original stream.
pub(crate) fn create_expression_until(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    data_type: &mut DataType,
    ownership: &Ownership,
    stop_tokens: &[TokenKind],
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    if stop_tokens.is_empty() {
        return create_expression(
            token_stream,
            context,
            data_type,
            ownership,
            false,
            string_table,
        );
    }

    let start_index = token_stream.index;
    let mut end_index = token_stream.index;
    let mut parenthesis_depth: i32 = 0;

    while end_index < token_stream.length {
        let token_kind = &token_stream.tokens[end_index].kind;

        match token_kind {
            TokenKind::OpenParenthesis => parenthesis_depth += 1,
            TokenKind::CloseParenthesis if parenthesis_depth > 0 => parenthesis_depth -= 1,
            _ => {}
        }

        // Delimiters only terminate at top-level depth so nested subexpressions remain intact.
        if parenthesis_depth == 0 && stop_tokens.iter().any(|stop| token_kind == stop) {
            break;
        }

        if matches!(token_kind, TokenKind::Eof) {
            break;
        }

        end_index += 1;
    }

    if end_index >= token_stream.length {
        let expected_tokens = stop_tokens
            .iter()
            .map(|token| format!("{token:?}"))
            .collect::<Vec<_>>()
            .join(", ");

        return_syntax_error!(
            format!(
                "Expected one of [{}] to end this expression, but reached end of file",
                expected_tokens
            ),
            token_stream.current_location().to_error_location(string_table),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Complete the expression and add the required delimiter token",
            }
        )
    }

    if end_index == start_index {
        return_syntax_error!(
            "Expected an expression before this delimiter",
            token_stream.tokens[end_index]
                .location
                .to_error_location(string_table),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Add a valid expression before this token",
            }
        )
    }

    if !stop_tokens
        .iter()
        .any(|stop| token_stream.tokens[end_index].kind == *stop)
    {
        let expected_tokens = stop_tokens
            .iter()
            .map(|token| format!("{token:?}"))
            .collect::<Vec<_>>()
            .join(", ");

        return_syntax_error!(
            format!(
                "Expected one of [{}] to end this expression",
                expected_tokens
            ),
            token_stream.tokens[end_index]
                .location
                .to_error_location(string_table),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Add the required delimiter token after this expression",
            }
        )
    }

    let mut expression_tokens = token_stream.tokens[start_index..end_index].to_vec();
    expression_tokens.push(Token::new(
        TokenKind::Eof,
        token_stream.tokens[end_index].location.clone(),
    ));

    let mut scoped_stream = FileTokens::new(token_stream.src_path.clone(), expression_tokens);
    let expression = create_expression(
        &mut scoped_stream,
        context,
        data_type,
        ownership,
        false,
        string_table,
    )?;

    token_stream.index = end_index;
    Ok(expression)
}

// pub fn create_args_from_types(data_types: &[DataType]) -> Vec<Arg> {
//     let mut arguments = Vec::new();
//
//     for data_type in data_types {
//         if let DataType::Args(inner_args) = data_type {
//             arguments.extend(inner_args.to_owned());
//         }
//     }
//
//     arguments
// }
