#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};

use super::tokens::{Token, TokenizeMode, VarVisibility};
use crate::bs_types::DataType;
use crate::parsers::build_ast::TokenContext;
use crate::{CompileError, ErrorType};
use std::iter::Peekable;
use std::str::Chars;

// Line number, how many chars in the line
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TokenPosition {
    pub line_number: i32,
    pub char_column: i32,
}

pub struct TokenizerOutput {
    pub token_context: TokenContext,
    pub imports: Vec<String>,
    pub exports: Vec<Token>,
}
pub fn tokenize(source_code: &str, module_name: &str) -> Result<TokenizerOutput, CompileError> {
    // About 1/6 of the source code seems to be tokens roughly from some very small preliminary tests
    let initial_capacity = source_code.len() / 5;

    let mut tokens: Vec<Token> = Vec::with_capacity(initial_capacity);

    let mut exports: Vec<Token> = Vec::new();
    let mut imports: Vec<String> = Vec::new();

    let mut token_position: TokenPosition = TokenPosition::default();

    let mut token_positions: Vec<TokenPosition> = Vec::with_capacity(initial_capacity);
    let mut chars: Peekable<Chars<'_>> = source_code.chars().peekable();
    let mut tokenize_mode: TokenizeMode = TokenizeMode::Normal;
    let scene_nesting_level: &mut i64 = &mut 0;

    let mut token: Token = Token::ModuleStart(module_name.to_owned());

    loop {
        if token == Token::EOF {
            break;
        }

        // dark_cyan_ln!("Token: {:?}. Mode: {:?}", token, tokenize_mode);
        tokens.push(token);
        token_positions.push(token_position.clone());

        token = get_next_token(
            &mut chars,
            &mut tokenize_mode,
            scene_nesting_level,
            &mut token_position,
            &mut exports,
            &mut imports,
            tokens.last().unwrap(),
        )?;
    }

    tokens.push(token);
    token_positions.push(token_position);

    debug_assert_eq!(
        tokens.len(),
        token_positions.len(),
        "Compiler Bug: Tokens and line numbers not the same length"
    );

    // First creation of TokenContext
    Ok(TokenizerOutput {
        token_context: TokenContext {
            length: tokens.len(),
            tokens,
            index: 0,
            token_positions,
        },
        imports,
        exports,
    })
}

