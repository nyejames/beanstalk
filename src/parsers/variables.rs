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
    inside_collection: bool, // This allows parse_expression to know that new variable declarations are valid
) -> Result<AstNode, CompileError> {
    let token_line_number = x.current_position().line_number;
    let token_start_pos = x.current_position().char_column;

    // If this is a reference to a function or variable
    // This to_owned here is gross, probably a better way to avoid this
    if let Some(arg) = variable_declarations
        .to_owned()
        .iter()
        .find(|a| a.name == name)
    {
        
        return match arg.data_type {

            // Function Call
            DataType::Function(ref argument_refs, ref return_type) => {
                x.index += 1;
                // blue_ln!("arg value purity: {:?}, for {}",  arg.value.is_pure(), name);
                parse_function_call(
                    x,
                    name,
                    ast,
                    variable_declarations,
                    argument_refs,
                    return_type,
                    arg.value.is_pure(),
                )
            },

            _ => {
                // Check to make sure there is no access attempt on any other types
                // Get accessed arg will return an error if there is an access attempt on the wrong type
                // This SHOULD always be None (for now) but this is being assigned to the reference here
                // incase the language changes in the future and properties/methods are added to other types
                let accessed_arg =
                    get_accessed_args(x, &arg.name, &arg.data_type, &mut Vec::new())?;

                // If the value isn't wrapped in a runtime value,
                // Replace the reference with a literal value
                if arg.value.is_pure() {
                    return Ok(AstNode::Literal(
                        arg.value.to_owned(),
                        TokenPosition {
                            line_number: token_line_number,
                            char_column: token_start_pos,
                        },
                    ))
                }

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
        inside_collection,
    )
}

fn new_variable(
    x: &mut TokenContext,
    name: String,
    is_exported: bool,
    ast: &[AstNode],
    variable_declarations: &mut Vec<Arg>,
    inside_collection: bool,
) -> Result<AstNode, CompileError> {
    
    // Move past the name
    let mut data_type = DataType::Inferred(false);
    x.index += 1;

    match x.tokens[x.index] {
        Token::Assign => {
            x.index += 1;

            if x.tokens[x.index] == Token::FunctionKeyword {
                x.index += 1;
                let (function, arg_refs, return_type) =
                    create_function(x, name.to_owned(), is_exported, ast, variable_declarations)?;

                if !inside_collection {
                    variable_declarations.push(Arg {
                        name: name.to_owned(),
                        data_type: DataType::Function(arg_refs.clone(), Box::new(return_type)),
                        value: function.get_value(),
                    });
                }

                return Ok(function);
            }
        }

        // Has a type declaration
        Token::DatatypeLiteral(ref type_keyword) => {
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
        inside_collection,
        ast,
        &mut data_type,
        false,
        variable_declarations,
    )?;

    // println!("parsed expr: {:?}", parsed_expr);

    // Check if a type of collection / struct has been created
    // Or whether it is a literal or expression
    // If the expression is an empty expression when the variable is NOT a function, return an error
    let var_value = parsed_expr;
    let new_var = AstNode::VarDeclaration(
        name.to_string(),
        var_value.to_owned(),
        is_exported,
        data_type.to_owned(),
        x.token_positions[x.index].to_owned(),
    );

    if !inside_collection {
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
        DataType::Float(_) => AstNode::VarDeclaration(
            name,
            Value::Float(0.0),
            is_exported,
            data_type,
            token_position,
        ),

        DataType::Int(_) => AstNode::VarDeclaration(
            name,
            Value::Int(0),
            is_exported,
            data_type,
            token_position,
        ),

        DataType::String(_) => AstNode::VarDeclaration(
            name,
            Value::String("".to_string()),
            is_exported,
            data_type,
            token_position,
        ),

        DataType::Bool(_) => AstNode::VarDeclaration(
            name,
            Value::Bool(false),
            is_exported,
            data_type,
            token_position,
        ),

        _ => AstNode::VarDeclaration(
            name,
            Value::None,
            is_exported,
            data_type,
            token_position,
        ),
    }
}
