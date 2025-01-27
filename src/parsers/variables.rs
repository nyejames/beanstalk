use super::{
    ast_nodes::{Arg, AstNode},
    expressions::parse_expression::create_expression,
    functions::create_function,
};
use crate::parsers::ast_nodes::{NodeInfo, Value};
use crate::parsers::expressions::parse_expression::get_accessed_args;
use crate::parsers::functions::parse_function_call;
use crate::tokenizer::TokenPosition;
use crate::{bs_types::DataType, CompileError, ErrorType, Token};

pub fn create_new_var_or_ref(
    name: &String,
    variable_declarations: &mut Vec<Arg>,
    tokens: &Vec<Token>,
    i: &mut usize,
    is_exported: bool,
    ast: &Vec<AstNode>,
    token_pos: &Vec<TokenPosition>,
    inside_structure: bool, // This allows parse_expression to know that new variable declarations are valid
) -> Result<AstNode, CompileError> {
    let is_const = &name.to_uppercase() == name;

    let token_line_number = token_pos[*i].line_number;
    let token_start_pos = token_pos[*i].char_column;

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
                token_pos,
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
                    token_pos,
                    &mut Vec::new(),
                )?;

                Ok(AstNode::Literal(
                    Value::Reference(arg.name.to_owned(), arg.data_type.to_owned(), accessed_arg),
                    TokenPosition {
                        line_number: token_line_number,
                        char_column: token_start_pos,
                    },
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
                    token_pos,
                    &mut Vec::new(),
                )?;

                Ok(AstNode::Literal(
                    Value::Reference(arg.name.to_owned(), arg.data_type.to_owned(), accessed_arg),
                    TokenPosition {
                        line_number: token_line_number,
                        char_column: token_start_pos,
                    },
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
        token_pos,
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
    token_pos: &Vec<TokenPosition>,
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
                token_pos,
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
                        token_pos[*i].to_owned(),
                    ));
                }
                _ => {
                    return Err(CompileError {
                        msg: format!(
                            "Variable of type: {:?} does not exist in this scope",
                            data_type
                        ),
                        start_pos: token_pos[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_pos[*i].line_number,
                            char_column: token_pos[*i].char_column + name.len() as u32,
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
                    name, tokens[*i]
                ),
                start_pos: token_pos[*i].to_owned(),
                end_pos: TokenPosition {
                    line_number: token_pos[*i].line_number,
                    char_column: token_pos[*i].char_column + name.len() as u32,
                },
                error_type: ErrorType::Syntax,
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
        token_pos,
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
        token_pos[*i].to_owned(),
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
    token_position: TokenPosition,
) -> AstNode {
    if is_const {
        return AstNode::VarDeclaration(
            var_name,
            var_value.to_owned(),
            is_exported,
            data_type,
            true,
            token_position,
        );
    }

    AstNode::VarDeclaration(
        var_name,
        var_value.to_owned(),
        is_exported,
        data_type,
        false,
        token_position,
    )
}

fn create_zero_value_var(
    data_type: DataType,
    name: String,
    is_exported: bool,
    token_position: TokenPosition,
) -> AstNode {
    match data_type {
        DataType::Float => AstNode::VarDeclaration(
            name,
            Value::Float(0.0),
            is_exported,
            data_type,
            false,
            token_position,
        ),

        DataType::Int => AstNode::VarDeclaration(
            name,
            Value::Int(0),
            is_exported,
            data_type,
            false,
            token_position,
        ),

        DataType::String => AstNode::VarDeclaration(
            name,
            Value::String("".to_string()),
            is_exported,
            data_type,
            false,
            token_position,
        ),

        DataType::Bool => AstNode::VarDeclaration(
            name,
            Value::Bool(false),
            is_exported,
            data_type,
            false,
            token_position,
        ),

        _ => AstNode::VarDeclaration(
            name,
            Value::None,
            is_exported,
            data_type,
            false,
            token_position,
        ),
    }
}
