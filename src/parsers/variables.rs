use crate::{bs_types::DataType, CompileError, Token};
use crate::parsers::functions::create_func_call_args;
use crate::parsers::tuples::create_node_from_tuple;
use super::{
    ast_nodes::{AstNode, Arg},
    collections::new_collection,
    expressions::parse_expression::create_expression,
    functions::create_function, tuples::new_tuple,
};

pub fn create_new_var_or_ref(
    name: &String,
    variable_declarations: &mut Vec<Arg>,
    tokens: &Vec<Token>,
    i: &mut usize,
    is_exported: bool,
    ast: &Vec<AstNode>,
    token_line_numbers: &Vec<u32>,
) -> Result<(AstNode, DataType), CompileError> {
    let is_const = name.to_uppercase() == *name;

    // If this is a reference to a function or variable
    if let Some(arg) = variable_declarations.iter().find(|a| &a.name == name) {
        match arg.data_type {
            DataType::Function(ref argument_refs, ref return_type) => {

                // Parse arguments passed into the function
                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    argument_refs,
                    ast,
                    &mut variable_declarations.to_owned(),
                    token_line_numbers,
                )?;

                let line_number = token_line_numbers[*i];

                // Create the args for the function call
                // This makes sure the args are in the correct order
                let args = create_func_call_args(
                    &create_node_from_tuple(tuple.to_owned(), line_number)?,
                    &tuple,
                    &line_number,
                )?;

                return Ok((
                    AstNode::FunctionCall(
                        name.to_owned(),
                        args,
                        return_type.to_owned(),
                        token_line_numbers[*i],
                    ),
                    arg.data_type.to_owned(),
                ));
            }
            _ => {}
        }

        if is_const {
            return Ok((
                AstNode::ConstReference(arg.name.to_owned(), arg.data_type.to_owned(), token_line_numbers[*i]),
                arg.data_type.to_owned(),
            ));
        }

        return Ok((
            AstNode::VarReference(arg.name.to_owned(), arg.data_type.to_owned(), token_line_numbers[*i]),
            arg.data_type.to_owned(),
        ));
    }

    new_variable(
        name,
        tokens,
        i,
        is_exported,
        ast,
        token_line_numbers,
        &mut *variable_declarations,
        is_const,
    )
}

