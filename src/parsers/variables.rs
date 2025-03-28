use super::{
    ast_nodes::{Arg, AstNode},
    expressions::parse_expression::create_expression,
    functions::create_function,
};
use crate::parsers::ast_nodes::Value;
use crate::parsers::build_ast::TokenContext;
use crate::parsers::expressions::parse_expression::get_accessed_args;
use crate::parsers::functions::parse_function_call;
use crate::tokenizer::TokenPosition;
use crate::{bs_types::DataType, CompileError, ErrorType, Token};

pub fn create_new_var_or_ref(
    x: &mut TokenContext,
    name: String,
    variable_declarations: &mut Vec<Arg>,
    is_exported: bool,
    ast: &[AstNode],
    inside_structure: bool, // This allows parse_expression to know that new variable declarations are valid
) -> Result<AstNode, CompileError> {
    let token_line_number = x.token_positions[x.index].line_number;
    let token_start_pos = x.token_positions[x.index].char_column;

    // If this is a reference to a function or variable
    // This to_owned here is gross, probably a better way to avoid this
    if let Some(arg) = variable_declarations
        .to_owned()
        .iter()
        .find(|a| a.name == name)
    {
        return match arg.data_type {
            // Function Call
            DataType::Function(ref argument_refs, ref return_args) => parse_function_call(
                x,
                name,
                ast,
                variable_declarations,
                argument_refs,
                return_args,
            ),

            DataType::Structure(..) | DataType::Collection(..) => {
                let accessed_arg =
                    get_accessed_args(x, &arg.name, &arg.data_type, &mut Vec::new())?;

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
                let accessed_arg =
                    get_accessed_args(x, &arg.name, &arg.data_type, &mut Vec::new())?;

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
        x,
        name,
        is_exported,
        ast,
        &mut *variable_declarations,
        inside_structure,
    )
}

fn new_variable(
    x: &mut TokenContext,
    name: String,
    is_exported: bool,
    ast: &[AstNode],
    variable_declarations: &mut Vec<Arg>,
    inside_structure: bool,
) -> Result<AstNode, CompileError> {
    // Move past the name
    x.index += 1;
    let mut data_type = DataType::Inferred;
    let mut is_const = true;

    if x.tokens[x.index] == Token::Mutable {
        x.index += 1;
        is_const = false;
    }

    match x.tokens[x.index] {
        Token::Assign => {
            x.index += 1;
            // Check if this is a function declaration
            if x.tokens[x.index] == Token::FunctionKeyword {
                x.index += 1;
                let (function, arg_refs, return_type) =
                    create_function(x, name.to_owned(), is_exported, ast, variable_declarations)?;

                variable_declarations.push(Arg {
                    name: name.to_owned(),
                    data_type: DataType::Function(arg_refs.clone(), return_type.to_owned()),
                    value: Value::None,
                });

                if !inside_structure {
                    variable_declarations.push(Arg {
                        name: name.to_owned(),
                        data_type: DataType::Function(arg_refs.clone(), return_type.to_owned()),
                        value: Value::None,
                    });
                }

                return Ok(function);
            }
        }

        // Has a type declaration
        Token::TypeKeyword(ref type_keyword) => {
            data_type = type_keyword.to_owned();
            x.index += 1;

            match x.tokens[x.index] {
                Token::Assign => {
                    x.index += 1;
                }

                // If end of statement, then it's a zero value variable
                Token::Comma | Token::EOF | Token::Newline => {
                    return Ok(create_zero_value_var(
                        data_type,
                        name.to_string(),
                        is_exported,
                        x.token_positions[x.index].to_owned(),
                    ));
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
                            char_column: x.token_positions[x.index].char_column + name.len() as u32,
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
                    char_column: x.token_positions[x.index].char_column + name.len() as u32,
                },
                error_type: ErrorType::Syntax,
            });
        }
    };

    // Current token should be whatever is after assignment operator

    let parsed_expr = create_expression(
        x,
        inside_structure,
        ast,
        &mut data_type,
        false,
        variable_declarations,
    )?;

    // Check if a type of collection / struct has been created
    // Or whether it is a literal or expression
    // If the expression is an empty expression when the variable is NOT a function, return an error
    let var_value = parsed_expr;
    let new_var = AstNode::VarDeclaration(
        name.to_string(),
        var_value.to_owned(),
        is_exported,
        data_type.to_owned(),
        is_const,
        x.token_positions[x.index].to_owned(),
    );

    if !inside_structure {
        variable_declarations.push(Arg {
            name: name.to_string(),
            data_type,
            value: var_value,
        });
    }

    Ok(new_var)
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
