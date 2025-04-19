use super::{
    ast_nodes::AstNode, create_scene_node::new_scene,
    expressions::parse_expression::create_expression, variables::create_new_var_or_ref,
};
use crate::html_output::html_styles::get_html_styles;
use crate::parsers::ast_nodes::{Arg, Value};
use crate::parsers::functions::parse_function_call;
use crate::tokenizer::TokenPosition;
use crate::{CompileError, ErrorType, Token, bs_types::DataType};
use std::collections::HashMap;
use std::path::PathBuf;

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
    pub fn default() -> TokenContext {
        TokenContext {
            tokens: Vec::new(),
            index: 0,
            length: 0,
            token_positions: Vec::new(),
        }
    }
}

// This is a new scope
pub fn new_ast(
    x: &mut TokenContext,
    captured_declarations: &[Arg], // This includes imports
    return_type: &mut DataType,
    module_path: &PathBuf, // If empty, this isn't a module
    pure: &mut bool,       // No side effects or IO

                           // AST, Exports
) -> Result<Vec<AstNode>, CompileError> {
    let module_path = module_path.to_str().unwrap();

    // About 1/10 of the tokens seem to become AST nodes roughly from some very small preliminary tests
    let mut ast = Vec::with_capacity(x.length / 10);

    let mut needs_to_return = return_type != &DataType::None;
    let mut declarations = captured_declarations.to_vec();

    while x.index < x.length {
        // This should be starting after the imports
        let current_token = x.current_token().to_owned();

        match current_token {
            Token::Comment(value) => {
                ast.push(AstNode::Comment(value.clone()));
            }

            // Scene literals
            Token::SceneHead | Token::ParentScene => {
                if module_path.is_empty() {
                    return Err(CompileError {
                        msg: "Scene literals can only be used at the top level of a module"
                            .to_string(),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + i32::MAX,
                        },
                        error_type: ErrorType::Rule,
                    });
                }

                // Add the default core HTML styles as the initial unlocked styles
                let mut unlocked_styles = HashMap::from(get_html_styles());

                let scene = new_scene(x, &ast, &mut declarations, &mut unlocked_styles)?;

                ast.push(AstNode::Literal(scene, x.current_position()));
            }

            Token::ModuleStart(_) => {
                // In the future, need to structure into code blocks
            }

            // New Function or Variable declaration
            Token::Variable(name, is_exported) => {
                let new_var = create_new_var_or_ref(
                    x,
                    name.to_owned(),
                    &mut declarations,
                    is_exported,
                    &ast,
                    false,
                )?;

                if !new_var.get_value().is_pure() {
                    // red_ln!("flipping pure for {}", name);
                    *pure = false;
                }

                ast.push(new_var);
            }

            Token::JS(value) => {
                if module_path.is_empty() {
                    return Err(CompileError {
                        msg: "JS block can only be used inside of a module scope (not inside of a function)".to_string(),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + value.len() as i32,
                        },
                        error_type: ErrorType::Rule,
                    });
                }

                *pure = false;

                ast.push(AstNode::JS(value.clone(), x.current_position()));
            }

            // IGNORED TOKENS
            Token::Newline | Token::Empty | Token::SceneClose => {
                // Do nothing for now
            }

            Token::Import => {
                // Imports are just left in the token stream but don't continue here (At the moment)
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
                // Remove the entire declaration or scope of the variable declaration
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
                if !module_path.is_empty() {
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

                ast.push(AstNode::Return(return_value, x.current_position()));

                x.index -= 1;
            }

            Token::EOF => {
                break;
            }

            // TOKEN::End SHOULD NEVER BE IN MODULE SCOPE
            Token::End => {
                if !module_path.is_empty() {
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

    Ok(ast)
}

fn skip_dead_code(x: &mut TokenContext) {
    // Check what type of dead code it is
    // If it is a variable declaration, skip to the end of the declaration

    x.index += 1;
    match x.tokens.get(x.index).unwrap_or(&Token::EOF) {
        Token::DatatypeLiteral(_) => {
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