pub fn new_variable(
    name: &String,
    tokens: &Vec<Token>,
    i: &mut usize,
    is_exported: bool,
    ast: &Vec<AstNode>,
    token_line_numbers: &Vec<u32>,
    variable_declarations: &mut Vec<Arg>,
    is_const: bool,
) -> Result<(AstNode, DataType), CompileError> {
    *i += 1;
    let mut data_type = DataType::Inferred;

    // TODO - make sure that there is a type or a default value
    // Should be an error if neither is provided to initialise a variable?
    
    match &tokens[*i] {
        // Type is inferred
        &Token::Assign => {}

        &Token::FunctionKeyword => {
            *i += 1;
            let (function, arg_refs, return_type) = create_function(
                name.to_owned(),
                tokens,
                i,
                is_exported,
                ast,
                token_line_numbers,
                variable_declarations,
            )?;

            variable_declarations.push(Arg {
                name: name.to_owned(),
                data_type: DataType::Function(arg_refs.clone(), return_type.to_owned()),
                value: AstNode::Empty(token_line_numbers[*i]),
            });
            return Ok((function, DataType::Function(arg_refs, return_type.to_owned())));
        }

        // Has a type declaration
        &Token::TypeKeyword(ref type_keyword) => {
            data_type = type_keyword.to_owned();
            *i += 1;

            match &tokens[*i] {
                &Token::Assign => {}

                // If this is the end of the assignment, it is an uninitalised variable
                // Currently just creates a zero value variable, should be uninitialised in future
                &Token::Newline | &Token::EOF => {
                    variable_declarations.push(Arg {
                        name: name.to_owned(),
                        data_type: data_type.to_owned(),
                        value: AstNode::Empty(token_line_numbers[*i]),
                    });

                    return Ok((
                        create_zero_value_var(
                            data_type.to_owned(),
                            name.to_string(),
                            is_exported,
                            token_line_numbers[*i],
                        ),
                        data_type,
                    ));
                }
                _ => {
                    return Err(CompileError {
                        msg: format!(
                            "Variable of type: {:?} does not exsist in this scope",
                            data_type
                        ),
                        line_number: token_line_numbers[*i],
                    });
                }
            }
        }

        // TO DO: Multiple assignments
        // &Token::Comma => {
        // }

        // Anything else is a syntax error
        _ => {
            return Err(CompileError {
                msg: format!(
                    "'{}' - Invalid variable declaration: {:?}",
                    name, tokens[*i]
                ),
                line_number: token_line_numbers[*i],
            });
        }
    };

    // Current token (SHOULD BE) the assignment operator
    // Move past assignment to get assigned values
    *i += 1;

    let parsed_expr;
    match &tokens[*i] {
        // Check if this is a COLLECTION
        Token::OpenCurly => {
            if is_const {
                // Make a read only collection
            }

            // Dynamic Collection literal
            let collection = new_collection(tokens, i, ast, token_line_numbers, &mut data_type, variable_declarations)?;
            return match collection {
                AstNode::Collection(..) => {
                    variable_declarations.push(Arg {
                        name: name.to_owned(),
                        data_type: data_type.to_owned(),
                        value: AstNode::Empty(token_line_numbers[*i]),
                    });
                    Ok((AstNode::VarDeclaration(
                        name.to_string(),
                        Box::new(collection),
                        is_exported,
                        data_type.to_owned(),
                        false,
                        token_line_numbers[*i],
                    ),
                        data_type
                    ))
                }
                _ => {
                    Err(CompileError {
                        msg: "Invalid collection".to_string(),
                        line_number: token_line_numbers[*i],
                    })
                }
            }
        }

        _ => {
            parsed_expr = create_expression(
                tokens,
                i,
                false,
                &ast,
                &mut data_type,
                false,
                variable_declarations,
                token_line_numbers,
            )?;
        }
    }

    // Check if a type of collection / tuple has been created
    // Or whether it is a literal or expression
    // If the expression is an empty expression when the variable is NOT a function, return an error
    match parsed_expr {
        AstNode::RuntimeExpression(_, ref evaluated_type, _) => {
            Ok((create_var_node(
                is_const,
                name.to_string(),
                parsed_expr.to_owned(),
                is_exported,
                evaluated_type.to_owned(),
                variable_declarations,
                token_line_numbers[*i],
            ), evaluated_type.to_owned()))
        }
        AstNode::Literal(ref token, _) => {
            let data_type = match token {
                Token::FloatLiteral(_) => DataType::Float,
                Token::IntLiteral(_) => DataType::Int,
                Token::StringLiteral(_) => DataType::String,
                Token::BoolLiteral(_) => DataType::Bool,
                _ => DataType::Inferred,
            };
            Ok((create_var_node(
                is_const,
                name.to_string(),
                parsed_expr,
                is_exported,
                data_type.to_owned(),
                variable_declarations,
                token_line_numbers[*i],
            ), data_type))
        }
        AstNode::Tuple(..) => {
            Ok((create_var_node(
                is_const,
                name.to_string(),
                parsed_expr,
                is_exported,
                data_type.to_owned(),
                variable_declarations,
                token_line_numbers[*i],
            ), data_type))
        }
        AstNode::Scene(..) => {
            Ok((create_var_node(
                is_const,
                name.to_string(),
                parsed_expr,
                is_exported,
                DataType::Scene,
                variable_declarations,
                token_line_numbers[*i],
            ), DataType::Scene))
        }
        AstNode::Error(err, line) => {
            Err(CompileError {
                msg: format!(
                    "Error: Invalid expression for variable assignment (creating new variable: {name}) at line {}: {}",
                    line,
                    err
                ),
                line_number: line,
            })
        }

        _ => {
            Err(CompileError {
                msg: format!("Invalid expression for variable assignment (creating new variable: {name}). Value was: {:?}", parsed_expr),
                line_number: token_line_numbers[*i - 1],
            })
        }
    }
}

fn create_var_node(
    is_const: bool,
    var_name: String,
    var_value: AstNode,
    is_exported: bool,
    data_type: DataType,
    variable_declarations: &mut Vec<Arg>,
    line_number: u32,
) -> AstNode {
    variable_declarations.push(Arg {
        name: var_name.to_owned(),
        data_type: data_type.to_owned(),
        value: AstNode::Empty(line_number),
    });

    if is_const {
        return AstNode::VarDeclaration(
            var_name,
            Box::new(var_value),
            is_exported,
            data_type,
            true,
            line_number,
        );
    }

    AstNode::VarDeclaration(var_name, Box::new(var_value), is_exported, data_type, false, line_number)
}

fn create_zero_value_var(data_type: DataType, name: String, is_exported: bool, line_number: u32) -> AstNode {
    match data_type {
        DataType::Float => AstNode::VarDeclaration(
            name,
            Box::new(AstNode::Literal(Token::FloatLiteral(0.0), line_number)),
            is_exported,
            data_type,
            false,
            line_number,
        ),
        DataType::Int => AstNode::VarDeclaration(
            name,
            Box::new(AstNode::Literal(Token::IntLiteral(0), line_number)),
            is_exported,
            data_type,
            false,
            line_number,
        ),
        DataType::String => AstNode::VarDeclaration(
            name,
            Box::new(AstNode::Literal(Token::StringLiteral("".to_string()), line_number)),
            is_exported,
            data_type,
            false,
            line_number,
        ),
        DataType::Bool => AstNode::VarDeclaration(
            name,
            Box::new(AstNode::Literal(Token::BoolLiteral(false), line_number)),
            is_exported,
            data_type,
            false,
            line_number,
        ),
        _ => AstNode::VarDeclaration(
            name,
            Box::new(AstNode::Empty(line_number)),
            is_exported,
            data_type,
            false,
            line_number,
        ),
    }
}
