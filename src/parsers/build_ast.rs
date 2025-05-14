use super::{
    ast_nodes::AstNode, create_scene_node::new_scene,
    expressions::parse_expression::create_expression, variables::create_new_var_or_ref,
};
// use crate::html_output::html_styles::get_html_styles;
use crate::parsers::ast_nodes::{Arg, Expr};
use crate::parsers::expressions::parse_expression::{
    create_multiple_expressions, get_arguments_from_datatypes,
};
use crate::parsers::functions::parse_function_call;
use crate::parsers::scene::{SceneType, Style};
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
    pub fn current_position(&self) -> TokenPosition {
        debug_assert!(self.index <= self.length);

        if self.index == self.length {
            return self.token_positions[self.index - 1].to_owned();
        }

        self.token_positions[self.index].to_owned()
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

    // This Maybe should be separating imports, as this is args and imports
    // And only imports should be captured by nested blocks
    captured_declarations: &[Arg],

    returns: &[DataType],
) -> Result<Expr, CompileError> {
    // About 1/10 of the tokens seem to become AST nodes roughly from some very small preliminary tests
    let mut ast = Vec::with_capacity(x.length / 10);
    let mut declarations = captured_declarations.to_vec();

    let mut exports = Vec::new();

    while x.index < x.length {
        // This should be starting after the imports
        let current_token = x.current_token().to_owned();
        let start_pos = x.current_position(); // Store start position for potential nodes

        match current_token {
            Token::Comment(value) => {
                ast.push(AstNode::Comment(value.clone()));
            }

            // Scene literals
            Token::SceneHead | Token::ParentScene => {
                // Add the default core HTML styles as the initially unlocked styles
                // let mut unlocked_styles = HashMap::from(get_html_styles());

                let scene = new_scene(x, &mut declarations, &mut HashMap::new(), Style::default())?;

                match scene {
                    SceneType::Scene(expr) => {
                        ast.push(AstNode::Literal(expr, x.current_position()));
                    }
                    SceneType::Slot => {
                        return Err(CompileError {
                            msg: "Slot can't be used at the top level of a scene. Slots can only be used inside of other scenes".to_string(),
                            start_pos: x.current_position(),
                            end_pos: TokenPosition {
                                line_number: x.current_position().line_number,
                                char_column: x.current_position().char_column + 5,
                            },
                            error_type: ErrorType::Rule,
                        })
                    }
                    _ => {}
                }
            }

            Token::ModuleStart => {
                // TODO - figure out if we are using this or get rid of it
                // In the future, need to structure into code blocks
            }

            // New Function or Variable declaration
            Token::Variable(ref name, is_exported, ..) => {
                let new_var = create_new_var_or_ref(x, name, &declarations, &is_exported)?;

                // Make sure this is a new variable declaration or function call
                match &new_var {
                    AstNode::VarDeclaration(_, expr, _, data_type, ..) => {
                        let arg = Arg {
                            name: name.to_owned(),
                            data_type: data_type.to_owned(),
                            expr: expr.to_owned(),
                        };

                        declarations.push(arg.to_owned());

                        if is_exported == VarVisibility::Public {
                            exports.push(arg)
                        }
                    }

                    // Chill
                    AstNode::FunctionCall(..) => {}

                    _ => {
                        return Err(CompileError {
                            msg: format!(
                                "Expected variable, function declaration, or function call. Found {:?}",
                                new_var
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

                ast.push(new_var);
            }

            // Control Flow
            Token::For => {
                x.index += 1;

                // Create expressions checks what the condition for the loop is
                // If it encounters an 'in' keyword, the type becomes a Range
                // If it encounters a boolean expression, it comes a while loop
                let mut data_type = DataType::Inferred(false);
                let item = create_expression(
                    x,
                    &mut data_type, // For figuring out the type of loop
                    false,
                    &mut declarations,
                )?;

                match data_type {
                    // For loop (iterator)
                    DataType::Range => {
                        let collection =
                            create_expression(x, &mut DataType::Range, false, &mut declarations)?;

                        if x.current_token() != &Token::Colon {
                            return Err(CompileError {
                                msg: format!(
                                    "Expected ':' after for loop condition, found {:?}",
                                    x.current_token()
                                ),
                                start_pos: x.current_position(),
                                end_pos: x.current_position(),
                                error_type: ErrorType::Syntax,
                            });
                        }

                        x.index += 1; // Consume ':'

                        ast.push(AstNode::ForLoop(
                            item, // Item name
                            collection,
                            new_ast(x, &declarations, returns)?,
                            start_pos,
                        ))
                    }

                    // While loop
                    DataType::Bool(_) => {
                        if x.current_token() != &Token::Colon {
                            return Err(CompileError {
                                msg: format!(
                                    "Expected ':' after for loop condition, found {:?}",
                                    x.current_token()
                                ),
                                start_pos: x.current_position(),
                                end_pos: x.current_position(),
                                error_type: ErrorType::Syntax,
                            });
                        }

                        x.index += 1; // Consume ':'

                        ast.push(AstNode::WhileLoop(
                            item, // Condition
                            new_ast(x, &declarations, returns)?,
                            start_pos,
                        ))
                    }

                    _ => {
                        return Err(CompileError {
                            msg: format!(
                                "Expected 'in' keyword or condition for the loop, found {:?}",
                                item
                            ),
                            start_pos: x.current_position(),
                            end_pos: x.current_position(),
                            error_type: ErrorType::Syntax,
                        });
                    }
                }
            }

            Token::If => {
                x.index += 1;
                let condition =
                    create_expression(x, &mut DataType::Bool(false), false, &declarations)?;

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
                        start_pos: x.current_position(),
                        end_pos: x.current_position(),
                        error_type: ErrorType::Syntax,
                    });
                }

                x.index += 1; // Consume ':'

                ast.push(AstNode::If(
                    condition,
                    // This
                    new_ast(x, &declarations, returns)?,
                    start_pos,
                ))
            }

            Token::JS(value) => {
                ast.push(AstNode::JS(value.clone(), x.current_position()));
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
                x.index += 1;

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
                    x.current_position(),
                ));
            }

            Token::Return => {
                x.index += 1;

                let return_values =
                    create_multiple_expressions(x, returns, &declarations)?;

                // if !return_value.is_pure() {
                //     *pure = false;
                // }

                ast.push(AstNode::Return(return_values, x.current_position()));
                x.index -= 1;
            }

            Token::EOF => {
                break;
            }

            // TOKEN::End SHOULD NEVER BE IN MODULE SCOPE
            Token::End => {
                x.index += 1;
                break;
            }

            Token::Settings => {
                let config =
                    create_new_var_or_ref(x, "settings", &declarations, &VarVisibility::Public)?;

                let config = match config {
                    AstNode::VarDeclaration(_, Expr::Block(_, _, return_datatypes), ..) => {
                        get_arguments_from_datatypes(return_datatypes)
                    }
                    _ => {
                        return Err(CompileError {
                            msg: format!(
                                "Settings must be assigned with a struct literal. Found {:?}",
                                config
                            ),
                            start_pos: x.current_position(),
                            end_pos: TokenPosition {
                                line_number: x.current_position().line_number,
                                char_column: x.current_position().char_column + 1,
                            },
                            error_type: ErrorType::Compiler,
                        });
                    }
                };

                ast.push(AstNode::Settings(config, x.current_position()));
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

    Ok(Expr::Block(
        Vec::from(captured_declarations),
        ast,
        returns.to_owned(),
    ))
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

        x.index += 1;
    }
}
