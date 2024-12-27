use super::{
    ast_nodes::{Arg, AstNode},
    collections::new_collection,
    expressions::parse_expression::{create_expression, get_accessed_arg},
    functions::create_function,
    tuples::new_tuple,
};
use crate::parsers::ast_nodes::{NodeInfo, Value};
use crate::parsers::functions::create_func_call_args;
use crate::{bs_types::DataType, CompileError, Token};

pub fn create_new_var_or_ref(
    name: &String,
    variable_declarations: &mut Vec<Arg>,
    tokens: &Vec<Token>,
    i: &mut usize,
    is_exported: bool,
    ast: &Vec<AstNode>,
    token_line_numbers: &Vec<u32>,
    inside_tuple: bool,
) -> Result<AstNode, CompileError> {
    let is_const = name.to_uppercase() == *name;

    // If this is a reference to a function or variable
    if let Some(arg) = variable_declarations.iter().find(|a| &a.name == name) {
        match arg.data_type {
            // Function Call
            DataType::Function(ref argument_refs, ref return_args) => {
                // Parse arguments passed into the function
                let tuple = new_tuple(
                    Value::None,
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
                let args = create_func_call_args(&tuple, &argument_refs, &line_number)?;
                // look for which arguments are being accessed from the function call
                // If nothing is being accessed, just pass the arguments in
                let accessed_arg = get_accessed_arg(
                    &arg.name,
                    tokens,
                    &mut *i,
                    return_args.to_owned(),
                    token_line_numbers,
                )?;

                return Ok(AstNode::FunctionCall(
                    name.to_owned(),
                    args,
                    return_args.to_owned(),
                    accessed_arg,
                    token_line_numbers[*i],
                ));
            }

            DataType::Tuple(ref inner_types) => {
                // Move past variable name
                *i += 1;

                let accessed_arg = get_accessed_arg(
                    &arg.name,
                    tokens,
                    &mut *i,
                    inner_types.to_owned(),
                    token_line_numbers,
                )?;

                return Ok(AstNode::Literal(
                    Value::Reference(arg.name.to_owned(), arg.data_type.to_owned(), accessed_arg),
                    token_line_numbers[*i].to_owned(),
                ));
            }

            DataType::Collection(_) => {
                // Check if this is a collection access
                if let Some(Token::Dot) = tokens.get(*i + 1) {
                    // Move past the dot
                    *i += 2;

                    // Make sure an integer is next
                    return if let Some(Token::IntLiteral(index)) = tokens.get(*i) {
                        Ok(AstNode::Literal(
                            Value::Reference(
                                arg.name.to_owned(),
                                arg.data_type.to_owned(),
                                Some(*index as usize),
                            ),
                            token_line_numbers[*i].to_owned(),
                        ))
                    } else {
                        Err(CompileError {
                            msg: format!(
                                "Expected an integer index to access collection '{}'",
                                arg.name
                            ),
                            line_number: token_line_numbers[*i].to_owned(),
                        })
                    };
                }
            }
            _ => {}
        }

        return Ok(AstNode::Literal(
            Value::Reference(arg.name.to_owned(), arg.data_type.to_owned(), None),
            token_line_numbers[*i],
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
        inside_tuple,
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
    inside_tuple: bool,
) -> Result<AstNode, CompileError> {
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
                value: Value::None,
            });

            return Ok(function);
        }

        // Has a type declaration
        &Token::TypeKeyword(ref type_keyword) => {
            data_type = type_keyword.to_owned();
            *i += 1;

            match &tokens[*i] {
                &Token::Assign => {}

                // If this is the end of the assignment, it is an uninitialized variable
                // Currently just creates a zero value variable, should be uninitialised in future
                &Token::Newline | &Token::EOF => {
                    variable_declarations.push(Arg {
                        name: name.to_owned(),
                        data_type: data_type.to_owned(),
                        value: Value::None,
                    });

                    return Ok(create_zero_value_var(
                        data_type.to_owned(),
                        name.to_string(),
                        is_exported,
                        token_line_numbers[*i],
                    ));
                }
                _ => {
                    return Err(CompileError {
                        msg: format!(
                            "Variable of type: {:?} does not exist in this scope",
                            data_type
                        ),
                        line_number: token_line_numbers[*i],
                    });
                }
            }
        }

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
            // Dynamic Collection literal
            let collection = new_collection(
                tokens,
                i,
                ast,
                token_line_numbers,
                &mut data_type,
                variable_declarations,
            )?;

            return Ok(AstNode::VarDeclaration(
                name.to_string(),
                collection,
                is_exported,
                data_type.to_owned(),
                false,
                token_line_numbers[*i],
            ));
        }

        _ => {
            parsed_expr = create_expression(
                tokens,
                i,
                inside_tuple,
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
    let new_var = create_var_node(
        is_const,
        name.to_string(),
        parsed_expr.get_value(),
        is_exported,
        data_type,
        variable_declarations,
        token_line_numbers[*i],
    );

    Ok(new_var)
}

fn create_var_node(
    is_const: bool,
    var_name: String,
    var_value: Value,
    is_exported: bool,
    data_type: DataType,
    variable_declarations: &mut Vec<Arg>,
    line_number: u32,
) -> AstNode {
    variable_declarations.push(Arg {
        name: var_name.to_owned(),
        data_type: data_type.to_owned(),
        value: var_value.to_owned(),
    });

    if is_const {
        return AstNode::VarDeclaration(
            var_name,
            var_value.to_owned(),
            is_exported,
            data_type,
            true,
            line_number,
        );
    }

    AstNode::VarDeclaration(
        var_name,
        var_value.to_owned(),
        is_exported,
        data_type,
        false,
        line_number,
    )
}

fn create_zero_value_var(
    data_type: DataType,
    name: String,
    is_exported: bool,
    line_number: u32,
) -> AstNode {
    match data_type {
        DataType::Float => AstNode::VarDeclaration(
            name,
            Value::Float(0.0),
            is_exported,
            data_type,
            false,
            line_number,
        ),

        DataType::Int => AstNode::VarDeclaration(
            name,
            Value::Int(0),
            is_exported,
            data_type,
            false,
            line_number,
        ),

        DataType::String => AstNode::VarDeclaration(
            name,
            Value::String("".to_string()),
            is_exported,
            data_type,
            false,
            line_number,
        ),

        DataType::Bool => AstNode::VarDeclaration(
            name,
            Value::Bool(false),
            is_exported,
            data_type,
            false,
            line_number,
        ),

        _ => AstNode::VarDeclaration(
            name,
            Value::None,
            is_exported,
            data_type,
            false,
            line_number,
        ),
    }
}
