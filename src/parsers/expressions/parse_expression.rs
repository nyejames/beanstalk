use super::eval_expression::evaluate_expression;
use crate::parsers::ast_nodes::{NodeInfo, Value};
use crate::parsers::variables::create_new_var_or_ref;
use crate::{
    bs_types::DataType,
    parsers::{
        ast_nodes::{Arg, AstNode},
        create_scene_node::new_scene,
        tuples::new_tuple,
    },
    CompileError, Token,
};

// If the datatype is a collection
// The expression must only contain references to collections
// Or collection literals
pub fn create_expression(
    tokens: &Vec<Token>,
    i: &mut usize,
    inside_tuple: bool,
    ast: &Vec<AstNode>,
    mut data_type: &mut DataType,
    inside_brackets: bool,
    variable_declarations: &mut Vec<Arg>,
    token_line_numbers: &Vec<u32>,
) -> Result<Value, CompileError> {
    let mut expression = Vec::new();
    let mut number_union = DataType::Union(vec![DataType::Int, DataType::Float]);

    if inside_brackets {
        // Make sure there is an open parenthesis here (if not, return an error)
        // Only needed if create expression is called from new_ast/new_scene
        // Makes sure to enforce the parenthesis syntax
        // (otherwise would end up with some function call style stuff not needing parenthesis - this would look inconsistent)
        if let Some(Token::OpenParenthesis) = tokens.get(*i) {
            *i += 1;
        } else {
            return Err(CompileError {
                msg:
                    "Missing open parenthesis (function call arguments must be inside parenthesis)"
                        .to_string(),
                line_number: token_line_numbers[*i].to_owned(),
            });
        }

        match data_type {
            DataType::Tuple(inner_types) => {
                // HAS DEFINED INNER TYPES FOR THE TUPLE
                let tuple = new_tuple(
                    Value::None,
                    tokens,
                    &mut *i,
                    inner_types,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                return Ok(tuple_to_value(&tuple));
            }

            DataType::Inferred => {
                // DOES NOT HAVE DEFINED INNER TYPES FOR THE TUPLE
                let tuple = new_tuple(
                    Value::None,
                    tokens,
                    &mut *i,
                    &Vec::new(),
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                *data_type = DataType::Tuple(tuple.to_owned());
                return Ok(tuple_to_value(&tuple));
            }
            _ => {}
        }
    }

    // Loop through the expression and create the AST nodes
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
                        return Ok(Value::None);
                    }
                    break;
                } else {
                    if inside_tuple {
                        break;
                    }
                    *i += 1;
                    // Mismatched brackets, return an error
                    return Err(CompileError {
                        msg: "Mismatched brackets in expression".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
            }

            Token::OpenParenthesis => {
                return create_expression(
                    tokens,
                    &mut *i,
                    false,
                    ast,
                    &mut data_type,
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
                        expression,
                        token_line_numbers[*i].to_owned(),
                        &mut data_type,
                    )?;

                    let tuple = new_tuple(
                        eval_first_expr,
                        tokens,
                        i,
                        &Vec::new(),
                        ast,
                        variable_declarations,
                        token_line_numbers,
                    )?;

                    return Ok(Value::Tuple(tuple));
                }

                return Err(CompileError {
                    msg: "Comma found outside of tuple".to_string(),
                    line_number: token_line_numbers[*i].to_owned(),
                });
            }

            // Check if name is a reference to another variable or function call
            Token::Variable(name) => {
                let new_ref = create_new_var_or_ref(
                    name,
                    variable_declarations,
                    tokens,
                    &mut *i,
                    false,
                    ast,
                    token_line_numbers,
                    false,
                )?;

                match new_ref {
                    // Make sure this is a reference and not a new variable
                    AstNode::Literal(..) | AstNode::FunctionCall(..) => {
                        // Check type is correct
                        let reference_data_type = new_ref.get_type();
                        if !check_if_valid_type(&reference_data_type, &mut data_type) {
                            return Err(CompileError {
                                msg: format!(
                                    "Variable '{}' is of type {:?}, but used in an expression of type {:?}",
                                    name, reference_data_type, data_type
                                ),
                                line_number: token_line_numbers[*i].to_owned(),
                            });
                        }

                        expression.push(new_ref);
                    }

                    _ => {
                        return Err(CompileError {
                            msg: format!(
                                "Variable '{}' is not a valid reference - it's a: {:?}",
                                name,
                                new_ref.get_type()
                            ),
                            line_number: token_line_numbers[*i].to_owned(),
                        });
                    }
                }
            }

            // Check if is a literal
            Token::FloatLiteral(mut float) => {
                if !check_if_valid_type(&DataType::Float, &mut data_type) {
                    return Err(CompileError {
                        msg: format!("Float literal used in expression of type: {:?}", data_type),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }

                if next_number_negative {
                    float = -float;
                    next_number_negative = false;
                }

                expression.push(AstNode::Literal(
                    Value::Float(float),
                    token_line_numbers[*i],
                ));
            }

            Token::IntLiteral(int) => {
                if !check_if_valid_type(&DataType::Int, &mut data_type) {
                    return Err(CompileError {
                        msg: format!("Int literal used in expression of type: {:?}", data_type),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }

                if next_number_negative {
                    expression.push(AstNode::Literal(
                        Value::Int(-(*int)),
                        token_line_numbers[*i],
                    ));
                    next_number_negative = false;
                }

                expression.push(AstNode::Literal(Value::Int(*int), token_line_numbers[*i]));
            }

            Token::StringLiteral(string) => {
                if !check_if_valid_type(&DataType::String, &mut data_type) {
                    return Err(CompileError {
                        msg: format!("String literal used in expression of type: {:?}", data_type),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }

                expression.push(AstNode::Literal(
                    Value::String(string.clone()),
                    token_line_numbers[*i],
                ));
            }

            // Scenes - Create a new scene node
            // Maybe scenes can be added together like strings
            Token::SceneHead | Token::ParentScene => {
                if !check_if_valid_type(&DataType::Scene, &mut data_type) {
                    return Err(CompileError {
                        msg: format!("Scene literal used in expression of type: {:?}", data_type),
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
                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    1,
                    token_line_numbers[*i],
                ));
            }

            Token::Subtract => {
                if !check_if_valid_type(&data_type, &mut number_union) {
                    return Err(CompileError {
                        msg: format!(
                            "Subtraction can't be used in expression of type: {:?}",
                            data_type
                        ),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    1,
                    token_line_numbers[*i],
                ));
            }

            Token::Multiply => {
                if !check_if_valid_type(&number_union, &mut data_type) {
                    return Err(CompileError {
                        msg: format!(
                            "Multiplication can't be used in expression of type: {:?}",
                            data_type
                        ),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    2,
                    token_line_numbers[*i],
                ));
            }

            Token::Divide => {
                if !check_if_valid_type(&number_union, &mut data_type) {
                    return Err(CompileError {
                        msg: format!(
                            "Division can't be used in expression of type: {:?}",
                            data_type
                        ),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    2,
                    token_line_numbers[*i],
                ));
            }

            Token::Modulus => {
                if !check_if_valid_type(&number_union, &mut data_type) {
                    return Err(CompileError {
                        msg: format!(
                            "Modulus can't be used in expression of type: {:?}",
                            data_type
                        ),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    2,
                    token_line_numbers[*i],
                ));
            }

            // LOGICAL OPERATORS
            Token::Equal => {
                expression.push(AstNode::LogicalOperator(
                    Token::Equal,
                    5,
                    token_line_numbers[*i],
                ));
            }
            Token::LessThan => {
                expression.push(AstNode::LogicalOperator(
                    Token::LessThan,
                    5,
                    token_line_numbers[*i],
                ));
            }
            Token::LessThanOrEqual => {
                expression.push(AstNode::LogicalOperator(
                    Token::LessThanOrEqual,
                    5,
                    token_line_numbers[*i],
                ));
            }
            Token::GreaterThan => {
                expression.push(AstNode::LogicalOperator(
                    Token::GreaterThan,
                    5,
                    token_line_numbers[*i],
                ));
            }
            Token::GreaterThanOrEqual => {
                expression.push(AstNode::LogicalOperator(
                    Token::GreaterThanOrEqual,
                    5,
                    token_line_numbers[*i],
                ));
            }
            Token::And => {
                expression.push(AstNode::LogicalOperator(
                    Token::And,
                    4,
                    token_line_numbers[*i],
                ));
            }
            Token::Or => {
                expression.push(AstNode::LogicalOperator(
                    Token::Or,
                    3,
                    token_line_numbers[*i],
                ));
            }

            _ => {
                return Err(CompileError {
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

    evaluate_expression(expression, token_line_numbers[*i].to_owned(), data_type)
}

fn check_if_valid_type(data_type: &DataType, accepted_type: &mut DataType) -> bool {
    // Has to make sure if either type is a union, that the other type is also a member of the union

    match data_type {
        DataType::Union(ref types) => {
            for t in types {
                if check_if_valid_type(t, accepted_type) {
                    return true;
                }
            }
            return false;
        }
        _ => {}
    }

    match accepted_type {
        DataType::Inferred => {
            *accepted_type = data_type.to_owned();
            true
        }
        DataType::CoerceToString => true,
        DataType::Union(ref types) => {
            for t in types {
                if data_type == t {
                    return true;
                }
            }
            false
        }
        _ => {
            if data_type == accepted_type {
                true
            } else {
                false
            }
        }
    }
}

fn tuple_to_value(args: &Vec<Arg>) -> Value {
    // AUTOMATICALLY TURNS TUPLES OF ONE ITEM INTO THAT ITEM
    // This is a weird/unique design choice of the language

    // An empty tuple is None in this language
    if args.len() < 1 {
        return Value::None;
    }

    // Automatically convert tuples of one item into that item
    if args.len() == 1 {
        return args[0].value.to_owned();
    }

    Value::Tuple(args.to_owned())
}

pub fn get_accessed_arg(
    collection_name: &String,
    tokens: &Vec<Token>,
    i: &mut usize,
    possible_args: Vec<Arg>,
    token_line_numbers: &Vec<u32>,
) -> Result<Option<usize>, CompileError> {
    // Check if this is an access
    // Should be at the dot

    if let Some(Token::Dot) = tokens.get(*i) {
        // Move past the dot
        *i += 1;

        // Make sure an integer is next
        match tokens.get(*i) {
            Some(Token::IntLiteral(index)) => {
                // Check this is a valid index
                // Usize will flip to max number if negative
                // Maybe in future negative indexes with be supported (minus from the end)
                let idx: usize = *index as usize;
                if idx >= possible_args.len() {
                    return Err(CompileError {
                        msg: format!(
                            "Index {} out of range for any arguments in '{}'",
                            idx, collection_name
                        ),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }

                // Get the data type of this argument
                Ok(Some(idx))
            }

            Some(Token::Variable(name)) => {
                // Check if this is a named argument
                for (idx, arg) in possible_args.iter().enumerate() {
                    if arg.name == *name {
                        return Ok(Some(idx));
                    }
                }

                Err(CompileError {
                    msg: format!("Name '{}' not found in tuple '{}'", name, collection_name),
                    line_number: token_line_numbers[*i].to_owned(),
                })
            }

            _ => Err(CompileError {
                msg: format!(
                    "Expected an index or name to access tuple '{}'",
                    collection_name
                ),
                line_number: token_line_numbers[*i].to_owned(),
            }),
        }
    } else {
        Ok(None)
    }
}
