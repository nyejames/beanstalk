use super::{
    ast_nodes::AstNode, create_scene_node::new_scene,
    expressions::parse_expression::create_expression,
};
// use crate::html_output::html_styles::get_html_styles;
use crate::parsers::ast_nodes::{Arg, Expr};
use crate::parsers::expressions::parse_expression::{
    create_multiple_expressions, create_args_from_types,
};
use crate::parsers::functions::parse_function_call;
use crate::parsers::loops::create_loop;
use crate::parsers::scene::{SceneType, Style};
use crate::parsers::variables::{get_reference, mutated_arg, new_arg};
use crate::tokenizer::TokenPosition;
use crate::tokens::VarVisibility;
use crate::{CompileError, ErrorType, Token, bs_types::DataType};
use std::collections::HashMap;

pub struct TokenContext {
    pub tokens: Vec<Token>,
    pub index: usize,
    pub length: usize,
    pub token_positions: Vec<TokenPosition>,
}
impl TokenContext {
    pub fn current_token(&self) -> &Token {
        // Do we actually ever need to do a bound check here?
        debug_assert!(self.index < self.length);

        &self.tokens[self.index]
    }
    pub fn token_start_position(&self) -> TokenPosition {
        debug_assert!(self.index <= self.length);

        if self.index == self.length {
            return self.token_positions[self.index - 1].to_owned();
        }

        self.token_positions[self.index].to_owned()
    }
    pub fn token_end_position(&self) -> TokenPosition {
        debug_assert!(self.index <= self.length);

        if self.index == self.length {
            return self.token_positions[self.index - 1].to_owned();
        }

        let current_token_dimensions = self.current_token().dimensions();
        let current_token_start_position = self.token_start_position();

        TokenPosition {
            line_number: current_token_start_position.line_number
                + current_token_dimensions.line_number,
            char_column: current_token_start_position.char_column
                + current_token_dimensions.char_column,
        }
    }