pub fn get_next_token(
    chars: &mut Peekable<Chars>,
    tokenize_mode: &mut TokenizeMode,
    scene_nesting_level: &mut i64,
    token_position: &mut TokenPosition,
    exports: &mut Vec<Token>,
    imports: &mut Vec<String>,
    previous_token: &Token,
) -> Result<Token, CompileError> {
    let mut current_char = match chars.next() {
        Some(ch) => {
            token_position.char_column += 1;
            ch
        }
        None => return Ok(Token::EOF),
    };

    let mut token_value: String = String::new();

    // Check for raw strings (backticks)
    // Also used in scenes for raw outputs
    if current_char == '`' {
        for ch in chars.by_ref() {
            token_position.char_column += 1;

            if ch == '`' {
                return Ok(Token::RawStringLiteral(token_value));
            }
            token_value.push(ch);
        }
    }

    if tokenize_mode == &TokenizeMode::SceneBody && current_char != ']' && current_char != '[' {
        return tokenize_scenebody(current_char, chars, token_position);
    }

    // Whitespace
    if current_char == '\n' {
        token_position.line_number += 1;
        token_position.char_column = 0;
        return Ok(Token::Newline);
    }

    while current_char.is_whitespace() {
        token_position.char_column += 1;
        current_char = match chars.next() {
            Some(ch) => ch,
            None => return Ok(Token::EOF),
        };
    }

    if current_char == '[' {
        *scene_nesting_level += 1;
        return match tokenize_mode {
            TokenizeMode::SceneHead => {
                return Err(CompileError {
                    msg: "Cannot have nested scenes inside of a scene head, must be inside the scene body. Use a colon to start the scene body.".to_string(),
                    start_pos: TokenPosition {
                        line_number: token_position.line_number,
                        char_column: token_position.char_column
                    },
                    end_pos: TokenPosition {
                        line_number: token_position.line_number,
                        char_column: token_position.char_column + 1
                    },
                    error_type: ErrorType::Syntax,
                });
            }

            TokenizeMode::Normal => {
                *tokenize_mode = TokenizeMode::SceneHead;
                Ok(Token::ParentScene)
            }

            // Going into the scene head
            _ => {
                // [] is an empty scene
                if chars.peek() == Some(&']') {
                    chars.next();
                    token_position.char_column += 1;

                    let mut spaces_after_scene = 0;

                    while let Some(ch) = chars.peek() {
                        if !ch.is_whitespace() {
                            break;
                        }

                        if ch == &'\n' {
                            token_position.line_number += 1;
                            token_position.char_column = 0;
                        }

                        spaces_after_scene += 1;

                        chars.next();
                        token_position.char_column += 1;
                    }

                    return Ok(Token::EmptyScene(spaces_after_scene));
                }

                *tokenize_mode = TokenizeMode::SceneHead;
                Ok(Token::SceneHead)
            }
        };
    }

    if current_char == ']' {
        *scene_nesting_level -= 1;

        if *scene_nesting_level == 0 {
            *tokenize_mode = TokenizeMode::Normal;
        } else {
            *tokenize_mode = TokenizeMode::SceneBody;
        }

        return Ok(Token::SceneClose);
    }

    // Check if going into the scene body
    if current_char == ':' {
        if tokenize_mode == &TokenizeMode::SceneHead {
            *tokenize_mode = TokenizeMode::SceneBody;
            return Ok(Token::Colon);
        }

        // ::
        if let Some(&next_char) = chars.peek() {
            if next_char == ':' {
                chars.next();
                token_position.char_column += 1;
                return Ok(Token::Private);
            }
        }

        return Ok(Token::Colon);
    }

    // Compiler Directives
    if current_char == '#' {
        return compiler_directive(&mut token_value, chars, token_position);
    }

    // Check for string literals
    if current_char == '"' {
        return tokenize_string(chars, token_position);
    }

    // Check for character literals
    if current_char == '\'' {
        if let Some(c) = chars.next() {
            if let Some(&char_after_next) = chars.peek() {
                if char_after_next == '\'' {
                    return Ok(Token::CharLiteral(c));
                }
            }
        };

        // If not correct declaration of char
        return Err(CompileError {
            msg: format!(
                "Unexpected character '{}' during char declaration",
                current_char
            ),
            start_pos: TokenPosition {
                line_number: token_position.line_number,
                char_column: token_position.char_column,
            },
            end_pos: TokenPosition {
                line_number: token_position.line_number,
                char_column: token_position.char_column + 1,
            },
            error_type: ErrorType::Syntax,
        });
    }

    // Functions and grouping expressions
    if current_char == '(' {
        return Ok(Token::OpenParenthesis);
    }

    if current_char == ')' {
        return Ok(Token::CloseParenthesis);
    }

    // Context Free Grammars
    if current_char == '=' {
        return Ok(Token::Assign);
    }

    if current_char == ',' {
        return Ok(Token::Comma);
    }

    if current_char == '.' {
        return Ok(Token::Dot);
    }

    // Collections
    if current_char == '{' {
        return Ok(Token::OpenCurly);
    }
    if current_char == '}' {
        return Ok(Token::CloseCurly);
    }

    // Structs
    if current_char == '|' {
        return Ok(Token::ArgConstructor);
    }

    // Currently not using bangs
    if current_char == '!' {
        return Ok(Token::Bang);
    }

    // Option type
    if current_char == '?' {
        return Ok(Token::QuestionMark);
    }

    // Comments / Subtraction / Negative / Scene Head / Arrow
    if current_char == '-' {
        if let Some(&next_char) = chars.peek() {
            // Comments
            if next_char == '-' {
                chars.next();
                token_position.char_column += 1;

                // Check for multiline
                if let Some(&next_next_char) = chars.peek() {
                    if next_next_char == '-' {
                        // Multiline Comment (---)
                        chars.next();
                        token_position.char_column += 1;

                        // Multiline Comment
                        for ch in chars.by_ref() {
                            token_position.char_column += 1;
                            token_value.push(ch);

                            if ch == '\n' {
                                token_position.char_column = 0;
                                token_position.line_number += 1;
                            }

                            if token_value.ends_with("---") {
                                return Ok(Token::Comment(
                                    token_value.trim_end_matches("---").to_string(),
                                ));
                            }
                        }
                    }

                    // Inline Comment
                    for ch in chars.by_ref() {
                        token_position.char_column += 1;

                        if ch == '\n' {
                            token_position.line_number += 1;
                            token_position.char_column = 0;
                            return Ok(Token::Comment(token_value));
                        }

                        token_value.push(ch);
                    }
                }
            // Subtraction / Negative / Return / Subtract Assign
            } else {
                if next_char == '=' {
                    chars.next();
                    token_position.char_column += 1;

                    return Ok(Token::SubtractAssign);
                }

                if next_char == '>' {
                    chars.next();
                    token_position.char_column += 1;

                    return Ok(Token::Arrow);
                }

                if next_char.is_numeric() {
                    return Ok(Token::Negative);
                }
                return Ok(Token::Subtract);
            }
        }
    }

    // Mathematical operators
    // must peak ahead to check for exponentiation (**) or roots (//) and assign variations
    if current_char == '+' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                token_position.char_column += 1;

                return Ok(Token::AddAssign);
            }
        }
        return Ok(Token::Add);
    }

    if current_char == '*' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                token_position.char_column += 1;

                return Ok(Token::MultiplyAssign);
            }
            return Ok(Token::Multiply);
        }
    }

    if current_char == '/' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '/' {
                chars.next();
                token_position.char_column += 1;

                if let Some(&next_next_char) = chars.peek() {
                    if next_next_char == '=' {
                        chars.next();
                        token_position.char_column += 1;

                        return Ok(Token::RootAssign);
                    }
                }
                return Ok(Token::Root);
            }
            if next_char == '=' {
                chars.next();
                token_position.char_column += 1;

                return Ok(Token::DivideAssign);
            }
            return Ok(Token::Divide);
        }
    }

    if current_char == '%' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                token_position.char_column += 1;

                return Ok(Token::ModulusAssign);
            }
            if next_char == '%' {
                chars.next();
                token_position.char_column += 1;

                if let Some(&next_next_char) = chars.peek() {
                    if next_next_char == '=' {
                        chars.next();
                        token_position.char_column += 1;

                        return Ok(Token::RemainderAssign);
                    }
                }
                return Ok(Token::Remainder);
            }
            return Ok(Token::Modulus);
        }
    }

    if current_char == '^' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                token_position.char_column += 1;

                return Ok(Token::ExponentAssign);
            }
        }
        return Ok(Token::Exponent);
    }

    // Check for greater than and Less than logic operators
    // must also peak ahead to check it's not also equal to
    if current_char == '>' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                token_position.char_column += 1;

                return Ok(Token::GreaterThanOrEqual);
            }
            return Ok(Token::GreaterThan);
        }
    }

    if current_char == '<' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                token_position.char_column += 1;

                return Ok(Token::LessThanOrEqual);
            }
            return Ok(Token::LessThan);
        }
    }

    if current_char == '~' {
        return Ok(Token::Mutable);
    }

    // Exporting variables out of the module or scope (public declaration)
    // When used in a scene head, it's an ID for that scene
    if current_char == '@' {
        if tokenize_mode == &TokenizeMode::SceneHead {
            while let Some(&next_char) = chars.peek() {
                if next_char.is_alphanumeric() || next_char == '_' {
                    token_value.push(chars.next().unwrap());
                    token_position.char_column += 1;
                    continue;
                }
                break;
            }
            return Ok(Token::Id(token_value));
        }

        return Ok(Token::Public);
    }

    // Numbers
    if current_char.is_numeric() {
        token_value.push(current_char);
        let mut dot_count = 0;

        while let Some(&next_char) = chars.peek() {
            if next_char == '_' {
                chars.next();
                continue;
            }

            if next_char == '.' {
                dot_count += 1;
                // Stop if too many dots
                if dot_count > 1 {
                    return Err(CompileError {
                        msg: "Cannot have more than one decimal point in a number".to_string(),
                        start_pos: TokenPosition {
                            line_number: token_position.line_number,
                            char_column: token_position.char_column,
                        },
                        end_pos: TokenPosition {
                            line_number: token_position.line_number,
                            char_column: token_position.char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }
                token_value.push(chars.next().unwrap());
                token_position.char_column += 1;
                continue;
            }

            if next_char.is_numeric() {
                token_value.push(chars.next().unwrap());
                token_position.char_column += 1;
            } else {
                break;
            }
        }

        if dot_count == 0 {
            return Ok(Token::IntLiteral(token_value.parse::<i32>().unwrap()));
        }
        return Ok(Token::FloatLiteral(token_value.parse::<f64>().unwrap()));
    }

    // Currently unused
    if current_char == ';' {
        return Ok(Token::Semicolon);
    }

    if current_char.is_alphabetic() {
        token_value.push(current_char);
        return keyword_or_variable(
            &mut token_value,
            chars,
            token_position,
            imports,
            exports,
            previous_token,
        );
    }

    Err(CompileError {
        msg: format!(
            "Invalid Token Used (tokenizer). Token: '{}'. Tokenizer mode: {:?}",
            current_char, tokenize_mode
        ),
        start_pos: TokenPosition {
            line_number: token_position.line_number,
            char_column: token_position.char_column,
        },
        end_pos: TokenPosition {
            line_number: token_position.line_number,
            char_column: token_position.char_column + 1,
        },
        error_type: ErrorType::Syntax,
    })
}

// Nested function because may need multiple searches for variables
const END_KEYWORD: &str = "zz";
fn keyword_or_variable(
    token_value: &mut String,
    chars: &mut Peekable<Chars<'_>>,
    token_position: &mut TokenPosition,
    imports: &mut Vec<String>,
    exports: &mut Vec<Token>,
    previous_token: &Token,
) -> Result<Token, CompileError> {
    let mutable_modifier: bool = matches!(previous_token, Token::Mutable);

    // Match variables or keywords
    loop {
        let is_not_eof = match chars.peek() {
            // If there is a char that is not None
            // And is an underscore or alphabetic, add it to the token value
            Some(char) => {
                if char.is_alphanumeric() || *char == '_' {
                    token_value.push(chars.next().unwrap());
                    token_position.char_column += 1;
                    continue;
                }
                true
            }
            None => false,
        };

        // Codeblock tokenizing - removed for now
        // if tokenize_mode == &TokenizeMode::SceneHead && token_value == "Code" {
        //     *tokenize_mode= TokenizeMode::Codeblock;
        //     return Ok(Token::CodeKeyword);
        // }

        // Always check if token value is a keyword in every other case
        // If there's whitespace or some termination
        // First check if there is a match to a keyword
        // Otherwise break out and check it is a valid variable name
        match token_value.as_str() {
            "import" => {
                imports.push(tokenize_import(chars, token_position)?);
                return Ok(Token::Import);
            }

            // Control Flow
            END_KEYWORD => return Ok(Token::End),
            "return" => return Ok(Token::Return),
            "if" => return Ok(Token::If),
            "else" => return Ok(Token::Else),
            "for" => return Ok(Token::For),
            "from" => return Ok(Token::From),
            "break" => return Ok(Token::Break),
            "then" => return Ok(Token::Then),
            "defer" => return Ok(Token::Defer),
            "in" => return Ok(Token::In),
            "as" => return Ok(Token::As),
            "copy" => return Ok(Token::Copy),

            "async" => return Ok(Token::Async),

            // Logical
            "is" => return Ok(Token::Is),
            "not" => return Ok(Token::Not),
            "and" => return Ok(Token::And),
            "or" => return Ok(Token::Or),

            // Data Types
            "true" | "True" => return Ok(Token::BoolLiteral(mutable_modifier)),
            "false" | "False" => return Ok(Token::BoolLiteral(mutable_modifier)),

            "Float" => return Ok(Token::DatatypeLiteral(DataType::Float(mutable_modifier))),
            "Int" => return Ok(Token::DatatypeLiteral(DataType::Int(mutable_modifier))),
            "String" => return Ok(Token::DatatypeLiteral(DataType::String(mutable_modifier))),
            "Bool" => return Ok(Token::DatatypeLiteral(DataType::Bool(mutable_modifier))),

            "None" => return Ok(Token::DatatypeLiteral(DataType::None)),

            // Scene-related keywords
            "Scene" => return Ok(Token::DatatypeLiteral(DataType::Scene(mutable_modifier))),

            _ => {}
        }

        // VARIABLE
        if is_not_eof && is_valid_identifier(token_value) {
            // Check if this declaration has any modifiers in front of it
            let visibility = match previous_token {
                Token::Private => VarVisibility::Private,
                Token::Public => {
                    exports.push(Token::Variable(
                        token_value.to_string(),
                        VarVisibility::Public,
                        mutable_modifier,
                    ));
                    VarVisibility::Public
                }
                _ => VarVisibility::Temporary,
            };

            return Ok(Token::Variable(
                token_value.to_string(),
                visibility,
                mutable_modifier,
            ));
        } else {
            break;
        }
    }

    // Failing all of that, this is an error
    Err(CompileError {
        msg: format!("Invalid variable name: {}", token_value),
        start_pos: TokenPosition {
            line_number: token_position.line_number,
            char_column: token_position.char_column,
        },
        end_pos: TokenPosition {
            line_number: token_position.line_number,
            char_column: token_position.char_column + token_value.len() as i32,
        },
        error_type: ErrorType::Syntax,
    })
}

fn compiler_directive(
    token_value: &mut String,
    chars: &mut Peekable<Chars<'_>>,
    token_position: &mut TokenPosition,
) -> Result<Token, CompileError> {
    loop {
        if chars
            .peek()
            .is_some_and(|c| c.is_alphanumeric() || c == &'_')
        {
            token_value.push(chars.next().unwrap());
            token_position.char_column += 1;
            continue;
        }

        return match token_value.as_str() {
            // Built-in functions
            "print" => Ok(Token::Print),
            "assert" => Ok(Token::Assert),
            "panic" => Ok(Token::Panic),
            "log" => Ok(Token::Log),

            // Compiler settings
            "settings" => Ok(Token::Settings),
            "title" => Ok(Token::Title),
            "date" => Ok(Token::Date),

            // External language blocks
            "JS" => Ok(Token::JS(string_block(chars, token_position)?)),
            "WASM" => Ok(Token::WASM(string_block(chars, token_position)?)),
            "CSS" => Ok(Token::CSS(string_block(chars, token_position)?)),

            // Scene Style properties
            "markdown" => Ok(Token::Markdown),
            "child_default" => Ok(Token::ChildDefault),
            "slot" => Ok(Token::Slot),

            _ => Err(CompileError {
                msg: format!("Invalid compiler directive: #{}", token_value),
                start_pos: TokenPosition {
                    line_number: token_position.line_number,
                    char_column: token_position.char_column,
                },
                end_pos: TokenPosition {
                    line_number: token_position.line_number,
                    char_column: token_position.char_column + token_value.len() as i32,
                },
                error_type: ErrorType::Syntax,
            }),
        };
    }
}

// Checking if the variable name is valid
fn is_valid_identifier(s: &str) -> bool {
    // Check if the string is a valid identifier (variable name)
    s.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

// A block that starts with: and ends with the 'fin' keyword
// Everything inbetween is returned as a string
// Throws an error if there is no starting colon or ending 'fin' keyword
fn string_block(
    chars: &mut Peekable<Chars>,
    token_position: &mut TokenPosition,
) -> Result<String, CompileError> {
    let mut string_value = String::new();

    while let Some(ch) = chars.peek() {
        // Skip whitespace before the first colon that starts the block
        if ch.is_whitespace() {
            if ch == &'\n' {
                token_position.line_number += 1;
                token_position.char_column = 0;
            } else {
                token_position.char_column += 1;
            }

            chars.next();
            continue;
        }

        // Start the code block at the colon
        if *ch != ':' {
            return Err(CompileError {
                msg: "Block must start with a colon".to_string(),
                start_pos: TokenPosition {
                    line_number: token_position.line_number,
                    char_column: token_position.char_column,
                },
                end_pos: TokenPosition {
                    line_number: token_position.line_number,
                    char_column: token_position.char_column + 1,
                },
                error_type: ErrorType::Syntax,
            });
        } else {
            chars.next();
            token_position.char_column += 1;

            break;
        }
    }

    let mut closing_end_keyword = false;

    loop {
        match chars.peek() {
            Some(char) => {
                string_value.push(*char);

                chars.next();
                token_position.char_column += 1;
            }
            None => {
                if !closing_end_keyword {
                    return Err(CompileError {
                        msg: format!("block must end with '{}' keyword", END_KEYWORD),
                        start_pos: TokenPosition {
                            line_number: token_position.line_number,
                            char_column: token_position.char_column,
                        },
                        end_pos: TokenPosition {
                            line_number: token_position.line_number,
                            char_column: token_position.char_column,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }
                break;
            }
        };

        // Push everything to the JS code block until the first 'end' keyword
        // must have newline before and whitespace after the 'end' keyword
        if string_value.ends_with(END_KEYWORD) {
            closing_end_keyword = true;
            string_value = string_value
                .split_at(string_value.len() - END_KEYWORD.len())
                .0
                .to_string();
        }
    }

    Ok(string_value)
}
fn tokenize_string(
    chars: &mut Peekable<Chars>,
    token_position: &mut TokenPosition,
) -> Result<Token, CompileError> {
    let mut token_value = String::new();

    // Currently should be at the character that started the String
    while let Some(ch) = chars.next() {
        token_position.char_column += 1;

        if ch == '\n' {
            token_position.line_number += 1;
            token_position.char_column = 0;
        }

        // Check for escape characters
        if ch == '\\' {
            if let Some(next_char) = chars.next() {
                token_position.char_column += 1;

                token_value.push(next_char);
            }
        } else if ch == '"' {
            return Ok(Token::StringLiteral(token_value));
        }

        token_value.push(ch);
    }

    Ok(Token::StringLiteral(token_value))
}

fn tokenize_scenebody(
    current_char: char,
    chars: &mut Peekable<Chars>,
    token_position: &mut TokenPosition,
) -> Result<Token, CompileError> {
    let mut token_value = String::from(current_char);

    // Currently should be at the character that started the String
    while let Some(ch) = chars.peek() {
        if ch == &'\n' {
            token_position.line_number += 1;
            token_position.char_column = 0;
        }

        // Check for escape characters
        if ch == &'\\' {
            chars.next();
            token_position.char_column += 1;

            if let Some(next_char) = chars.next() {
                token_position.char_column += 1;
                token_value.push(next_char);
            }
        } else if ch == &'[' || ch == &']' {
            return Ok(Token::StringLiteral(token_value));
        }

        token_position.char_column += 1;

        // Should always be a valid char
        token_value.push(chars.next().unwrap());
    }

    Ok(Token::StringLiteral(token_value))
}

fn tokenize_import(
    chars: &mut Peekable<Chars>,
    token_position: &mut TokenPosition,
) -> Result<String, CompileError> {
    // Skip starting whitespace
    while let Some(c) = chars.peek() {
        if c.is_whitespace() {
            if c == &'\n' {
                return Err(CompileError {
                    msg: "Unexpected newline in import statement. Import statements must be on a single line. e.g import path/to/file".to_string(),
                    start_pos: TokenPosition {
                        line_number: token_position.line_number + 1,
                        char_column: token_position.char_column,
                    },
                    end_pos: TokenPosition {
                        line_number: token_position.line_number + 1,
                        char_column: token_position.char_column + 1,
                    },
                    error_type: ErrorType::Syntax,
                });
            }

            token_position.char_column += 1;
            chars.next();
            continue;
        }

        break;
    }

    // Parse the import path
    // This assumes starting the path from the project root directory
    let mut import_path = String::new();
    while let Some(c) = chars.peek() {
        if c.is_whitespace() {
            if c == &'\n' {
                token_position.line_number += 1;
                token_position.char_column = 0;
            }
            break;
        }

        import_path.push(chars.next().unwrap());
        token_position.char_column += 1;
    }

    if import_path.is_empty() {
        return Err(CompileError {
            msg: "Import path cannot be empty".to_string(),
            start_pos: TokenPosition {
                line_number: token_position.line_number,
                char_column: token_position.char_column,
            },
            end_pos: TokenPosition {
                line_number: token_position.line_number,
                char_column: token_position.char_column + 1,
            },
            error_type: ErrorType::Syntax,
        });
    }

    Ok(import_path)
}
