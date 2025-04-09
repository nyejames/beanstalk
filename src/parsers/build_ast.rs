use std::collections::HashMap;
use super::{
    ast_nodes::AstNode, create_scene_node::new_scene,
    expressions::parse_expression::create_expression, variables::create_new_var_or_ref,
};
use crate::html_output::html_styles::get_html_styles;
use crate::parsers::ast_nodes::{Arg, Value};
use crate::tokenizer::TokenPosition;
use crate::{bs_types::DataType, CompileError, ErrorType, Token};
use std::path::PathBuf;
use colour::red_ln;
use crate::parsers::functions::parse_function_call;

pub struct TokenContext {
    pub tokens: Vec<Token>,
    pub index: usize,
    pub length: usize,
    pub token_positions: Vec<TokenPosition>,
}
impl TokenContext {
    pub fn current_token(&self) -> &Token {
        // Do we actually ever need to do a bounds check here?
        debug_assert!(self.index < self.length);

        &self.tokens[self.index]
    }
    pub fn current_position(&self) -> TokenPosition {
        debug_assert!(self.index <= self.length);

        if self.index == self.length {
            return self.token_positions[self.index - 1].to_owned();
        }

        self.token_positions[self.index].to_owned()
    }
}

// This is a new scope
pub fn new_ast(
    x: &mut TokenContext,
    captured_declarations: &[Arg],
    return_type: &mut DataType,
    module: bool,
    pure: &mut bool, // No side effects or IO
    // AST, Imports
) -> Result<(Vec<AstNode>, Vec<AstNode>), CompileError> {
    
    // About 1/10 of the tokens seem to become AST nodes roughly from some very small preliminary tests
    let mut ast = Vec::with_capacity(x.length / 10);

    let mut imports = Vec::new();
    let mut exported: bool = false;
    let mut needs_to_return = return_type != &DataType::None;
    let mut declarations = captured_declarations.to_vec();

    while x.index < x.length {
        let current_token = x.current_token().to_owned();

        match current_token {
            Token::Comment(value) => {
                ast.push(AstNode::Comment(value.clone()));
            }

            Token::Import => {
                if !module {
                    return Err(CompileError {
                        msg: "Import found outside of module scope".to_string(),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + 6,
                        },
                        error_type: ErrorType::Rule,
                    });
                }

                x.index += 1;

                match &x.current_token() {
                    // Module path that will have all it's exports dumped into the module
                    Token::StringLiteral(value) => {
                        imports.push(AstNode::Use(
                            PathBuf::from(value.clone()),
                            TokenPosition {
                                line_number: x.current_position().line_number,
                                char_column: x.current_position().char_column,
                            },
                        ));
                    }
                    _ => {
                        return Err(CompileError {
                            msg: "Import must have a valid path as a argument".to_string(),
                            start_pos: x.current_position(),
                            end_pos: TokenPosition {
                                line_number: x.current_position().line_number,
                                char_column: x.current_position().char_column + u32::MAX,
                            },
                            error_type: ErrorType::Rule,
                        });
                    }
                }
            }

            // Scene literals
            Token::SceneHead | Token::ParentScene => {
                if !module {
                    return Err(CompileError {
                        msg: "Scene literals can only be used at the top level of a module"
                            .to_string(),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + u32::MAX,
                        },
                        error_type: ErrorType::Rule,
                    });
                }
                
                // Add the default core HTML styles as the initial unlocked styles
                let mut unlocked_styles = HashMap::from(get_html_styles());

                let scene = new_scene(x, &ast, &mut declarations, &mut unlocked_styles)?;

                ast.push(AstNode::Literal(
                    scene,
                    x.current_position(),
                ));
            }

            Token::ModuleStart(_) => {
                // In the future, need to structure into code blocks
            }

            // New Function or Variable declaration
            Token::Variable(name) => {
                let new_var = create_new_var_or_ref(
                    x,
                    name.to_owned(),
                    &mut declarations,
                    exported,
                    &ast,
                    false,
                )?;

                if !new_var.get_value().is_pure() {
                    // red_ln!("flipping pure for {}", name);
                    *pure = false;
                }

                ast.push(new_var);
            }

            Token::Public => {
                exported = true;
            }

            Token::JS(value) => {
                if !module {
                    return Err(CompileError {
                        msg: "JS block can only be used inside of a module scope (not inside of a function)".to_string(),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + value.len() as u32,
                        },
                        error_type: ErrorType::Rule,
                    });
                }

                *pure = false;

                ast.push(AstNode::JS(
                    value.clone(),
                    x.current_position(),
                ));
            }

            Token::Newline | Token::Empty | Token::SceneClose => {
                // Do nothing for now
            }

            // The actual print function doesn't exist in the compiler or standard library
            // This is a small compile time speed improvement as print is used all the time
            // Standard library function 'io' might have a bunch of special print functions inside it
            // e.g io.red("red hello")
            Token::Print => {
                // This module is no longer pure
                *pure = false;

                // Move past the print keyword
                x.index += 1;

                ast.push(parse_function_call(
                    x,
                    String::from("console.log"),
                    &ast,
                    &mut declarations,
                    &[Arg {
                        name: "".to_string(),
                        data_type: DataType::CoerceToString(false),
                        value: Value::None,
                    }],

                    // Console.log does not return anything
                    &DataType::None,
                    false,
                )?);
            }

            Token::DeadVariable(name) => {
                // Remove entire declaration or scope of variable declaration
                // So don't put any dead code into the AST
                skip_dead_code(x);
                ast.push(AstNode::Warning(
                    format!(
                        "Dead Variable Declaration. Variable is never used or declared: {}",
                        name
                    ),
                    x.current_position(),
                ));
            }

            Token::Return => {
                if module {
                    return Err(CompileError {
                        msg: "Return statement used outside of function".to_string(),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + 6,
                        },
                        error_type: ErrorType::Rule,
                    });
                }

                if !needs_to_return {
                    return Err(CompileError {
                        msg: "Return statement used in function that doesn't return a value"
                            .to_string(),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + 6,
                        },
                        error_type: ErrorType::Rule,
                    });
                }

                needs_to_return = false;
                x.index += 1;

                let return_value =
                    create_expression(x, false, &ast, return_type, false, &mut declarations)?;

                // if !return_value.is_pure() {
                //     *pure = false;
                // }

                ast.push(AstNode::Return(
                    return_value,
                    x.current_position(),
                ));

                x.index -= 1;
            }

            Token::EOF => {
                break;
            }

            // TOKEN::End SHOULD NEVER BE IN MODULE SCOPE
            Token::End => {
                if module {
                    return Err(CompileError {
                        msg: "End statement used in module scope (too many end statements used?)"
                            .to_string(),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + 3,
                        },
                        error_type: ErrorType::Rule,
                    });
                }

                x.index += 1;
                break;
            }

            // Or stuff that hasn't been implemented yet
            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Token not recognised by AST parser when creating AST: {:?}",
                        &x.current_token()
                    ),
                    start_pos: x.current_position(),
                    end_pos: TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column + 1,
                    },
                    error_type: ErrorType::Compiler,
                });
            }
        }

        x.index += 1;
    }

    if needs_to_return {
        return Err(CompileError {
            msg: "Function does not return a value".to_string(),
            start_pos: x.token_positions[x.index - 1].to_owned(),
            end_pos: TokenPosition {
                line_number: x.token_positions[x.index - 1].line_number,
                char_column: x.token_positions[x.index - 1].char_column + 1,
            },
            error_type: ErrorType::Rule,
        });
    }

    Ok((ast, imports))
}

fn skip_dead_code(x: &mut TokenContext) {
    // Check what type of dead code it is
    // If it is a variable declaration, skip to the end of the declaration

    x.index += 1;
    match x.tokens.get(x.index).unwrap_or(&Token::EOF) {
        Token::TypeKeyword(_) => {
            x.index += 1;
            match x.tokens.get(x.index).unwrap_or(&Token::EOF) {
                Token::Assign => {
                    x.index += 1;
                }
                _ => {
                    return;
                }
            }
        }
        Token::Assign => {
            x.index += 1;
        }
        Token::Newline => {
            x.index += 1;
            return;
        }
        _ => {
            return;
        }
    }

    // Skip to end of variable declaration
    let mut open_parenthesis = 0;
    while let Some(token) = x.tokens.get(x.index) {
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

        x.index += 1;
    }
}
