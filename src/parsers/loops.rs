use crate::bs_types::DataType;
use crate::parsers::ast_nodes::{Arg, AstNode, Expr};
use crate::parsers::build_ast::{TokenContext, new_ast};
use crate::parsers::expressions::parse_expression::create_expression;
use crate::parsers::variables::get_reference;
use crate::{CompileError, ErrorType, Token};
use colour::red_ln;

// Returns a ForLoop node or WhileLoop Node (or error if there's invalid syntax)
pub fn create_loop(
    x: &mut TokenContext,
    returned_types: &[DataType],
    captured_declarations: &[Arg],
) -> Result<AstNode, CompileError> {
    // First check if the loop has a declaration or just an expression
    // If the first item is NOT a reference, then it is the item for the loop
    match x.current_token().to_owned() {
        Token::Variable(name, ..) => {
            // WHILE LOOP (existing variable found)
            if let Some(arg) = get_reference(&name, captured_declarations) {
                let mut data_type = arg.data_type.to_owned();
                let condition =
                    create_expression(x, &mut data_type, false, captured_declarations, &[])?;

                // Make sure this condition is a boolean expression
                return match data_type {
                    DataType::Bool(..) => {
                        // Make sure there is a colon after the condition
                        if x.current_token() != &Token::Colon {
                            return Err(CompileError {
                                msg: "A loop must have a colon after the condition".to_string(),
                                start_pos: x.token_start_position(),
                                end_pos: x.token_start_position(),
                                error_type: ErrorType::Syntax,
                            });
                        }

                        x.advance();

                        // create while loop
                        Ok(AstNode::WhileLoop(
                            condition,
                            new_ast(x, captured_declarations, returned_types)?,
                            x.token_start_position(),
                        ))
                    }

                    _ => Err(CompileError {
                        msg: format!(
                            "A loop condition using an existing variable must be a boolean expression (true or false). Found a {:?} expression",
                            data_type
                        ),
                        start_pos: x.token_start_position(),
                        end_pos: x.token_start_position(),
                        error_type: ErrorType::Type,
                    }),
                };
            }

            // FOR LOOP (new variable found)

            // TODO: might need to check for additional optional stuff like a type declaration or something here
            x.advance();

            // Make sure there is an 'in' keyword after the variable
            if x.current_token() != &Token::In {
                return Err(CompileError {
                    msg: "A loop must have an 'in' keyword after the variable".to_string(),
                    start_pos: x.token_start_position(),
                    end_pos: x.token_start_position(),
                    error_type: ErrorType::Syntax,
                });
            }

            x.advance();
            let mut iterable_type = DataType::Inferred(false);
            let iterated_item =
                create_expression(x, &mut iterable_type, false, captured_declarations, &[])?;

            // Make this type can be iterated over
            if !iterable_type.is_iterable() {
                return Err(CompileError {
                    msg: format!("The type {:?} is not iterable", iterable_type),
                    start_pos: x.token_start_position(),
                    end_pos: x.token_start_position(),
                    error_type: ErrorType::Type,
                });
            }

            // Make sure there is a colon
            if x.current_token() != &Token::Colon {
                return Err(CompileError {
                    msg: "A loop must have a colon after the condition".to_string(),
                    start_pos: x.token_start_position(),
                    end_pos: x.token_start_position(),
                    error_type: ErrorType::Syntax,
                });
            }

            x.advance();

            let loop_arg = Arg {
                name: name.to_owned(),
                data_type: iterable_type.get_iterable_type(),
                expr: Expr::None,
            };

            let mut combined = Vec::with_capacity(1 + captured_declarations.len());
            combined.push(loop_arg.to_owned());
            combined.extend_from_slice(captured_declarations);

            Ok(AstNode::ForLoop(
                Box::new(loop_arg),
                iterated_item,
                new_ast(x, &combined, returned_types)?,
                x.token_start_position(),
            ))
        }

        _ => Err(CompileError {
            msg: "A loop must have a variable declaration or an expression".to_string(),
            start_pos: x.token_start_position(),
            end_pos: x.token_start_position(),
            error_type: ErrorType::Syntax,
        }),
    }
}
