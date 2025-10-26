use crate::compiler::parsers::ast::ContextKind;
use super::eval_expression::evaluate_expression;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast::ScopeContext;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode, NodeKind};
use crate::compiler::parsers::collections::new_collection;
use crate::compiler::parsers::expressions::expression::{Expression, Operator};
use crate::compiler::parsers::statements::create_template_node::Template;
use crate::compiler::parsers::statements::functions::{parse_function_call, FunctionSignature};
use crate::compiler::parsers::statements::variables::create_reference;
use crate::compiler::parsers::template::TemplateType;
use crate::compiler::parsers::tokens::{FileTokens, TextLocation, TokenKind};
use crate::compiler::traits::ContainsReferences;
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
) -> Result<Vec<Expression>, CompileError> {
    let mut expressions: Vec<Expression> = Vec::new();
    let mut type_index = 0;

    while token_stream.index < token_stream.length && type_index < context.returns.len() {
        let mut expected_arg = context.returns[type_index].to_owned();

        // This should type check here
        let expression = create_expression(
            token_stream,
            context,
            &mut expected_arg.value.data_type,
            &expected_arg.value.ownership,
            consume_closing_parenthesis,
        )?;

        expressions.push(expression);
        type_index += 1;

        // Check for tokens breaking out of the expression chain
        match token_stream.current_token_kind() {
            &TokenKind::Comma => {
                if type_index >= context.returns.len() {
                    return_type_error!(
                        token_stream.current_location(),
                        "Too many arguments provided. Expected: {}. Provided: {}.",
                        context.returns.len(),
                        expressions.len()
                    )
                }

                token_stream.advance(); // Skip the comma
            }

            _ => {
                if type_index < context.returns.len() {
                    return_type_error!(
                        token_stream.current_location(),
                        "Too few arguments provided. Expected: {}. Provided: {}.",
                        context.returns.len(),
                        expressions.len()
                    )
                }
            }
        }
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
) -> Result<Expression, CompileError> {
    let mut expression: Vec<AstNode> = Vec::new();
    // let mut number_union = get_any_number_datatype(false);

    ast_log!("Parsing {} Expression", data_type.to_string());

    // Loop through the expression and create the AST nodes
    // Figure out the type it should be from the data
    // DOES NOT MOVE TOKENS PAST THE CLOSING TOKEN
    let mut next_number_negative = false;
    while token_stream.index < token_stream.length {
        let token = token_stream.current_token_kind().to_owned();
        ast_log!("Parsing token (expression): {:?}", token);

        match token {
            TokenKind::CloseParenthesis => {
                if consume_closing_parenthesis {
                    token_stream.advance();
                }

                if expression.is_empty() {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Empty expression found. Expected a value, variable, or expression."
                    );
                }

                break;
            }

            TokenKind::OpenParenthesis => {
                // Move past the open parenthesis before calling this function again
                // Removed this at one point for a test caused a wonderful infinite loop
                token_stream.advance();

                let value = create_expression(token_stream, context, data_type, ownership, true)?;

                expression.push(AstNode {
                    kind: NodeKind::Expression(value),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }

            // COLLECTION
            TokenKind::OpenCurly => {
                match &data_type {
                    DataType::Collection(inner_type, _) => {
                        expression.push(AstNode {
                            kind: NodeKind::Expression(new_collection(
                                token_stream,
                                inner_type,
                                context,
                                ownership,
                            )?),
                            location: token_stream.current_location(),
                            scope: context.scope_name.to_owned(),
                        });
                    }

                    DataType::Inferred => {
                        expression.push(AstNode {
                            kind: NodeKind::Expression(new_collection(
                                token_stream,
                                &DataType::Inferred,
                                context,
                                ownership,
                            )?),
                            location: token_stream.current_location(),
                            scope: context.scope_name.to_owned(),
                        });
                    }

                    // Need to error here as a collection literal is being made with the wrong type declaration
                    _ => {
                        return_type_error!(
                            token_stream.current_location(),
                            "Expected a collection, but assigned variable with a literal type of: {:?}",
                            &data_type
                        )
                    }
                };
            }

            TokenKind::CloseCurly
            // No longer supporting struct literals this way
            // | TokenKind::StructBracket
            | TokenKind::Comma
            | TokenKind::Eof
            | TokenKind::TemplateClose
            | TokenKind::Arrow
            | TokenKind::Colon
            | TokenKind::End => {
                ast_log!("Breaking out of expression");

                if consume_closing_parenthesis {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Unexpected token: '{:?}'. Seems to be missing a closing parenthesis at the end of this expression.",
                        token
                    )
                }

                break;
            }

            TokenKind::Newline => {
                // Fine if inside parenthesis (not closed yet)
                // Otherwise break out of the expression
                if consume_closing_parenthesis {
                    token_stream.skip_newlines();
                    continue;
                }

                ast_log!("Breaking out of expression with newline");
                break;
            }

            // --------------------------------------------
            // REFERENCE OR FUNCTION CALL INSIDE EXPRESSION
            // --------------------------------------------
            TokenKind::Symbol(ref name, ..) => {
                if let Some(arg) = context.get_reference(name) {
                    match &arg.value.data_type {
                        DataType::Function(signature) => {
                            // Advance past the function name to position at the opening parenthesis
                            token_stream.advance();

                            // This is a function call - parse it using the function call parser
                            let function_call_node = parse_function_call(
                                token_stream,
                                name,
                                context,
                                signature
                            )?;

                            // -------------------------------
                            // FUNCTION CALL INSIDE EXPRESSION
                            // -------------------------------
                            if let NodeKind::FunctionCall(func_name, args, returns, location) = function_call_node.kind {
                                let func_call_expr = Expression::function_call(
                                    func_name,
                                    args,
                                    returns,
                                    location,
                                );

                                expression.push(AstNode {
                                    kind: NodeKind::Expression(func_call_expr),
                                    location: function_call_node.location,
                                    scope: context.scope_name.to_owned(),
                                });

                                continue;
                            }
                        }

                        // --------------------------
                        // VARIABLE INSIDE EXPRESSION
                        // --------------------------
                        _ => {
                            expression.push(create_reference(token_stream, arg, context)?);

                            continue; // Will have moved onto the next token already
                        }
                    }
                }

                // ------------------------------------
                // HOST FUNCTION CALL INSIDE EXPRESSION
                // ------------------------------------
                if let Some(host_func_def) = context.host_registry.get_function(name) {

                    // Convert return types to Arg format
                    let converted_returns = host_func_def.return_types
                        .iter()
                        .map(|x| x.to_arg())
                        .collect::<Vec<Arg>>();
                    
                    let signature = FunctionSignature {
                        parameters: host_func_def.parameters.clone(),
                        returns: converted_returns.clone(),
                    };

                    // This is a function call - parse it using the function call parser
                    let function_call_node = parse_function_call(
                        token_stream,
                        name,
                        context,
                        &signature,
                    )?;

                    if let NodeKind::HostFunctionCall(func_name, expressions, _returns, _module_name, _wasm_import_name, location) = function_call_node.kind {
                        let func_call_expr = Expression::function_call(
                            func_name.to_owned(),
                            expressions.to_owned(),
                            converted_returns,
                            location,
                        );

                        expression.push(AstNode {
                            kind: NodeKind::Expression(func_call_expr),
                            location: TextLocation::default(),
                            scope: context.scope_name.to_owned(),
                        });

                        continue;
                    }
                }

                return_rule_error!(
                    token_stream.current_location(),
                    "Undefined variable '{}'. Variable must be declared before use.",
                    name,
                )
            }

            // Check if is a literal
            TokenKind::FloatLiteral(mut float) => {
                if next_number_negative {
                    float = -float;
                    next_number_negative = false;
                }

                let location = token_stream.current_location();

                let float_expr = Expression::float(
                    float,
                    location.to_owned(),
                    ownership.to_owned(),
                );

                expression.push(AstNode {
                    kind: NodeKind::Expression(float_expr),
                    location,
                    scope: context.scope_name.to_owned(),
                });
            }

            TokenKind::IntLiteral(mut int) => {
                if next_number_negative {
                    next_number_negative = false;
                    int = -int;
                };

                let location = token_stream.current_location();

                let int_expr = Expression::int(
                    int,
                    location.to_owned(),
                    ownership.to_owned(),
                );

                expression.push(AstNode {
                    kind: NodeKind::Expression(int_expr),
                    scope: context.scope_name.to_owned(),
                    location,
                });
            }

            TokenKind::StringSliceLiteral(ref string) => {
                let location = token_stream.current_location();

                let string_expr = Expression::string_slice(
                    string.to_owned(),
                    location.to_owned(),
                    ownership.to_owned(),
                );

                expression.push(AstNode {
                    kind: NodeKind::Expression(string_expr),
                    scope: context.scope_name.to_owned(),
                    location,
                });
            }

            TokenKind::TemplateHead | TokenKind::ParentTemplate => {
                let mut template =
                    Template::new(token_stream, new_template_context!(context), None)?;

                match template.kind {
                    TemplateType::StringFunction => {
                        return Ok(Expression::template(template, ownership.to_owned()));
                    }

                    TemplateType::CompileTimeString => {
                        return Ok(Expression::string_slice(
                            template.fold(&None)?,
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

            TokenKind::BoolLiteral(value) => {
                let location = token_stream.current_location();

                let bool_expr = Expression::bool(
                    value,
                    location.to_owned(),
                    ownership.to_owned(),
                );

                expression.push(AstNode {
                    kind: NodeKind::Expression(bool_expr),
                    location,
                    scope: context.scope_name.to_owned(),
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
                    context.scope_name.to_owned(),
                    expression,
                    &mut DataType::Range,
                    &ownership.get_reference(),
                );
            }

            // BINARY OPERATORS
            TokenKind::Add => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::Add),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }

            TokenKind::Subtract => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::Subtract),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }

            TokenKind::Multiply => expression.push(AstNode {
                kind: NodeKind::Operator(Operator::Multiply),
                location: token_stream.current_location(),
                scope: context.scope_name.to_owned(),
            }),

            TokenKind::Divide => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::Divide),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }

            TokenKind::Exponent => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::Exponent),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }

            TokenKind::Modulus => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::Modulus),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }

            // LOGICAL OPERATORS
            TokenKind::Is => {
                // Check if the next token is a "not" or the start of a match statement
                match token_stream.peek_next_token() {

                    // // IS NOT
                    // Some(TokenKind::Not) => {
                    //     token_stream.advance();
                    //     expression.push(AstNode {
                    //         kind: NodeKind::Operator(Operator::NotEqual),
                    //         location: token_stream.current_location(),
                    //         scope: context.scope_name.to_owned(),
                    //     });
                    // }

                    // MATCH STATEMENTS
                    Some(TokenKind::Colon) => {
                        // Match statements have a colon right after the "is".
                        // Currently, this should only match on one value
                        // So, we should make sure if there is a colon now, there is just one valid expression being matched
                        if expression.len() > 1 {
                            return_type_error!(
                                token_stream.current_location(),
                                "Match statements can only have one value to match against. Found: {}",
                                expression.len()
                            )
                        }

                        return evaluate_expression(context.scope_name.to_owned(), expression, data_type, ownership);
                    }

                    // IS
                    _ => {
                        expression.push(AstNode {
                            kind: NodeKind::Operator(Operator::Equality),
                            location: token_stream.current_location(),
                            scope: context.scope_name.to_owned(),
                        })
                    }
                }
            }

            TokenKind::LessThan => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::LessThan),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }
            TokenKind::LessThanOrEqual => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::LessThanOrEqual),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }
            TokenKind::GreaterThan => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::GreaterThan),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }
            TokenKind::GreaterThanOrEqual => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::GreaterThanOrEqual),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }
            TokenKind::And => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::And),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }
            TokenKind::Or => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::Or),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }
            TokenKind::Not => {
                expression.push(AstNode {
                    kind: NodeKind::Operator(Operator::Not),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }

            TokenKind::Range => expression.push(AstNode {
                kind: NodeKind::Operator(Operator::Range),
                location: token_stream.current_location(),
                scope: context.scope_name.to_owned(),
            }),

            // For mutating references
            TokenKind::AddAssign => {}

            _ => {
                return_type_error!(
                    token_stream.current_location(),
                    "Invalid token used in expression: '{:?}'",
                    token,
                )
            }
        }

        token_stream.advance();
    }

    evaluate_expression(
        context.scope_name.to_owned(),
        expression,
        data_type,
        ownership,
    )
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
