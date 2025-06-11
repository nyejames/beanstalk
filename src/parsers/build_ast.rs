#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};

use super::{
    ast_nodes::AstNode, create_scene_node::new_scene,
    expressions::parse_expression::create_expression,
};
// use crate::html_output::html_styles::get_html_styles;
use crate::parsers::ast_nodes::{Arg, Expr};
use crate::parsers::builtin_methods::get_builtin_methods;
use crate::parsers::expressions::parse_expression::{
    create_args_from_types, create_multiple_expressions,
};
use crate::parsers::functions::parse_function_call;
use crate::parsers::loops::create_loop;
use crate::parsers::scene::{SceneType, Style};
use crate::parsers::variables::{get_reference, mutated_arg, new_arg};
use crate::tokenizer::TokenPosition;
use crate::tokens::VarVisibility;
use crate::{CompileError, ErrorType, Token, bs_types::DataType};
use std::collections::HashMap;

#[derive(Clone)]
pub struct TokenContext {
    pub tokens: Vec<Token>,
    pub index: usize,
    pub length: usize,
    pub token_positions: Vec<TokenPosition>,
}
impl TokenContext {
    pub fn current_token(&self) -> &Token {
        debug_assert!(self.index < self.length, "Token in block {:?} is out of bounds", self.get_block_name());

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
        // Some tokens allow any number of newlines after them,
        // without breaking a statement or expression
        match self.current_token() {
            &Token::Colon |
            &Token::OpenParenthesis |
            &Token::ArgConstructor |
            &Token::Comma |

            &Token::Assign |
            &Token::AddAssign |
            &Token::SubtractAssign |
            &Token::MultiplyAssign |
            &Token::DivideAssign |
            &Token::ExponentAssign |
            &Token::RootAssign |

            &Token::Add |
            &Token::Subtract |
            &Token::Multiply |
            &Token::Divide |
            &Token::Modulus |
            &Token::Root |

            &Token::Arrow |

            &Token::Is |
            &Token::LessThan |
            &Token::LessThanOrEqual |
            &Token::GreaterThan |
            &Token::GreaterThanOrEqual => {
                self.index += 1;
                self.skip_newlines();
            }

            _ => {
                self.index += 1;
            }
        }
    }

    pub fn skip_newlines(&mut self) {
        while self.current_token() == &Token::Newline {
            self.index += 1;
        }
    }

    pub fn go_back(&mut self) {
        self.index -= 1;
    }

