use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::parsers::build_ast::ContextKind;
#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};

use super::eval_expression::evaluate_expression;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode, NodeKind};
use crate::compiler::parsers::build_ast::ScopeContext;
use crate::compiler::parsers::collections::new_collection;
use crate::compiler::parsers::expressions::expression::{Expression, Operator};
use crate::compiler::parsers::statements::create_template_node::new_template;
use crate::compiler::parsers::statements::variables::create_reference;
use crate::compiler::parsers::template::{Style, TemplateType};
use crate::compiler::parsers::tokens::{TokenContext, TokenKind};
use crate::compiler::traits::ContainsReferences;
use crate::{ast_log, new_template_context, return_syntax_error, return_type_error};
use std::collections::HashMap;

// For multiple returns or function calls
// MUST know all the types
pub fn create_multiple_expressions(
    token_stream: &mut TokenContext,
    context: &ScopeContext,
    consume_closing_parenthesis: bool,
) -> Result<Vec<Expression>, CompileError> {
    let mut expressions: Vec<Expression> = Vec::new();
    let mut type_index = 0;

    while token_stream.index < token_stream.length && type_index < context.returns.len() {
        let mut expression_type = context.returns[type_index].to_owned();

        let expression = create_expression(
            token_stream,
            &context,
            &mut expression_type,
            consume_closing_parenthesis,
        )?;

        if expression_type != context.returns[type_index] {
            return_type_error!(
                token_stream.current_location(),
                "Expected type: {:?}, but got type: {:?}",
                context.returns[type_index],
                expression.data_type
            )
        }

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
    token_stream: &mut TokenContext,
    context: &ScopeContext,
    data_type: &mut DataType,
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

                    // This is for the case this parenthesis is consumed
                    token_stream.skip_newlines();
                }

                if expression.is_empty() {
                    return Ok(Expression::none());
                }

                break;
            }

            TokenKind::OpenParenthesis => {
                // Move past the open parenthesis before calling this function again
                // Removed this at one point for a test caused a wonderful infinite loop
                token_stream.advance();

                let value = create_expression(token_stream, context, data_type, true)?;

                expression.push(AstNode {
                    kind: NodeKind::Reference(value),
                    location: token_stream.current_location(),
                    scope: context.scope_name.to_owned(),
                });
            }

            // COLLECTION
            TokenKind::OpenCurly => {
                match &data_type {
                    DataType::Collection(inner_type, _) => {
                        expression.push(AstNode {
                            kind: NodeKind::Reference(new_collection(
                                token_stream,
                                inner_type,
                                context,
                            )?),
                            location: token_stream.current_location(),
                            scope: context.scope_name.to_owned(),
                        });
                    }

                    DataType::Inferred(mutable) => {
                        expression.push(AstNode {
                            kind: NodeKind::Reference(new_collection(
                                token_stream,
                                &DataType::Inferred(mutable.to_owned()),
                                context,
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
            | TokenKind::StructDefinition
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
                ast_log!("Breaking out of expression with newline");

                if consume_closing_parenthesis {
                    token_stream.skip_newlines();
                    continue;
                } else {
                    // Check ahead if the next token must continue the expression
                    // So something like:
                    // x = 1 + 2
                    // + 3
                    // '+' would be a valid continuation,
                    // as '+' doesn't make sense outside expressions like this anyway
                    token_stream.skip_newlines();

                    match token_stream.current_token_kind() {
                        TokenKind::Add
                        | TokenKind::Subtract
                        | TokenKind::Multiply
                        | TokenKind::Root
                        | TokenKind::Divide
                        | TokenKind::Modulus
                        | TokenKind::Is
                        | TokenKind::GreaterThan
                        | TokenKind::GreaterThanOrEqual
                        | TokenKind::LessThan
                        | TokenKind::LessThanOrEqual
                        | TokenKind::Exponent
                        | TokenKind::Not
                        | TokenKind::Or
                        | TokenKind::Remainder
                        | TokenKind::RemainderAssign
                        | TokenKind::Log => continue,
                        _ => break,
                    }
                }
            }

            // Check if the name is a reference to another variable or function call
            TokenKind::Symbol(ref name, ..) => {
                if let Some(arg) = context.find_reference(name) {
                    expression.push(create_reference(token_stream, arg, context)?);

                    continue; // Will have moved onto the next token already
                } else {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Variable '{}' does not exist in this scope.",
                        name,
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
                expression.push(AstNode {
                    kind: NodeKind::Reference(Expression::float(float, location.to_owned(), context.lifetime)),
                    location,
                    scope: context.scope_name.to_owned(),
                });
            }

            TokenKind::IntLiteral(int) => {
                let int_value = if next_number_negative {
                    next_number_negative = false;
                    -int
                } else {
                    int
                };

                let location = token_stream.current_location();
                expression.push(AstNode {

                    kind: NodeKind::Reference(Expression::int(int_value, location.to_owned())),
                    scope: context.scope_name.to_owned(),
                    location,
                });
            }

            TokenKind::StringLiteral(ref string) => {
                let location = token_stream.current_location();
                expression.push(AstNode {

                    kind: NodeKind::Reference(Expression::string(
                        string.to_owned(),
                        location.to_owned(),
                    )),
                    scope: context.scope_name.to_owned(),
                    location,
                });
            }

            TokenKind::TemplateHead | TokenKind::ParentTemplate => {
                let template_type = new_template(
                    token_stream,
                    new_template_context!(context),
                    &mut HashMap::new(),
                    &mut Style::default(),
                )?;

                match template_type {
                    TemplateType::Template(template) => return Ok(template),

                    // Ignore comments
                    TemplateType::Comment => {}

                    // Error for anything else for now
                    _ => {
                        return_type_error!(
                            token_stream.current_location(),
                            "Unexpected template type used in expression: {:?}",
                            template_type
                        )
                    }
                }
            }

            TokenKind::BoolLiteral(value) => {
                let location = token_stream.current_location();
                expression.push(AstNode {

                    kind: NodeKind::Expression(Expression::bool(
                        value.to_owned(),
                        location.to_owned(),
                    )),
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
                // Check if the next token is a not
                if let Some(TokenKind::Not) = token_stream.peek_next_token() {
                    token_stream.advance();
                    expression.push(AstNode {

                        kind: NodeKind::Operator(Operator::NotEqual),
                        location: token_stream.current_location(),
                        scope: context.scope_name.to_owned(),
                    });
                } else {
                    expression.push(AstNode {

                        kind: NodeKind::Operator(Operator::Equality),
                        location: token_stream.current_location(),
                        scope: context.scope_name.to_owned(),
                    })
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

    evaluate_expression(context.scope_name.to_owned(), expression, data_type)
}

// This is used to unpack all the 'self' values of a block into multiple arguments
pub fn create_args_from_types(data_types: &[DataType]) -> Vec<Arg> {
    let mut arguments = Vec::new();

    for data_type in data_types {
        if let DataType::Args(inner_args) = data_type {
            arguments.extend(inner_args.to_owned());
        }
    }

    arguments
}
