#[allow(unused_imports)]
use colour::{red_ln, blue_ln, green_ln};

use super::{
    ast_nodes::{Arg, AstNode},
    expressions::parse_expression::create_expression,
};
use crate::parsers::ast_nodes::{AssignmentOperator, Expr};
use crate::parsers::build_ast::{TokenContext, new_ast};
use crate::parsers::functions::{create_block_signature, parse_function_call};
use crate::parsers::scene::{SceneContent, Style};
use crate::tokenizer::TokenPosition;
use crate::{CompileError, ErrorType, Token, bs_types::DataType};
use crate::parsers::util::combine_two_slices_to_vec;

pub fn create_reference(
    x: &mut TokenContext,
    arg: &Arg,
    captured_declarations: &[Arg],
) -> Result<AstNode, CompileError> {

    // Move past the name
    x.advance();

    match arg.data_type {
        // Function Call
        DataType::Block(ref argument_refs, ref return_types) => parse_function_call(
            x,
            &arg.name,
            captured_declarations,
            argument_refs,
            return_types,
        ),

        _ => {
            Ok(AstNode::Reference(
                Expr::Reference(arg.name.to_owned(), arg.data_type.to_owned()),
                x.token_start_position(),
            ))
        }
    }
}

pub fn new_arg(
    x: &mut TokenContext,
    name: &str,
    variable_declarations: &[Arg],
) -> Result<Arg, CompileError> {
    // Move past the name
    x.advance();

    let mutable = match x.current_token() {
        Token::Mutable => {
            x.advance();
            true
        }
        _ => false,
    };

    let mut data_type = DataType::Inferred(mutable);

    match x.current_token() {
        Token::Assign => {
            x.advance();
        }

        // New Block with args
        Token::ArgConstructor => {
            let (constructor_args, return_type) =
                create_block_signature(x, &mut true, variable_declarations)?;

            // Capture the variables from the surrounding scope (this might change in the future)
            // Maybe only public variables are captured?
            let combined = combine_two_slices_to_vec(&constructor_args, variable_declarations);

            return Ok(Arg {
                name: name.to_owned(),
                default_value: new_ast(x, &combined, &return_type, false)?,
                data_type: DataType::Block(constructor_args, return_type),
            });
        }

        // Block with no args. Only returns itself.
        Token::Colon => {
            x.advance();

            return Ok(Arg {
                name: name.to_owned(),
                default_value: new_ast(
                    x,
                    // TODO: separate imports from parent block so these can be used in the scope
                    variable_declarations, // No args for this block
                    // This implies it will return an instance of itself
                    &[],
                    false,
                )?,
                data_type: DataType::Block(Vec::new(), Vec::new()),
            });
        }

        // Has a type declaration
        Token::DatatypeLiteral(type_keyword) => {
            data_type = type_keyword.to_owned();

            x.advance();

            match x.current_token() {
                Token::Assign => {
                    x.advance();
                }

                // If end of statement, then it's a zero-value variable
                Token::Comma | Token::EOF | Token::Newline | Token::ArgConstructor => {
                    return Ok(create_zero_value_var(data_type, name));
                }

                _ => {
                    return Err(CompileError {
                        msg: format!(
                            "Variable of type: {:?} does not exist in this scope",
                            data_type
                        ),
                        start_pos: x.token_positions[x.index].to_owned(),
                        end_pos: TokenPosition {
                            line_number: x.token_positions[x.index].line_number,
                            char_column: x.token_positions[x.index].char_column + name.len() as i32,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }
            }
        }

        // Collection Type Declaration
        Token::OpenCurly => {
            x.advance();

            // Check if the datatype inside the curly braces is mutable
            let mutable = match x.current_token() {
                Token::Mutable => {
                    x.advance();
                    true
                }
                _ => false,
            };

            // Check if there is a type inside the curly braces
            data_type = match x.current_token().to_owned() {
                Token::DatatypeLiteral(data_type) => {
                    x.advance();
                    DataType::Collection(Box::new(data_type))
                }
                _ => DataType::Collection(Box::new(DataType::Inferred(mutable))),
            };

            // Make sure there is a closing curly brace
            match x.current_token() {
                Token::CloseCurly => {
                    x.advance();
                }
                _ => {
                    return Err(CompileError {
                        msg: "Missing closing curly brace for collection type declaration"
                            .to_owned(),
                        start_pos: x.token_positions[x.index].to_owned(),
                        end_pos: TokenPosition {
                            line_number: x.token_positions[x.index].line_number,
                            char_column: x.token_positions[x.index].char_column + name.len() as i32,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }
            }

            // Should have an assignment operator now
            match x.current_token() {
                Token::Assign => {
                    x.advance();
                }

                // If end of statement, then it's a zero-value variable
                Token::Comma | Token::EOF | Token::Newline | Token::ArgConstructor => {
                    return Ok(create_zero_value_var(data_type, name));
                }

                _ => {
                    return Err(CompileError {
                        msg: "Missing assignment operator for collection type declaration"
                            .to_owned(),
                        start_pos: x.token_positions[x.index].to_owned(),
                        end_pos: TokenPosition {
                            line_number: x.token_positions[x.index].line_number,
                            char_column: x.token_positions[x.index].char_column + name.len() as i32,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }
            }
        }

        Token::Newline => {
            // Ignore
            x.advance();
        }

        // Anything else is a syntax error
        _ => {
            return Err(CompileError {
                msg: format!(
                    "'{}' - Invalid variable declaration: {:?}",
                    name, x.tokens[x.index]
                ),
                start_pos: x.token_positions[x.index].to_owned(),
                end_pos: TokenPosition {
                    line_number: x.token_positions[x.index].line_number,
                    char_column: x.token_positions[x.index].char_column + name.len() as i32,
                },
                error_type: ErrorType::Syntax,
            });
        }
    };

    // The current token should be whatever is after the assignment operator

    // Check if this whole expression is nested in brackets.
    // This is just so we don't wastefully call create_expression recursively right away
    let parsed_expr = match x.current_token() {
        Token::OpenParenthesis => {
            x.advance();
            create_expression(x, &mut data_type, true, variable_declarations, &[])?
        }
        _ => create_expression(x, &mut data_type, false, variable_declarations, &[])?,
    };

    Ok(Arg {
        name: name.to_owned(),
        default_value: parsed_expr,
        data_type,
    })
}

pub fn mutated_arg(
    x: &mut TokenContext,
    arg: Arg,
    captured_declarations: &[Arg],
) -> Result<AstNode, CompileError> {

    if !arg.data_type.is_mutable() {
        return Err(CompileError {
            msg: format!(
                "Variable of type: {:?} is not mutable. Add a '~' to the variable declaration if you want it to be mutable!",
                arg.data_type
            ),
            start_pos: x.token_positions[x.index].to_owned(),
            end_pos: TokenPosition {
                line_number: x.token_positions[x.index].line_number,
                char_column: x.token_positions[x.index].char_column + arg.name.len() as i32,
            },
            error_type: ErrorType::Syntax,
        });
    }

    let assignment_op = match x.current_token() {
        Token::Assign => AssignmentOperator::Assign,

        Token::AddAssign => AssignmentOperator::AddAssign,

        Token::SubtractAssign => AssignmentOperator::SubtractAssign,

        Token::MultiplyAssign => AssignmentOperator::MultiplyAssign,

        Token::DivideAssign => AssignmentOperator::DivideAssign,
        
        // TODO: match on a bunch more things and throw more detailed errors about how this must be a mutation
        _ => {
            return Err(CompileError {
                msg: format!("Invalid operator for mutation: {:?}", x.tokens[x.index]),
                start_pos: x.token_positions[x.index].to_owned(),
                end_pos: TokenPosition {
                    line_number: x.token_positions[x.index].line_number,
                    char_column: x.token_positions[x.index].char_column + arg.name.len() as i32,
                },
                error_type: ErrorType::Syntax,
            });
        }
    };

    x.advance();
    
    let parsed_expression = create_expression(
        x,
        &mut arg.data_type.to_owned(),
        false,
        captured_declarations,
        &[],
    )?;

    Ok(AstNode::Mutation(
        arg.name,
        assignment_op,
        parsed_expression,
        x.token_start_position(),
    ))
}

fn create_zero_value_var(data_type: DataType, name: impl Into<String>) -> Arg {
    match data_type {
        DataType::Float(_) => Arg {
            name: name.into(),
            default_value: Expr::Float(0.0),
            data_type,
        },

        DataType::Int(_) => Arg {
            name: name.into(),
            default_value: Expr::Int(0),
            data_type,
        },

        DataType::Bool(_) => Arg {
            name: name.into(),
            default_value: Expr::Bool(false),
            data_type,
        },

        DataType::Scene(_) => Arg {
            name: name.into(),
            default_value: Expr::Scene(
                SceneContent::default(),
                Style::default(),
                String::default(),
            ),
            data_type,
        },

        DataType::String(_) | DataType::CoerceToString(_) => Arg {
            name: name.into(),
            default_value: Expr::String(String::new()),
            data_type,
        },

        _ => Arg {
            name: name.into(),
            default_value: Expr::None,
            data_type,
        },
    }
}

pub fn get_reference(name: &str, variable_declarations: &[Arg]) -> Option<Arg> {
    variable_declarations
        .iter()
        .rfind(|a| a.name == name)
        .map(|a| a.to_owned())
}
