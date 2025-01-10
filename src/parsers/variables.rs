use super::{
    ast_nodes::{Arg, AstNode},
    expressions::parse_expression::create_expression,
    functions::create_function,
};
use crate::parsers::ast_nodes::{NodeInfo, Value};
use crate::parsers::expressions::parse_expression::get_accessed_args;
use crate::parsers::functions::parse_function_call;
use crate::{bs_types::DataType, CompileError, Token};
use colour::red_ln;

pub fn create_new_var_or_ref(
    name: &String,
    variable_declarations: &mut Vec<Arg>,
    tokens: &Vec<Token>,
    i: &mut usize,
    is_exported: bool,
    ast: &Vec<AstNode>,
    token_line_numbers: &Vec<u32>,
    inside_structure: bool, // This allows parse_expression to know that new variable declarations are valid
) -> Result<AstNode, CompileError> {
    let is_const = &name.to_uppercase() == name;

    // If this is a reference to a function or variable
    // This to_owned here is gross, probably a better way to avoid this
    if let Some(arg) = variable_declarations
        .to_owned()
        .iter()
        .find(|a| &a.name == name)
    {
        return match arg.data_type {
            // Function Call
            DataType::Function(ref argument_refs, ref return_args) => parse_function_call(
                name,
                tokens,
                i,
                ast,
                token_line_numbers,
                variable_declarations,
                argument_refs,
                return_args,
            ),

            DataType::Structure(..) | DataType::Collection(..) => {
                let accessed_arg = get_accessed_args(
                    &arg.name,
                    tokens,
                    &mut *i,
                    &arg.data_type,
                    token_line_numbers,
                    &mut Vec::new(),
                )?;

                Ok(AstNode::Literal(
                    Value::Reference(arg.name.to_owned(), arg.data_type.to_owned(), accessed_arg),
                    token_line_numbers[*i].to_owned(),
                ))
            }

            _ => {
                // Check to make sure there is no access attempt on any other types
                // Get accessed arg will return an error if there is an access attempt on the wrong type
                // This SHOULD always be None (for now) but this is being assigned to the reference here
                // incase the language changes in the future and properties/methods are added to other types
                let accessed_arg = get_accessed_args(
                    &arg.name,
                    tokens,
                    &mut *i,
                    &arg.data_type,
                    token_line_numbers,
                    &mut Vec::new(),
                )?;

                Ok(AstNode::Literal(
                    Value::Reference(arg.name.to_owned(), arg.data_type.to_owned(), accessed_arg),
                    token_line_numbers[*i],
                ))
            }
        };
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
        inside_structure,
    )
}

fn new_variable(
    name: &String,
    tokens: &Vec<Token>,
    i: &mut usize,
    is_exported: bool,
    ast: &Vec<AstNode>,
    token_line_numbers: &Vec<u32>,
    variable_declarations: &mut Vec<Arg>,
    is_const: bool,
    inside_structure: bool,
) -> Result<AstNode, CompileError> {
    // Move past the name
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

            if !inside_structure {
                variable_declarations.push(Arg {
                    name: name.to_owned(),
                    data_type: DataType::Function(arg_refs.clone(), return_type.to_owned()),
                    value: Value::None,
                });
            }

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
                    if !inside_structure {
                        variable_declarations.push(Arg {
                            name: name.to_owned(),
                            data_type: data_type.to_owned(),
                            value: Value::None,
                        });
                    }

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

    let parsed_expr = create_expression(
        tokens,
        i,
        inside_structure,
        &ast,
        &mut data_type,
        false,
        variable_declarations,
        token_line_numbers,
    )?;

    // Check if a type of collection / struct has been created
    // Or whether it is a literal or expression
    // If the expression is an empty expression when the variable is NOT a function, return an error
    let var_value = parsed_expr.get_value();
    let new_var = create_var_node(
        is_const,
        name.to_string(),
        var_value.to_owned(),
        is_exported,
        data_type.to_owned(),
        token_line_numbers[*i],
    );

    if !inside_structure {
        variable_declarations.push(Arg {
            name: name.to_owned(),
            data_type,
            value: var_value,
        });
    }

    Ok(new_var)
}

fn create_var_node(
    is_const: bool,
    var_name: String,
    var_value: Value,
    is_exported: bool,
    data_type: DataType,
    line_number: u32,
) -> AstNode {
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
