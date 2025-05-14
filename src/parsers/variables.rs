use super::{
    ast_nodes::{Arg, AstNode},
    expressions::parse_expression::create_expression,
};
use crate::parsers::ast_nodes::Expr;
use crate::parsers::build_ast::{TokenContext, new_ast};
use crate::parsers::expressions::parse_expression::get_accessed_args;
use crate::parsers::functions::{create_block_signature, parse_function_call};
use crate::parsers::scene::{SceneContent, Style};
use crate::tokenizer::TokenPosition;
use crate::tokens::VarVisibility;
use crate::{CompileError, ErrorType, Token, bs_types::DataType};

pub fn create_new_var_or_ref(
    x: &mut TokenContext,
    name: &str,
    variable_declarations: &[Arg],
    visibility: &VarVisibility,
) -> Result<AstNode, CompileError> {
    // If this is a reference to a function or variable,
    // This to_owned here is gross, probably a better way to avoid this
    if let Some(arg) = get_reference(name, variable_declarations) {
        return match arg.data_type {
            // Function Call
            DataType::Block(ref argument_refs, ref return_types) => {
                x.advance();
                // blue_ln!("arg value purity: {:?}, for {}",  arg.value.is_pure(), name);
                parse_function_call(x, name, variable_declarations, argument_refs, return_types)
            }

            _ => {
                // Check to make sure there is no access attempt on any other types.
                // Get accessed arg will return an error if there is an access attempt on the wrong type.
                // This SHOULD always be None (for now), but this is being assigned to the reference here.
                // In case the language will change in the future, and properties/methods are added to other types
                let accessed_arg =
                    get_accessed_args(x, &arg.name, &[arg.data_type.to_owned()], &mut Vec::new())?;

                // If the value isn't wrapped in a runtime value,
                // Replace the reference with a literal value

                // DISABLED AS THIS ACTUALLY ISN'T GREAT FOR PERFORMANCE

                // Todo: Evaluate when to do this correctly
                // if arg.expr.is_pure() {
                //     return Ok(AstNode::Literal(arg.expr.to_owned(), x.current_position()));
                // }

                Ok(AstNode::Literal(
                    Expr::Reference(arg.name.to_owned(), arg.data_type.to_owned(), accessed_arg),
                    x.current_position(),
                ))
            }
        };
    };

    let arg = new_arg(x, name, variable_declarations)?;

    Ok(AstNode::VarDeclaration(
        arg.name,
        arg.expr,
        visibility.to_owned(),
        arg.data_type,
        x.current_position(),
    ))
}

pub fn new_arg(
    x: &mut TokenContext,
    name: &str,
    variable_declarations: &[Arg],
) -> Result<Arg, CompileError> {
    // Move past the name
    let mut data_type = DataType::Inferred(false);
    x.advance();

    match x.current_token() {
        Token::Assign => {
            x.advance();
        }

        // New Block with args
        Token::ArgConstructor => {
            let (constructor_args, return_type) =
                create_block_signature(x, &mut true, variable_declarations)?;

            return Ok(Arg {
                name: name.to_owned(),
                expr: new_ast(x, &constructor_args, &return_type)?,
                data_type: DataType::Block(constructor_args, return_type),
            });
        }

        // Block with no args. Only returns itself.
        Token::Colon => {
            x.advance();

            return Ok(Arg {
                name: name.to_owned(),
                expr: new_ast(
                    x,
                    // TODO: separate imports from parent block so these can be used in the scope
                    &[], // No args for this block
                    // This implies it will return an instance of itself
                    &[],
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
                    return Ok(create_zero_value_var(data_type, name.to_string()));
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
            create_expression(x, &mut data_type, true, variable_declarations)?
        }
        _ => create_expression(x, &mut data_type, false, variable_declarations)?,
    };

    Ok(Arg {
        name: name.to_owned(),
        expr: parsed_expr,
        data_type,
    })
}

fn create_zero_value_var(data_type: DataType, name: String) -> Arg {
    match data_type {
        DataType::Float(_) => Arg {
            name,
            expr: Expr::Float(0.0),
            data_type,
        },

        DataType::Int(_) => Arg {
            name,
            expr: Expr::Int(0),
            data_type,
        },

        DataType::Bool(_) => Arg {
            name,
            expr: Expr::Bool(false),
            data_type,
        },

        DataType::Scene(_) => Arg {
            name,
            expr: Expr::Scene(
                SceneContent::default(),
                Style::default(),
                SceneContent::default(),
                String::default(),
            ),
            data_type,
        },

        DataType::String(_) | DataType::CoerceToString(_) => Arg {
            name,
            expr: Expr::String(String::new()),
            data_type,
        },

        _ => Arg {
            name,
            expr: Expr::None,
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
