use super::{
    ast_nodes::AstNode,
    create_scene_node::new_scene,
    expressions::parse_expression::create_expression,
    variables::create_new_var_or_ref,
};
use crate::{bs_types::DataType, CompileError, Token};
use std::path::PathBuf;
use crate::parsers::ast_nodes::{Arg, Value};
use crate::parsers::functions::create_func_call_args;
use crate::parsers::tuples::new_tuple;

pub fn new_ast(
    tokens: Vec<Token>,
    i: &mut usize,
    token_line_numbers: &Vec<u32>,
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
                        msg: "Error: Import found outside of module scope".to_string(),
                        line_number: token_line_numbers[*i],
                    });
                }

                *i += 1;
                match &tokens[*i] {
                    // Module path that will have all it's exports dumped into the module
                    Token::StringLiteral(value) => {
                        imports.push(AstNode::Use(PathBuf::from(value.clone()), token_line_numbers[*i]));
                    }
                    _ => {
                        ast.push(AstNode::Error(
                            "Import must have a valid path as a argument".to_string(),
                            token_line_numbers[*i],
                        ));
                    }
                }
            }

            // Scene literals
            Token::SceneHead | Token::ParentScene => {
                if !module_scope {
                    return Err(CompileError {
                        msg: "Scene literals can only be used at the top level of a module".to_string(),
                        line_number: token_line_numbers[*i],
                    });
                }

                let scene = new_scene(
                    &tokens,
                    i,
                    &ast,
                    token_line_numbers,
                    variable_declarations,
                )?;

                ast.push(AstNode::Literal(scene, token_line_numbers[*i]));
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
                    token_line_numbers,
                    false,
                )?);
            }

            Token::Export => {
                exported = true;
            }
            Token::JS(value) => {
                ast.push(AstNode::JS(value.clone(), token_line_numbers[*i]));
            }
            Token::Title => {
                *i += 1;
                match &tokens[*i] {
                    Token::StringLiteral(value) => {
                        ast.push(AstNode::Title(value.clone(), token_line_numbers[*i]));
                    }
                    _ => {
                        ast.push(AstNode::Error(
                            "Title must have a valid string as a argument".to_string(),
                            token_line_numbers[*i],
                        ));
                    }
                }
            }

            Token::Date => {
                *i += 1;
                match &tokens[*i] {
                    Token::StringLiteral(value) => {
                        ast.push(AstNode::Date(value.clone(), token_line_numbers[*i]));
                    }
                    _ => {
                        ast.push(AstNode::Error(
                            "Date must have a valid string as a argument".to_string(),
                            token_line_numbers[*i],
                        ));
                    }
                }
            }

            Token::Newline | Token::Empty | Token::SceneClose(_) => {
                // Do nothing for now
            }

            Token::Print => {
                // Move past the print keyword
                *i += 1;
                let tuple = new_tuple(
                    Value::None,
                    &tokens,
                    i,
                    &Vec::new(),
                    &ast,
                    variable_declarations,
                    &token_line_numbers,
                )?;

                let args = create_func_call_args(&tuple, &Vec::new(), &token_line_numbers[*i])?;
                ast.push(AstNode::Print(args, token_line_numbers[*i]));
            }

            Token::DeadVariable(name) => {
                // Remove entire declaration or scope of variable declaration
                // So don't put any dead code into the AST
                skip_dead_code(&tokens, i);
                ast.push(AstNode::Error(
                    format!(
                        "Dead Variable Declaration. Variable is never used or declared: {}",
                        name
                    ),
                    token_line_numbers[*i - 1],
                ));
            }

            Token::Return => {
                if module_scope {
                    ast.push(AstNode::Error(
                        "Return statement used outside of function".to_string(),
                        token_line_numbers[*i],
                    ));
                }

                if !needs_to_return {
                    ast.push(AstNode::Error(
                        "Return statement used in function that doesn't return a value".to_string(),
                        token_line_numbers[*i],
                    ));
                }

                needs_to_return = false;
                *i += 1;
                
                let mut return_type = if return_args.len() > 1 {
                    DataType::Tuple(return_args.to_owned())
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
                    token_line_numbers,
                )?;

                ast.push(AstNode::Return(return_value, token_line_numbers[*i]));

                *i -= 1;
            }

            // TOKEN END SHOULD NEVER BE AT TOP LEVEL
            // This is to break out of blocks only
            // There should be a way to handle this to throw a syntax error if 'end' is used at the top level
            Token::EOF => {
                break;
            }

            Token::End => {
                *i += 1;
                break;
            }

            // Or stuff that hasn't been implemented yet
            _ => {
                ast.push(AstNode::Error(
                    format!("Compiler Error: Token not recognised by AST parser when creating AST: {:?}", &tokens[*i] ).to_string(),
                    token_line_numbers[*i - 1],
                ));
            }
        }

        *i += 1;
    }

    if needs_to_return {
        ast.push(AstNode::Error(
            "Function does not return a value".to_string(),
            token_line_numbers[*i - 1],
        ));
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
