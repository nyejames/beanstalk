use super::{
    ast_nodes::AstNode, create_scene_node::new_scene,
    expressions::parse_expression::create_expression, variables::create_new_var_or_ref,
};
use crate::parsers::ast_nodes::{Arg, Value};
use crate::parsers::structs::{new_struct, struct_to_value};
use crate::tokenizer::TokenPosition;
use crate::{bs_types::DataType, CompileError, ErrorType, Token};
use std::path::PathBuf;

// This is a new block that captures it's parents variable declarations
pub fn new_ast(
    tokens: Vec<Token>,
    i: &mut usize,
    token_pos: &Vec<TokenPosition>,
    variable_declarations: &mut Vec<Arg>,
    return_args: &Vec<Arg>,
    module_scope: bool,
    // AST, Imports
) -> Result<(Vec<AstNode>, Vec<AstNode>), CompileError> {
    let mut ast = Vec::new();
    let mut imports = Vec::new();
    let mut exported: bool = false;
    let mut needs_to_return = !return_args.is_empty();

    while *i < tokens.len() {
        match &tokens[*i] {
            Token::Comment(value) => {
                ast.push(AstNode::Comment(value.clone()));
            }

            Token::Import => {
                if !module_scope {
                    return Err(CompileError {
                        msg: "Import found outside of module scope".to_string(),
                        start_pos: token_pos[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_pos[*i].line_number,
                            char_column: token_pos[*i].char_column + 6,
                        },
                        error_type: ErrorType::Rule,
                    });
                }

                *i += 1;
                match &tokens[*i] {
                    // Module path that will have all it's exports dumped into the module
                    Token::StringLiteral(value) => {
                        imports.push(AstNode::Use(
                            PathBuf::from(value.clone()),
                            TokenPosition {
                                line_number: token_pos[*i].line_number,
                                char_column: token_pos[*i].char_column,
                            },
                        ));
                    }
                    _ => {
                        return Err(CompileError {
                            msg: "Import must have a valid path as a argument".to_string(),
                            start_pos: token_pos[*i].to_owned(),
                            end_pos: TokenPosition {
                                line_number: token_pos[*i].line_number,
                                char_column: token_pos[*i].char_column + u32::MAX,
                            },
                            error_type: ErrorType::Rule,
                        });
                    }
                }
            }

            // Scene literals
            Token::SceneHead | Token::ParentScene => {
                if !module_scope {
                    return Err(CompileError {
                        msg: "Scene literals can only be used at the top level of a module"
                            .to_string(),
                        start_pos: token_pos[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_pos[*i].line_number,
                            char_column: token_pos[*i].char_column + u32::MAX,
                        },
                        error_type: ErrorType::Rule,
                    });
                }

                let scene = new_scene(&tokens, i, &ast, token_pos, variable_declarations)?;

                ast.push(AstNode::Literal(
                    scene,
                    TokenPosition {
                        line_number: token_pos[*i].line_number,
                        char_column: token_pos[*i].char_column,
                    },
                ));
            }

            Token::ModuleStart(_) => {
                // In the future, need to structure into code blocks
            }

            // New Function or Variable declaration
            Token::Variable(name) => {
                ast.push(create_new_var_or_ref(
                    name,
                    variable_declarations,
                    &tokens,
                    i,
                    exported,
                    &ast,
                    token_pos,
                    false,
                )?);
            }

            Token::Public => {
                exported = true;
            }

            Token::JS(value) => {
                ast.push(AstNode::JS(
                    value.clone(),
                    TokenPosition {
                        line_number: token_pos[*i].line_number,
                        char_column: token_pos[*i].char_column,
                    },
                ));
            }

            Token::Title => {
                *i += 1;
                match &tokens[*i] {
                    Token::StringLiteral(value) => {
                        ast.push(AstNode::Title(
                            value.clone(),
                            TokenPosition {
                                line_number: token_pos[*i].line_number,
                                char_column: token_pos[*i].char_column,
                            },
                        ));
                    }
                    _ => {
                        return Err(CompileError {
                            msg: "Title must have a valid string as a argument".to_string(),
                            error_type: ErrorType::Rule,
                            start_pos: token_pos[*i].to_owned(),
                            end_pos: TokenPosition {
                                line_number: token_pos[*i].line_number,
                                char_column: token_pos[*i].char_column + u32::MAX,
                            },
                        });
                    }
                }
            }

            Token::Date => {
                *i += 1;
                match &tokens[*i] {
                    Token::StringLiteral(value) => {
                        ast.push(AstNode::Date(value.clone(), token_pos[*i].to_owned()));
                    }
                    _ => {
                        return Err(CompileError {
                            msg: "Date must have a valid string as a argument".to_string(),
                            error_type: ErrorType::Rule,
                            start_pos: token_pos[*i].to_owned(),
                            end_pos: TokenPosition {
                                line_number: token_pos[*i].line_number,
                                char_column: token_pos[*i].char_column + u32::MAX,
                            },
                        });
                    }
                }
            }

            Token::Newline | Token::Empty | Token::SceneClose(_) => {
                // Do nothing for now
            }

            // The actual print function doesn't exist in the compiler or standard library
            // This is a small compile time speed improvement as print is used all the time
            // Standard library function 'io' might have a bunch of special print functions inside it
            // e.g io.red("red hello")
            Token::Print => {
                // Move past the print keyword
                *i += 1;

                // Make sure there is an open parenthesis
                if tokens.get(*i) != Some(&Token::OpenParenthesis) {
                    return Err(CompileError {
                        msg: "Expected an open parenthesis after the print keyword".to_string(),
                        start_pos: token_pos[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_pos[*i].line_number,
                            char_column: token_pos[*i].char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }

                *i += 1;

                // Get the struct of args passed into the function call
                let structure = new_struct(
                    Value::None,
                    &tokens,
                    &mut *i,
                    &Vec::new(),
                    &ast,
                    &mut variable_declarations.to_owned(),
                    token_pos,
                )?;

                ast.push(AstNode::Print(
                    struct_to_value(&structure),
                    token_pos[*i].to_owned(),
                ));
            }

            Token::DeadVariable(name) => {
                // Remove entire declaration or scope of variable declaration
                // So don't put any dead code into the AST
                skip_dead_code(&tokens, i);
                ast.push(AstNode::Warning(
                    format!(
                        "Dead Variable Declaration. Variable is never used or declared: {}",
                        name
                    ),
                    token_pos[*i].to_owned(),
                ));
            }

            Token::Return => {
                if module_scope {
                    return Err(CompileError {
                        msg: "Return statement used outside of function".to_string(),
                        start_pos: token_pos[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_pos[*i].line_number,
                            char_column: token_pos[*i].char_column + 6,
                        },
                        error_type: ErrorType::Rule,
                    });
                }

                if !needs_to_return {
                    return Err(CompileError {
                        msg: "Return statement used in function that doesn't return a value"
                            .to_string(),
                        start_pos: token_pos[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_pos[*i].line_number,
                            char_column: token_pos[*i].char_column + 6,
                        },
                        error_type: ErrorType::Rule,
                    });
                }

                needs_to_return = false;
                *i += 1;

                let mut return_type = if return_args.len() > 1 {
                    DataType::Structure(return_args.to_owned())
                } else {
                    return_args[0].data_type.to_owned()
                };

                let return_value = create_expression(
                    &tokens,
                    i,
                    false,
                    &ast,
                    &mut return_type,
                    false,
                    variable_declarations,
                    token_pos,
                )?;

                ast.push(AstNode::Return(return_value, token_pos[*i].to_owned()));

                *i -= 1;
            }

            Token::EOF => {
                break;
            }

            // TOKEN::End SHOULD NEVER BE IN MODULE SCOPE
            Token::End => {
                if module_scope {
                    return Err(CompileError {
                        msg: "End statement used in module scope (too many end statements used?)"
                            .to_string(),
                        start_pos: token_pos[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_pos[*i].line_number,
                            char_column: token_pos[*i].char_column + 3,
                        },
                        error_type: ErrorType::Rule,
                    });
                }

                *i += 1;
                break;
            }

            // Or stuff that hasn't been implemented yet
            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Token not recognised by AST parser when creating AST: {:?}",
                        &tokens[*i]
                    ),
                    start_pos: token_pos[*i].to_owned(),
                    end_pos: TokenPosition {
                        line_number: token_pos[*i].line_number,
                        char_column: token_pos[*i].char_column + 1,
                    },
                    error_type: ErrorType::Compiler,
                });
            }
        }

        *i += 1;
    }

    if needs_to_return {
        return Err(CompileError {
            msg: "Function does not return a value".to_string(),
            start_pos: token_pos[*i - 1].to_owned(),
            end_pos: TokenPosition {
                line_number: token_pos[*i - 1].line_number,
                char_column: token_pos[*i - 1].char_column + 1,
            },
            error_type: ErrorType::Rule,
        });
    }

    Ok((ast, imports))
}