    pub fn get_block_name(&self) -> String {
        self.tokens
            .first()
            .unwrap_or(&Token::ModuleStart(String::new()))
            .get_name()
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
    arguments_passed_in: &[Arg],
    returns: &[DataType],
    global_scope: bool,
) -> Result<Expr, CompileError> {
    // About 1/10 of the tokens seem to become AST nodes roughly from some very small preliminary tests
    let mut ast = Vec::with_capacity(x.length / 10);
    let mut new_declarations = arguments_passed_in.to_vec();

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

                if !global_scope {
                    return Err(CompileError {
                        msg: "Scene literals can only be used at the top level of a module. \n
                        This is because they are handled differently by the compiler depending on the type of project".to_string(),
                        start_pos: x.token_start_position(),
                        end_pos: x.token_end_position(),
                        error_type: ErrorType::Rule,
                    });
                }

                let scene =
                    new_scene(x, &new_declarations, &mut HashMap::new(), &mut Style::default())?;

                match scene {
                    SceneType::Scene(expr) => {
                        ast.push(AstNode::Reference(expr, x.token_start_position()));
                    }
                    SceneType::Slot => {
                        return Err(CompileError {
                            msg: "Slot can't be used at the top level of a scene. Slots can only be used inside of other scenes".to_string(),
                            start_pos: x.token_start_position(),
                            end_pos: x.token_end_position(),
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
            Token::Variable(ref name, visibility, mutable) => {
                // There should be no 'mutable' modifier if a variable name is used on the RHS of an expression
                if mutable {
                    return Err( CompileError {
                        msg: "Mutability modifiers can only be used on types or on references on the right hand side of an expression".to_string(),
                        start_pos: x.token_start_position(),
                        end_pos: x.token_end_position(),
                        error_type: ErrorType::Rule,
                    });
                }

                if let Some(arg) = get_reference(name, &new_declarations) {
                    // Then the associated mutation afterwards.
                    // Move past the name
                    x.advance();

                    ast.push(AstNode::Reference(
                        arg.default_value.to_owned(),
                        x.token_start_position(),
                    ));

                    // We will need to keep pushing nodes if there are accesses after method calls
                    while x.current_token() == &Token::Dot {
                        ast.push(AstNode::Access);

                        // Move past the dot
                        x.advance();

                        // Currently, there is no just integer access.
                        // Only properties or methods are accessed on objects and collections
                        // Collections have a .get() method for accessing elements

                        if let Token::Variable(name, ..) = x.current_token().to_owned() {
                            // NAMED ARGUMENT ACCESS
                            let members = match &arg.data_type {
                                DataType::Object(inner_args) => inner_args,
                                DataType::Block(_, returned_args) => {
                                    &create_args_from_types(returned_args)
                                }
                                _ => &get_builtin_methods(&arg.data_type),
                            };

                            if members.is_empty() {
                                return Err(CompileError {
                                    msg: format!(
                                        "{} is of type {} and has no public methods or properties.",
                                        arg.name,
                                        arg.data_type.to_string()
                                    ),
                                    start_pos: x.token_start_position(),
                                    end_pos: x.token_end_position(),
                                    error_type: ErrorType::Rule,
                                });
                            }

                            let access = match members.iter().find(|member| member.name == *name) {
                                Some(access) => access,
                                None => {
                                    return Err(CompileError {
                                        msg: format!(
                                            "Can't find property or method '{}' inside '{}'",
                                            name, arg.name
                                        ),
                                        start_pos: x.token_start_position(),
                                        end_pos: x.token_end_position(),
                                        error_type: ErrorType::Rule,
                                    });
                                }
                            };

                            // Move past the name
                            x.advance();

                            match &access.data_type {
                                DataType::Block(required_arguments, returned_types) => {
                                    ast.push(parse_function_call(
                                        x,
                                        &name,
                                        &new_declarations,
                                        required_arguments,
                                        returned_types,
                                    )?)
                                }

                                // Property access
                                _ => {
                                    ast.push(mutated_arg(x, arg.to_owned(), &new_declarations)?)
                                }
                            }
                        } else {
                            return Err(CompileError {
                                msg: "Expected a name after the dot (accessing a member of the variable such as a method or property)".to_string(),
                                start_pos: x.token_start_position(),
                                end_pos: x.token_end_position(),
                                error_type: ErrorType::Syntax,
                            });
                        }
                    }

                // NEW VARIABLE DECLARATION
                } else {
                    let arg = new_arg(x, name, &new_declarations)?;


                    if visibility == VarVisibility::Public {
                        exports.push(arg.to_owned());
                    }

                    red_ln!("new variable declaration: {}", arg.name);

                    new_declarations.push(arg.to_owned());

                    ast.push(AstNode::Declaration(
                        name.to_owned(),
                        arg.default_value.to_owned(),
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

                ast.push(create_loop(x, returns, &new_declarations)?);
            }

            Token::If => {
                x.advance();

                let condition =
                    create_expression(x, &mut DataType::Bool(false), false, &new_declarations, &[])?;

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
                    new_ast(x, &new_declarations, returns, global_scope)?,
                    start_pos,
                ))
            }

            Token::JS(value) => {
                ast.push(AstNode::JS(value.clone(), x.token_start_position()));
            }

            // IGNORED TOKENS
            Token::Newline | Token::Empty => {
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
                    &new_declarations,
                    &[Arg {
                        name: "".to_string(),
                        data_type: DataType::CoerceToString(false),
                        default_value: Expr::None,
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
                red_ln!("parsing return inside: {}", x.get_block_name());

                if global_scope {
                    return Err(CompileError {
                        msg: "Can't use the return keyword at the top level of a module. Return can only be used inside a block.".to_string(),
                        start_pos: x.token_start_position(),
                        end_pos: x.token_end_position(),
                        error_type: ErrorType::Rule,
                    });
                }

                x.advance();

                let return_values = create_multiple_expressions(x, returns, &new_declarations)?;

                // if !return_value.is_pure() {
                //     *pure = false;
                // }

                ast.push(AstNode::Return(return_values, x.token_start_position()));
                x.index -= 1;
            }

            Token::End | Token::EOF => {
                break;
            }

            Token::Settings => {
                if !global_scope {
                    return Err(CompileError {
                        msg: "Settings can only be created at the top level of a module, #settings can't be changed inside of blocks".to_string(),
                        start_pos: x.token_start_position(),
                        end_pos: x.token_end_position(),
                        error_type: ErrorType::Rule,
                    });
                }

                let config = new_arg(x, "config", &new_declarations)?;

                let config = match config.default_value {
                    Expr::Block(_, _, return_data_types, _) => {
                        create_args_from_types(&return_data_types)
                    }
                    _ => {
                        return Err(CompileError {
                            msg: "Settings must be a block".to_string(),
                            start_pos: x.token_start_position(),
                            end_pos: x.token_end_position(),
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
        new_declarations,
        ast,
        returns.to_owned(),
        exports,
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