    pub fn advance(&mut self) {
        self.index += 1;
    }
    pub fn go_back(&mut self) {
        self.index -= 1;
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
    captured_declarations: &[Arg],
    returns: &[DataType],
    global_scope: bool,
) -> Result<Expr, CompileError> {
    // About 1/10 of the tokens seem to become AST nodes roughly from some very small preliminary tests
    let mut ast = Vec::with_capacity(x.length / 10);
    let mut declarations = captured_declarations.to_vec();

    let mut exports = Vec::new();

    while x.index < x.length {
        // This should be starting after the imports
        let current_token = x.current_token().to_owned();
        let start_pos = x.token_start_position(); // Store start position for potential nodes

        match current_token {
            Token::Comment(..) => {
                // ast.push(AstNode::Comment(value.clone()));
            }

            // Scene literals
            Token::SceneHead | Token::ParentScene => {
                // Add the default core HTML styles as the initially unlocked styles
                // let mut unlocked_styles = HashMap::from(get_html_styles());

                let scene = new_scene(x, &declarations, &mut HashMap::new(), &mut Style::default())?;

                match scene {
                    SceneType::Scene(expr) => {
                        ast.push(AstNode::Reference(expr, x.token_start_position()));
                    }
                    SceneType::Slot => {
                        return Err(CompileError {
                            msg: "Slot can't be used at the top level of a scene. Slots can only be used inside of other scenes".to_string(),
                            start_pos: x.token_start_position(),
                            end_pos: TokenPosition {
                                line_number: x.token_start_position().line_number,
                                char_column: x.token_start_position().char_column + 5,
                            },
                            error_type: ErrorType::Rule,
                        })
                    }
                    _ => {}
                }
            }

            Token::ModuleStart(..) => {
                // Ignored during AST creation but used to look up the name of the module efficiently
                // Is used to help name space variable names to avoid clashes with scenes across modules
            }

            // New Function or Variable declaration
            Token::Variable(ref name, visibility) => {
                if let Some(arg) = get_reference(name, &declarations) {
                    // Then the associated mutation afterwards
                    ast.push(mutated_arg(x, arg, &declarations)?);
                } else {
                    let arg = new_arg(x, name, &declarations)?;

                    if visibility == VarVisibility::Public {
                        exports.push(arg.to_owned());
                    }

                    declarations.push(arg.to_owned());

                    ast.push(AstNode::Declaration(
                        name.to_owned(),
                        arg.expr.to_owned(),
                        visibility.to_owned(),
                        arg.data_type,
                        x.token_start_position(),
                    ));
                }
            }

            // Modifiers
            Token::Public | Token::Private | Token::Mutable => {
                // Ignoring for now as the tokenizer is doing the important stuff
                // TODO - add some helpful errors if these are used in the wrong context here
            }

            // Control Flow
            Token::For => {
                x.advance();

                ast.push(create_loop(x, returns, &declarations)?);
            }

            Token::If => {
                x.advance();

                let condition =
                    create_expression(x, &mut DataType::Bool(false), false, &declarations, &[])?;

                // TODO - fold evaluated if statements
                // If this condition isn't runtime,
                // The statement can be removed completely;
                // I THINK, NOT SURE HOW 'ELSE' AND ALL THAT WORK YET

                if x.current_token() != &Token::Colon {
                    return Err(CompileError {
                        msg: format!(
                            "Expected ':' after the if condition, found {:?}",
                            x.current_token()
                        ),
                        start_pos: x.token_start_position(),
                        end_pos: x.token_start_position(),
                        error_type: ErrorType::Syntax,
                    });
                }

                x.advance(); // Consume ':'

                ast.push(AstNode::If(
                    condition,
                    // This
                    new_ast(x, &declarations, returns, global_scope)?,
                    start_pos,
                ))
            }

            Token::JS(value) => {
                ast.push(AstNode::JS(value.clone(), x.token_start_position()));
            }

            // IGNORED TOKENS
            Token::Newline | Token::Empty | Token::SceneClose => {
                // Do nothing for now
            }

            Token::Import => {
                // Imports are just left in the token stream but don't continue here (At the moment)
            }

            Token::Print => {
                // Move past the print keyword
                x.advance();

                ast.push(parse_function_call(
                    x,
                    "console.log",
                    &declarations,
                    &[Arg {
                        name: "".to_string(),
                        data_type: DataType::CoerceToString(false),
                        expr: Expr::None,
                    }],
                    // Console.log does not return anything
                    &[],
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
                    x.token_start_position(),
                ));
            }

            Token::Return => {
                x.advance();

                let return_values = create_multiple_expressions(x, returns, &declarations)?;

                // if !return_value.is_pure() {
                //     *pure = false;
                // }

                ast.push(AstNode::Return(return_values, x.token_start_position()));
                x.index -= 1;
            }

            Token::EOF => {
                break;
            }

            Token::End => {
                x.advance();
                break;
            }

            Token::Settings => {
                let config = new_arg(x, "config", &declarations)?;

                let config = match config.expr {
                    Expr::Block(_, _, return_data_types) => {
                        create_args_from_types(&return_data_types)
                    }
                    _ => {
                        return Err(CompileError {
                            msg: "Settings must be a block".to_string(),
                            start_pos: x.token_start_position(),
                            end_pos: x.token_start_position(),
                            error_type: ErrorType::Syntax,
                        });
                    }
                };

                ast.push(AstNode::Settings(config, x.token_start_position()));
            }

            // Or stuff that hasn't been implemented yet
            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Token not recognised by AST parser when creating AST: {:?}",
                        &x.current_token()
                    ),
                    start_pos: x.token_start_position(),
                    end_pos: TokenPosition {
                        line_number: x.token_start_position().line_number,
                        char_column: x.token_start_position().char_column + 1,
                    },
                    error_type: ErrorType::Compiler,
                });
            }
        }

        x.advance();
    }

    Ok(Expr::Block(
        Vec::from(captured_declarations),
        ast,
        returns.to_owned(),
    ))
}

fn skip_dead_code(x: &mut TokenContext) {
    // Check what type of dead code it is
    // If it is a variable declaration, skip to the end of the declaration

    x.advance();
    match x.tokens.get(x.index).unwrap_or(&Token::EOF) {
        Token::DatatypeLiteral(_) => {
            x.advance();
            match x.tokens.get(x.index).unwrap_or(&Token::EOF) {
                Token::Assign => {
                    x.advance();
                }
                _ => {
                    return;
                }
            }
        }
        Token::Assign => {
            x.advance();
        }
        Token::Newline => {
            x.advance();
            return;
        }
        _ => {
            return;
        }
    }

    // Skip to the end of variable declaration
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

        x.advance();
    }
}