fn skip_dead_code(tokens: &Vec<Token>, i: &mut usize) {
    // Check what type of dead code it is
    // If it is a variable declaration, skip to the end of the declaration

    *i += 1;
    match tokens.get(*i).unwrap_or(&Token::EOF) {
        Token::TypeKeyword(_) => {
            *i += 1;
            match tokens.get(*i).unwrap_or(&Token::EOF) {
                Token::Assign => {
                    *i += 1;
                }
                _ => {
                    return;
                }
            }
        }
        Token::Assign => {
            *i += 1;
        }
        Token::Newline => {
            *i += 1;
            return;
        }
        _ => {
            return;
        }
    }

    // Skip to end of variable declaration
    let mut open_parenthesis = 0;
    while let Some(token) = tokens.get(*i) {
        match token {
            Token::OpenParenthesis => {
                open_parenthesis += 1;
            }
            Token::CloseParenthesis => {
                open_parenthesis -= 1;
            }
            Token::Newline => {
                if open_parenthesis < 1 {
                    return;
                }
            }
            Token::EOF | Token::End => {
                break;
            }
            _ => {}
        }

        *i += 1;
    }
}

// pub fn get_var_declaration_type(var_name: String, ast: &Vec<AstNode>) -> DataType {
//     for node in ast {
//         match node {
//             AstNode::VarDeclaration(name, _, _, data_type, _) => {
//                 if *name == var_name {
//                     return data_type.to_owned();
//                 }
//             }
//             _ => {}
//         }
//     }

//     DataType::Inferred
// }
