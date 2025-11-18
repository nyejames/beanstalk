use super::eval_expression::evaluate_expression;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast::ContextKind;
use crate::compiler::parsers::ast::ScopeContext;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::{Expression, Operator};
use crate::compiler::parsers::statements::collections::new_collection;
use crate::compiler::parsers::statements::create_template_node::Template;
use crate::compiler::parsers::statements::functions::parse_function_call;
use crate::compiler::parsers::statements::template::TemplateType;
use crate::compiler::parsers::statements::variables::create_reference;
use crate::compiler::parsers::tokenizer::tokens::{FileTokens, TextLocation, TokenKind};
use crate::compiler::string_interning::StringTable;
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
    string_table: &mut StringTable,
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
            string_table,
        )?;

        expressions.push(expression);
        type_index += 1;

        // Check for tokens breaking out of the expression chain
        match token_stream.current_token_kind() {
            &TokenKind::Comma => {
                if type_index >= context.returns.len() {
                    return_type_error!(
                        format!("Too many arguments provided. Expected: {}. Provided: {}.", context.returns.len(), expressions.len()),
                        token_stream.current_location().to_error_location(&string_table),
                        {
                            CompilationStage => "Expression Parsing",
                            PrimarySuggestion => "Remove extra arguments to match the expected count",
                        }
                    )
                }

                token_stream.advance(); // Skip the comma
            }

            _ => {
                if type_index < context.returns.len() {
                    return_type_error!(
                        format!("Too few arguments provided. Expected: {}. Provided: {}.", context.returns.len(), expressions.len()),
                        token_stream.current_location().to_error_location(&string_table),
                        {
                            CompilationStage => "Expression Parsing",
                            PrimarySuggestion => "Add missing arguments to match the expected count",
                        }
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
    string_table: &mut StringTable,
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
                        "Empty expression found. Expected a value, variable, or expression.",
                        token_stream.current_location().to_error_location(&string_table),
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

                let value = create_expression(token_stream, context, data_type, ownership, true, string_table)?;

                expression.push(AstNode {
                    kind: NodeKind::Expression(value),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
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
                                string_table,
                            )?),
                            location: token_stream.current_location(),
                            scope: context.scope.clone(),
                        });
                    }

                    DataType::Inferred => {
                        expression.push(AstNode {
                            kind: NodeKind::Expression(new_collection(
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
            TokenKind::Symbol(ref id, ..) => {
                if let Some(arg) = context.get_reference(id) {
                    match &arg.value.data_type {
                        DataType::Function(signature) => {
                            // Advance past the function name to position at the opening parenthesis
                            token_stream.advance();

                            // This is a function call - parse it using the function call parser
                            let function_call_node = parse_function_call(
                                token_stream,
                                id,
                                context,
                                signature,
                                string_table,
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
                                    scope: context.scope.clone(),
                                });

                                continue;
                            }
                        }

                        // --------------------------
                        // VARIABLE INSIDE EXPRESSION
                        // --------------------------
                        _ => {
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
                if let Some(host_func_def) = context.host_registry.get_function(id) {

                    // Convert return types to Arg format
                    let signature = host_func_def.params_to_signature(string_table);

                    // This is a function call - parse it using the function call parser
                    let function_call_node = parse_function_call(
                        token_stream,
                        id,
                        context,
                        &signature,
                        string_table,
                    )?;

                    if let NodeKind::HostFunctionCall(func_name, expressions, _returns, _module_name, _wasm_import_name, location) = function_call_node.kind {
                        let func_call_expr = Expression::function_call(
                            func_name.to_owned(),
                            expressions.to_owned(),
                            signature.returns,
                            location,
                        );

                        expression.push(AstNode {
                            kind: NodeKind::Expression(func_call_expr),
                            location: TextLocation::default(),
                            scope: context.scope.clone(),
                        });

                        continue;
                    }
                }

                {
                    let var_name_static: &'static str = Box::leak(string_table.resolve(*id).to_string().into_boxed_str());
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
                    scope: context.scope.clone(),
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
                    scope: context.scope.clone(),
                    location,
                });
            }

            TokenKind::StringSliceLiteral(ref string) => {
                let location = token_stream.current_location();

                let string_expr = Expression::string_slice(
                    *string,
                    location.to_owned(),
                    ownership.to_owned(),
                );

                expression.push(AstNode {
                    kind: NodeKind::Expression(string_expr),
                    scope: context.scope.clone(),
                    location,
                });
            }

            TokenKind::TemplateHead | TokenKind::ParentTemplate => {
                let mut template =
                    Template::new(token_stream, new_template_context!(context), None, string_table)?;

                match template.kind {
                    TemplateType::StringFunction => {
                        return Ok(Expression::template(template, ownership.to_owned()));
                    }

                    TemplateType::String => {
                        let folded_string = template.fold(&None, string_table)?;
                        let interned = folded_string;

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
                    _ => {
                        expression.push(AstNode {
                            kind: NodeKind::Operator(Operator::Equality),
                            location: token_stream.current_location(),
                            scope: context.scope.clone(),
                        })
                    }
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

            TokenKind::Range => expression.push(AstNode {
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
