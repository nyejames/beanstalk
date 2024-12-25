use colour::red_ln;

use crate::{
    bs_types::DataType, parsers::{
        ast_nodes::{AstNode, Arg},
        create_scene_node::new_scene,
        tuples::new_tuple,
    }, CompileError, Token
};
use crate::parsers::ast_nodes::Node;
use crate::parsers::functions::create_func_call_args;
use crate::parsers::tuples::create_node_from_tuple;
use super::eval_expression::evaluate_expression;

// If the datatype is a collection
// The expression must only contain references to collections
// Or collection literals
pub fn create_expression(
    tokens: &Vec<Token>,
    i: &mut usize,
    inside_tuple: bool,
    ast: &Vec<AstNode>,
    data_type: &mut DataType,
    inside_brackets: bool,
    variable_declarations: &mut Vec<Arg>,
    token_line_numbers: &Vec<u32>,
) -> Result<AstNode, CompileError> {
    let mut expression = Vec::new();
    let number_union = DataType::Union(vec![DataType::Int, DataType::Float]);

    if inside_brackets {
        *i += 1;
        match data_type {
            DataType::Tuple(inner_types) => {
                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    inner_types,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                return create_node_from_tuple(tuple, token_line_numbers[*i])
            },
            DataType::Inferred => {
                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &Vec::new(),
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                return create_node_from_tuple(tuple, token_line_numbers[*i])
            }
            _ => {},
        }
    }

    // Loop through the expression and create the AST nodes (increment i each time)
    // Figure out the type it should be from the data
    // DOES NOT MOVE TOKENS PAST THE CLOSING TOKEN
    let mut next_number_negative = false;
    while let Some(token) = tokens.get(*i) {
        match token {
            // Conditions that close the expression
            Token::CloseParenthesis => {
                if inside_brackets {
                    *i += 1;
                    if expression.is_empty() {
                        return Ok(AstNode::Empty(token_line_numbers[*i].to_owned()));
                    }
                    break;
                } else {
                    if inside_tuple {
                        break;
                    }
                    *i += 1;
                    // Mismatched brackets, return an error
                    return Err( CompileError {
                        msg: "Mismatched brackets in expression".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
            }

            Token::OpenParenthesis => {
                return create_expression(
                    tokens,
                    i,
                    false,
                    ast,
                    data_type,
                    true,
                    variable_declarations,
                    token_line_numbers,
                );
            }

            Token::EOF | Token::SceneClose(_) | Token::Arrow | Token::Colon | Token::End => {
                if inside_brackets {
                    return Err( CompileError {
                        msg: "Not enough closing parenthesis for expression. Need more ')' at the end of the expression!".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
                break;
            }

            Token::Newline => {
                // Fine if inside of brackets (not closed yet)
                // Otherwise break out of the expression
                if inside_brackets {
                    *i += 1;
                    continue;
                } else {
                    break;
                }
            }

            Token::Comma => {
                if inside_tuple {
                    break;
                }
                *i += 1;

                if inside_brackets {
                    let eval_first_expr = evaluate_expression(
                        AstNode::Expression(expression, token_line_numbers[*i].to_owned()),
                        data_type,
                        ast,
                    )?;

                    let tuple = new_tuple(
                        Some(eval_first_expr),
                        tokens,
                        i,
                        &Vec::new(),
                        ast,
                        variable_declarations,
                        token_line_numbers,
                    )?;

                    return create_node_from_tuple(tuple, token_line_numbers[*i])
                }

                return Err( CompileError {
                    msg: "Comma found outside of tuple".to_string(),
                    line_number: token_line_numbers[*i].to_owned(),
                });
            }

            // Check if name is a reference to another variable or function call
            Token::Variable(name) => {
                match variable_declarations.iter().find(|a| a.name == *name) {
                    Some(arg) => {
                        // If this expression is inferring its type from the expression
                        if *data_type == DataType::Inferred {
                            *data_type = arg.data_type.to_owned();
                        }

                        // Check if this is a tuple/type/collection that is being accessed by a dot
                        match &arg.data_type {
                            DataType::Tuple(inner_types) => {
                                // Check if this is a tuple access
                                if let Some(Token::Dot) = tokens.get(*i + 1) {
                                    // Move past the dot
                                    *i += 2;

                                    // Make sure an integer is next
                                    if let Some(Token::IntLiteral(index)) = tokens.get(*i) {
                                        // Check this is a valid index
                                        // Usize will flip to max number if negative
                                        // Maybe in future negative indexes with be supported (minus from the end)
                                        let idx: usize = *index as usize;
                                        if idx >= inner_types.len() {
                                            return Err( CompileError {
                                                msg: format!(
                                                    "Index {} out of range for tuple '{}'",
                                                    idx, arg.name
                                                ),
                                                line_number: token_line_numbers[*i].to_owned(),
                                            });
                                        }
                                        // Check the accessed item in the tuple is the same type as the expression
                                        // Or let it through if this expression is being coerced to a string
                                        let tuple_item_type = create_node_from_tuple(
                                            inner_types.to_owned(),
                                            token_line_numbers[*i]
                                                .to_owned())?
                                            .get_type();

                                        if !check_if_valid_type(&tuple_item_type, data_type) {
                                            return Err( CompileError {
                                                msg: format!(
                                                    "Tuple '{}' is of type {:?}, but used in an expression of type {:?}",
                                                    arg.name, arg.data_type, data_type
                                                ),
                                                line_number: token_line_numbers[*i].to_owned(),
                                            });
                                        }
                                        expression.push(AstNode::TupleAccess(
                                            arg.name.to_owned(),
                                            *index as usize,
                                            tuple_item_type.to_owned(),
                                            token_line_numbers[*i].to_owned(),
                                        ));

                                        *i += 1;
                                        continue;
                                    } else {
                                        return Err( CompileError {
                                            msg: format!(
                                                "Expected an integer index to access tuple '{}'",
                                                arg.name
                                            ),
                                            line_number: token_line_numbers[*i].to_owned(),
                                        });
                                    }
                                }
                            }

                            DataType::Collection(inner_types) => {
                                // Check if this is a collection access
                                if let Some(Token::Dot) = tokens.get(*i + 1) {
                                    // Make sure the type of the collection is the same as the type of the expression
                                    if !check_if_valid_type(&inner_types, data_type) {
                                        return Err( CompileError {
                                            msg: format!(
                                                "Collection '{}' is of type {:?}, but used in an expression of type {:?}",
                                                arg.name, arg.data_type, data_type
                                            ),
                                           line_number: token_line_numbers[*i].to_owned(),
                                        });
                                    }

                                    // Move past the dot
                                    *i += 2;

                                    // Make sure an integer is next
                                    if let Some(Token::IntLiteral(index)) = tokens.get(*i) {
                                        expression.push(AstNode::CollectionAccess(
                                            arg.name.to_owned(),
                                            *index as usize,
                                            *inner_types.to_owned(),
                                            token_line_numbers[*i].to_owned(),
                                        ));
                                        *i += 1;
                                        continue;
                                    } else {
                                        return Err( CompileError {
                                            msg: format!(
                                                "Expected an integer index to access collection '{}'",
                                                arg.name
                                            ),
                                            line_number: token_line_numbers[*i].to_owned(),
                                        });
                                    }
                                }
                            }

                            DataType::Function(arguments, return_type) => {
                                // FUNCTION CALLS
                                // move past the variable name
                                let function_type = create_node_from_tuple(
                                    arguments.to_owned(),
                                    token_line_numbers[*i].to_owned())?
                                    .get_type();

                                *i += 1;
                                match tokens[*i] {
                                    Token::OpenParenthesis => {
                                        *i += 1;
                                        if !check_if_valid_type(&function_type, data_type) {
                                            return Err( CompileError {
                                                msg: format!(
                                                    "Function '{}' returns type {:?}, but used in an expression of type {:?}",
                                                    arg.name, return_type, data_type
                                                ),
                                                line_number: token_line_numbers[*i].to_owned(),
                                            });
                                        }

                                        let tuple = new_tuple(
                                            None,
                                            tokens,
                                            i,
                                            arguments,
                                            ast,
                                            &mut variable_declarations.to_owned(),
                                            token_line_numbers,
                                        )?;

                                        let line_number = token_line_numbers[*i];
                                        let args = create_func_call_args(
                                            &create_node_from_tuple(tuple.to_owned(), line_number)?,
                                            &tuple,
                                            &line_number,
                                        )?;
                                        
                                        expression.push(AstNode::FunctionCall(
                                            arg.name.to_owned(),
                                            args,
                                            return_type.clone(),
                                            token_line_numbers[*i].to_owned(),
                                        ));

                                        *i += 1;
                                        continue;
                                    }

                                    // Just a reference to a function
                                    _ => {
                                        if !check_if_valid_type(&arg.data_type, data_type) {
                                            return Err( CompileError {
                                                msg: format!(
                                                    "Function {} literal used in expression of type {:?}",
                                                    arg.name, data_type
                                                ),
                                                line_number: token_line_numbers[*i].to_owned(),
                                            });
                                        }
                                    }
                                };
                            }
                            _ => {}
                        }

                        // If the variables type is known and not the same as the type of the expression
                        // Return a type error
                        if !check_if_valid_type(&arg.data_type, data_type) {
                            return Err( CompileError {
                                msg: format!(
                                    "Variable {} is of type {:?}, but used in an expression of type {:?}",
                                    arg.name, arg.data_type, data_type
                                ),
                                line_number: token_line_numbers[*i].to_owned(),
                            });
                        }

                        if arg.name.to_uppercase() == arg.name {
                            expression.push(AstNode::ConstReference(
                                arg.name.to_owned(),
                                arg.data_type.to_owned(),
                                token_line_numbers[*i].to_owned(),
                            ));
                        } else {
                            expression.push(AstNode::VarReference(
                                arg.name.to_owned(),
                                arg.data_type.to_owned(),
                                token_line_numbers[*i].to_owned(),
                            ));
                        };
                    }
                    None => {
                        return Err( CompileError {
                            msg: format!("Variable {} not found in scope", name),
                            line_number: token_line_numbers[*i].to_owned(),
                        });
                    }
                }
            }

            // Check if is a literal
            Token::FloatLiteral(mut float) => {
                if !check_if_valid_type(&DataType::Float, data_type) {
                    return Err( CompileError {
                        msg: "Float literal used in non-float expression".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
                if next_number_negative {
                    float = -float;
                    next_number_negative = false;
                }
                expression.push(AstNode::Literal(Token::FloatLiteral(float), token_line_numbers[*i]));
            }
            Token::IntLiteral(int) => {
                if !check_if_valid_type(&DataType::Int, data_type) {
                    return Err(CompileError {
                        msg: "Int literal used in non-integer expression".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
                if next_number_negative {
                    expression.push(AstNode::Literal(Token::IntLiteral(-(*int)), token_line_numbers[*i]));
                    next_number_negative = false;
                } else {
                    expression.push(AstNode::Literal(Token::IntLiteral(*int), token_line_numbers[*i]));
                }
            }
            Token::StringLiteral(string) => {
                if !check_if_valid_type(&DataType::String, data_type) {
                    return Err(CompileError {
                        msg: "String literal used in non-string expression".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
                expression.push(AstNode::Literal(Token::StringLiteral(string.clone()), token_line_numbers[*i]));
            }

            // Scenes - Create a new scene node
            // Maybe scenes can be added together like strings
            Token::SceneHead | Token::ParentScene => {
                if !check_if_valid_type(&DataType::Scene, data_type) {
                    return Err(CompileError {
                        msg: "Scene used in non-scene expression".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
                return new_scene(tokens, i, &ast, token_line_numbers, variable_declarations);
            }

            // OPERATORS
            // Will push as a string so shunting yard can handle it later just as a string
            Token::Negative => {
                next_number_negative = true;
            }

            // BINARY OPERATORS
            Token::Add => {
                expression.push(AstNode::BinaryOperator(token.to_owned(), 1, token_line_numbers[*i]));
            }
            Token::Subtract => {
                if !check_if_valid_type(&number_union, data_type) {
                    return Err(CompileError {
                        msg: "Subtraction used in non-numerical expression".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
                expression.push(AstNode::BinaryOperator(token.to_owned(), 1, token_line_numbers[*i]));
            }
            Token::Multiply => {
                if !check_if_valid_type(&number_union,  data_type) {
                    return Err(CompileError {
                        msg: "Multiplication used in non-numerical expression".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
                expression.push(AstNode::BinaryOperator(token.to_owned(), 2, token_line_numbers[*i]));
            }
            Token::Divide => {
                if !check_if_valid_type(&number_union, data_type) {
                    return Err(CompileError {
                        msg: "Division used in non-numerical expression".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
                expression.push(AstNode::BinaryOperator(token.to_owned(), 2, token_line_numbers[*i]));
            }
            Token::Modulus => {
                if !check_if_valid_type(&number_union,  data_type) {
                    return Err(CompileError {
                        msg: "Modulus used in non-numerical expression".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
                expression.push(AstNode::BinaryOperator(token.to_owned(), 2, token_line_numbers[*i]));
            }

            // LOGICAL OPERATORS
            Token::Equal => {
                expression.push(AstNode::LogicalOperator(Token::Equal, 5, token_line_numbers[*i]));
            }
            Token::LessThan => {
                expression.push(AstNode::LogicalOperator(Token::LessThan, 5, token_line_numbers[*i]));
            }
            Token::LessThanOrEqual => {
                expression.push(AstNode::LogicalOperator(Token::LessThanOrEqual, 5, token_line_numbers[*i]));
            }
            Token::GreaterThan => {
                expression.push(AstNode::LogicalOperator(Token::GreaterThan, 5, token_line_numbers[*i]));
            }
            Token::GreaterThanOrEqual => {
                expression.push(AstNode::LogicalOperator(Token::GreaterThanOrEqual, 5, token_line_numbers[*i]));
            }
            Token::And => {
                expression.push(AstNode::LogicalOperator(Token::And, 4, token_line_numbers[*i]));
            }
            Token::Or => {
                expression.push(AstNode::LogicalOperator(Token::Or, 3, token_line_numbers[*i]));
            }

            _ => {
                return Err( CompileError {
                    msg: format!(
                        "Invalid Expression: {:?}, must be assigned with a valid datatype",
                        token
                    ),
                    line_number: token_line_numbers[*i].to_owned(),
                });
            }
        }

        *i += 1;
    }

     evaluate_expression(
        AstNode::Expression(expression, token_line_numbers[*i].to_owned()),
        data_type,
        ast,
    )
}

// RETURNING NONE MEANS NOT A FUNCTION CALL -> JUST A REFERENCE
/*pub fn get_args(
    tokens: &Vec<Token>,
    i: &mut usize,
    ast: &Vec<AstNode>,
    token_line_numbers: &Vec<u32>,
    variable_declarations: &mut Vec<Reference>,
    argument_refs: &Vec<Reference>,
) -> Option<AstNode> {
    if *i >= tokens.len() {
        return None;
    }

    // TO DO: Check the argument refs, if there are multiple, pass in a tuple of the correct types
    let mut data_type = if argument_refs.len() > 1 {
        // Create tuple of the argument types
        DataType::Tuple(
            argument_refs
                .iter()
                .map(|arg| arg.data_type.to_owned())
                .collect(),
        )
    } else if argument_refs.len() == 1 {
        argument_refs[0].data_type.to_owned()
    } else {
        DataType::None
    };

    // Check if the current token is an open bracket
    // This can be passed an empty tuple
    // So hopefully there will be a type error,
    // if more than 0 arguments are passed in the case of a function call with 0 args
    // Will probably be faster to check specifically for the empty tuple case before parsing in the future.
    match &tokens[*i] {
        // Check if open bracket
        Token::OpenParenthesis => match create_expression(
            tokens,
            &mut *i,
            false,
            ast,
            &mut data_type,
            true,
            variable_declarations,
            token_line_numbers,
        ) {
            Ok(node) => Some(node),
            Err(e) => return Err(CompileError {
                msg: format!("Error parsing expression: {:?}", e),
                line_number: token_line_numbers[*i].to_owned(),
            }),
        },
        _ => None,
    }
}*/

fn check_if_valid_type(data_type: &DataType, accepted_type: &mut DataType) -> bool {
    match accepted_type {
        DataType::Inferred => {
            *accepted_type = data_type.to_owned();
            true
        }
        DataType::CoerseToString => true,
        DataType::Union(types) => {
            for t in &**types {
                if data_type == t {
                    return true;
                }
            }
            red_ln!(
                "Type Error: Expected type {:?}, but got type {:?}",
                accepted_type,
                data_type
            );
            false
        }
        _ => {
            if data_type == accepted_type {
                true
            } else {
                red_ln!(
                    "Type Error: Expected type {:?}, but got type {:?}",
                    accepted_type,
                    data_type
                );
                false
            }
        }
    }
}
