use std::collections::HashMap;
use super::tokens::{Token, TokenizeMode};
use crate::bs_types::DataType;
use crate::parsers::build_ast::TokenContext;
use crate::{CompileError, ErrorType};
use std::iter::Peekable;
use std::path::PathBuf;
use std::str::Chars;
use colour::red_ln;
use crate::parsers::codeblock::tokenize_codeblock;

// Line number, how many chars in the line
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TokenPosition {
    pub line_number: u32,
    pub char_column: u32,
}

pub fn tokenize(source_code: &str, module_path: &PathBuf) -> Result<(TokenContext, HashMap<String, DataType>), CompileError> {
    
    // About 1/6 of the source code seems to be tokens roughly from some very small preliminary tests
    let initial_capacity = source_code.len() / 5;
    
    let mut tokens: Vec<Token> = Vec::with_capacity(initial_capacity);
    let mut exports: HashMap<String, DataType> = HashMap::new();
    let mut line_number: u32 = 0;

    // Is zero because get_next_token will increment it at the start
    // Only ModuleStart will have a char column of zero
    let mut char_column: u32 = 1;

    let mut token_positions: Vec<TokenPosition> = Vec::with_capacity(initial_capacity);
    let mut chars: Peekable<Chars<'_>> = source_code.chars().peekable();
    let mut tokenize_mode: TokenizeMode = TokenizeMode::Normal;
    let scene_nesting_level: &mut i64 = &mut 0;

    // This is pointless atm
    let mut token: Token = Token::ModuleStart(module_path.to_owned());

    loop {
        if token == Token::EOF {
            break;
        }
        
        tokens.push(token);
        token_positions.push(TokenPosition {
            line_number,
            char_column,
        });
        token = get_next_token(
            &mut chars,
            &mut tokenize_mode,
            scene_nesting_level,
            &mut line_number,
            &mut char_column,
            &mut exports,
            module_path
        )?;
    }
    
    tokens.push(token);
    token_positions.push(TokenPosition {
        line_number,
        char_column,
    });

    debug_assert_eq!(
        tokens.len(),
        token_positions.len(),
        "Compiler Bug: Tokens and line numbers not the same length"
    );
    
    // First creation of TokenContext
    Ok((TokenContext {
        length: tokens.len(),
        tokens,
        index: 0,
        token_positions,
    }, exports))
}

pub fn get_next_token(
    chars: &mut Peekable<Chars>,
    tokenize_mode: &mut TokenizeMode,
    scene_nesting_level: &mut i64,
    line_number: &mut u32,
    char_column: &mut u32,
    mut exports: &mut HashMap<String, DataType>,
    module_path: &PathBuf,
) -> Result<Token, CompileError> {
    let mut current_char = match chars.next() {
        Some(ch) => {
            *char_column += 1;
            ch
        }
        None => return Ok(Token::EOF),
    };

    let mut token_value: String = String::new();

    // Check for raw strings (backticks)
    // Also used in scenes for raw outputs
    if current_char == '`' {
        for ch in chars.by_ref() {
            *char_column += 1;

            if ch == '`' {
                return Ok(Token::RawStringLiteral(token_value));
            }
            token_value.push(ch);
        }
    }

    if tokenize_mode == &TokenizeMode::SceneBody && current_char != ']' && current_char != '[' {
        return tokenize_scenebody(current_char, chars, line_number, char_column);
    }

    // Whitespace
    if current_char == '\n' {
        *line_number += 1;
        *char_column = 1;
        return Ok(Token::Newline);
    }

    while current_char.is_whitespace() {
        current_char = match chars.next() {
            Some(ch) => {
                *char_column += 1;
                ch
            }
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
                        line_number: *line_number,
                        char_column: *char_column
                    },
                    end_pos: TokenPosition {
                        line_number: *line_number,
                        char_column: *char_column + 1
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
                    *char_column += 1;

                    let mut spaces_after_scene = 0;

                    while let Some(ch) = chars.peek() {
                        if !ch.is_whitespace() {
                            break;
                        }

                        if ch == &'\n' {
                            *line_number += 1;
                            *char_column = 1;
                        }

                        spaces_after_scene += 1;

                        chars.next();
                        *char_column += 1;
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

    // Check if going into scene body
    if current_char == ':' {
        
        if tokenize_mode  == &TokenizeMode::Codeblock {
            chars.next();
            *char_column += 1;
            
            let parsed_codeblock = tokenize_codeblock(chars, line_number, char_column);
            let codeblock_dimensions = parsed_codeblock.dimensions();
            *char_column += codeblock_dimensions.char_column;
            *line_number += codeblock_dimensions.line_number;
            
            return Ok(parsed_codeblock);
        }
        
        if tokenize_mode == &TokenizeMode::SceneHead {
            *tokenize_mode = TokenizeMode::SceneBody;
        }

        return Ok(Token::Colon);
    }

    //Window
    if current_char == '#' {
        *tokenize_mode = TokenizeMode::CompilerDirective;

        //Get compiler directive token
        return keyword_or_variable(
            &mut token_value,
            chars,
            tokenize_mode,
            line_number,
            char_column,
            false,
        );
    }

    // Check for string literals
    if current_char == '"' {
        return tokenize_string(chars, line_number, char_column);
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
                line_number: *line_number,
                char_column: *char_column,
            },
            end_pos: TokenPosition {
                line_number: *line_number,
                char_column: *char_column + 1,
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

    if current_char == '~' {
        // Check if this is a datatype literal


        return Ok(Token::Mutable);
    }

    if current_char == ',' {
        return Ok(Token::Comma);
    }

    if current_char == '.' {
        return Ok(Token::Dot);
    }

    if current_char == '$' {
        return Ok(Token::This(token_value));
    }

    // Collections
    if current_char == '{' {
        return Ok(Token::OpenCurly);
    }

    if current_char == '}' {
        return Ok(Token::CloseCurly);
    }

    //Error handling
    if current_char == '!' {
        return Ok(Token::Bang);
    }

    if current_char == '?' {
        return Ok(Token::QuestionMark);
    }

    if current_char == ';' {
        return Ok(Token::Semicolon);
    }

    // Comments / Subtraction / Negative / Scene Head / Arrow
    if current_char == '-' {
        if let Some(&next_char) = chars.peek() {
            // Comments
            if next_char == '-' {
                chars.next();
                *char_column += 1;

                // Check for multiline
                if let Some(&next_next_char) = chars.peek() {
                    if next_next_char == '-' {
                        // Multiline Comment (---)
                        chars.next();
                        *char_column += 1;

                        // Multiline Comment
                        for ch in chars.by_ref() {
                            *char_column += 1;
                            token_value.push(ch);

                            if ch == '\n' {
                                *char_column = 1;
                                *line_number += 1;
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
                        *char_column += 1;

                        if ch == '\n' {
                            *line_number += 1;
                            *char_column = 1;
                            return Ok(Token::Comment(token_value));
                        }

                        token_value.push(ch);
                    }
                }
            // Subtraction / Negative / Return / Subtract Assign
            } else {
                if next_char == '=' {
                    chars.next();
                    *char_column += 1;

                    return Ok(Token::SubtractAssign);
                }

                if next_char == '>' {
                    chars.next();
                    *char_column += 1;

                    return Ok(Token::Arrow);
                }

                if next_char.is_numeric() {
                    return Ok(Token::Negative);
                }
                return Ok(Token::Subtract);
            }
        }
    }

    // Mathematical operators,
    // must peak ahead to check for exponentiation (**) or roots (//) and assign variations
    if current_char == '+' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                *char_column += 1;

                return Ok(Token::AddAssign);
            }
        }
        return Ok(Token::Add);
    }

    if current_char == '*' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                *char_column += 1;

                return Ok(Token::MultiplyAssign);
            }
            return Ok(Token::Multiply);
        }
    }

    if current_char == '/' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '/' {
                chars.next();
                *char_column += 1;

                if let Some(&next_next_char) = chars.peek() {
                    if next_next_char == '=' {
                        chars.next();
                        *char_column += 1;

                        return Ok(Token::RootAssign);
                    }
                }
                return Ok(Token::Root);
            }
            if next_char == '=' {
                chars.next();
                *char_column += 1;

                return Ok(Token::DivideAssign);
            }
            return Ok(Token::Divide);
        }
    }

    if current_char == '%' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                *char_column += 1;

                return Ok(Token::ModulusAssign);
            }
            if next_char == '%' {
                chars.next();
                *char_column += 1;

                if let Some(&next_next_char) = chars.peek() {
                    if next_next_char == '=' {
                        chars.next();
                        *char_column += 1;

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
                *char_column += 1;

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
                *char_column += 1;

                return Ok(Token::GreaterThanOrEqual);
            }
            return Ok(Token::GreaterThan);
        }
    }

    if current_char == '<' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                *char_column += 1;

                return Ok(Token::LessThanOrEqual);
            }
            return Ok(Token::LessThan);
        }
    }

    // Exporting variables out of the module or scope (public declaration)
    // When used in a scene head, it's an ID for that scene
    if current_char == '@' {
        if tokenize_mode == &TokenizeMode::SceneHead {
            while let Some(&next_char) = chars.peek() {
                if next_char.is_alphanumeric() || next_char == '_' {
                    token_value.push(chars.next().unwrap());
                    *char_column += 1;
                    continue;
                }
                break;
            }
            return Ok(Token::Id(token_value));
        }
        
        *char_column += 1;
        chars.next();

        exports.insert(
            format!("{}:{}", module_path.to_string_lossy(), token_value),
            DataType::Inferred(false)
        );
        
        return keyword_or_variable(
            &mut token_value,
            chars,
            tokenize_mode,
            line_number,
            char_column,
            true,
        )
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
                            line_number: *line_number,
                            char_column: *char_column,
                        },
                        end_pos: TokenPosition {
                            line_number: *line_number,
                            char_column: *char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }
                token_value.push(chars.next().unwrap());
                *char_column += 1;
                continue;
            }

            if next_char.is_numeric() {
                token_value.push(chars.next().unwrap());
                *char_column += 1;
            } else {
                break;
            }
        }

        if dot_count == 0 {
            return Ok(Token::IntLiteral(token_value.parse::<i32>().unwrap()));
        }
        return Ok(Token::FloatLiteral(token_value.parse::<f64>().unwrap()));
    }

    if current_char.is_alphabetic() {
        token_value.push(current_char);
        return keyword_or_variable(
            &mut token_value,
            chars,
            tokenize_mode,
            line_number,
            char_column,
            false,
        );
    }

    Err(CompileError {
        msg: format!(
            "Invalid Token Used (tokenizer). Token: '{}'. Tokenizer mode: {:?}",
            current_char, tokenize_mode
        ),
        start_pos: TokenPosition {
            line_number: *line_number,
            char_column: *char_column,
        },
        end_pos: TokenPosition {
            line_number: *line_number,
            char_column: *char_column + 1,
        },
        error_type: ErrorType::Syntax,
    })
}

// Nested function because may need multiple searches for variables
const END_KEYWORD: &str = "zz";
fn keyword_or_variable(
    token_value: &mut String,
    chars: &mut Peekable<Chars<'_>>,
    tokenize_mode: &mut TokenizeMode,
    line_number: &mut u32,
    char_column: &mut u32,
    is_exported: bool,
) -> Result<Token, CompileError> {
    // Match variables or keywords
    loop {
        let is_not_eof = match chars.peek() {
            // If there is a char that is not None
            // And is an underscore or alphabetic, add it to the token value
            Some(char) => {
                if char.is_alphanumeric() || *char == '_' {
                    token_value.push(chars.next().unwrap());
                    *char_column += 1;
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
                return Ok(Token::Import);
            },

            // Control Flow
            END_KEYWORD => return Ok(Token::End),
            "return" => return Ok(Token::Return),
            "if" => return Ok(Token::If),
            "else" => return Ok(Token::Else),
            "for" => return Ok(Token::For),
            "from" => return Ok(Token::From),
            "break" => return Ok(Token::Break),
            "defer" => return Ok(Token::Defer),
            "in" => return Ok(Token::In),
            "as" => return Ok(Token::As),
            "copy" => return Ok(Token::Copy),

            "fn" => return Ok(Token::FunctionKeyword),
            "async" => return Ok(Token::AsyncFunctionKeyword),

            // Logical
            "is" => return Ok(Token::Equal),
            "not" => return Ok(Token::Not),
            "and" => return Ok(Token::And),
            "or" => return Ok(Token::Or),

            // Data Types
            "true" | "True" => return Ok(Token::BoolLiteral(true)),
            "false" | "False" => return Ok(Token::BoolLiteral(false)),
            
            "Float" => return Ok(Token::DatatypeLiteral(DataType::Float(false))),
            "~Float" => return Ok(Token::DatatypeLiteral(DataType::Float(true))),
            "Int" => return Ok(Token::DatatypeLiteral(DataType::Int(false))),
            "~Int" => return Ok(Token::DatatypeLiteral(DataType::Int(true))),
            "String" => return Ok(Token::DatatypeLiteral(DataType::String(false))),
            "~String" => return Ok(Token::DatatypeLiteral(DataType::String(true))),
            "Bool" => return Ok(Token::DatatypeLiteral(DataType::Bool(false))),
            "~Bool" => return Ok(Token::DatatypeLiteral(DataType::Bool(true))),
            
            "Type" => return Ok(Token::DatatypeLiteral(DataType::Type)),
            "None" => return Ok(Token::DatatypeLiteral(DataType::None)),
            "Function" => {
                return Ok(Token::DatatypeLiteral(DataType::Function(
                    Vec::new(),
                    Box::new(DataType::None),
                )))
            }
            
            "Scene" => return Ok(Token::DatatypeLiteral(DataType::Scene(false))),
            "~Scene" => return Ok(Token::DatatypeLiteral(DataType::Scene(true))),

            // Built in standard library functions
            "print" => return Ok(Token::Print),
            "log" => return Ok(Token::Log),
            "assert" => return Ok(Token::Assert),
            "panic" => return Ok(Token::Panic),

            "io" => return Ok(Token::IO),

            _ => {}
        }
        
        if tokenize_mode == &TokenizeMode::CompilerDirective {
            match token_value.as_str() {
                "settings" => {
                    *tokenize_mode = TokenizeMode::Normal;
                    return Ok(Token::Settings);
                }
                "title" => {
                    *tokenize_mode = TokenizeMode::Normal;
                    return Ok(Token::Title);
                }
                "date" => {
                    *tokenize_mode = TokenizeMode::Normal;
                    return Ok(Token::Date);
                }
                "JS" => {
                    *tokenize_mode = TokenizeMode::Normal;
                    return Ok(Token::JS(string_block(chars, line_number, char_column)?));
                }
                "WASM" => {
                    *tokenize_mode = TokenizeMode::Normal;
                    return Ok(Token::WASM(string_block(chars, line_number, char_column)?));
                }
                "CSS" => {
                    *tokenize_mode = TokenizeMode::Normal;
                    return Ok(Token::CSS(string_block(chars, line_number, char_column)?));
                }
                _ => {}
            }
        }
        
        // VARIABLE
        if is_not_eof && is_valid_identifier(token_value) {
            return Ok(Token::Variable(token_value.to_string(), is_exported));
        } else {
            break;
        }
    }

    // Failing all of that, this is an error
    Err(CompileError {
        msg: format!("Invalid variable name: {}", token_value),
        start_pos: TokenPosition {
            line_number: *line_number,
            char_column: *char_column,
        },
        end_pos: TokenPosition {
            line_number: *line_number,
            char_column: *char_column + token_value.len() as u32,
        },
        error_type: ErrorType::Syntax,
    })
}

// Checking if the variable name is valid
fn is_valid_identifier(s: &str) -> bool {
    // Check if the string is a valid identifier (variable name)
    s.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

// A block that starts with : and ends with the 'fin' keyword
// Everything inbetween is returned as a string
// Throws an error if there is no starting colon or ending 'fin' keyword
fn string_block(
    chars: &mut Peekable<Chars>,
    line_number: &u32,
    char_column: &mut u32,
) -> Result<String, CompileError> {
    let mut string_value = String::new();

    while let Some(ch) = chars.peek() {
        // Skip whitespace before the first colon that starts the block
        if ch.is_whitespace() {
            chars.next();
            *char_column += 1;

            continue;
        }

        // Start the code block at the colon
        if *ch != ':' {
            return Err(CompileError {
                msg: "Block must start with a colon".to_string(),
                start_pos: TokenPosition {
                    line_number: *line_number,
                    char_column: *char_column,
                },
                end_pos: TokenPosition {
                    line_number: *line_number,
                    char_column: *char_column + 1,
                },
                error_type: ErrorType::Syntax,
            });
        } else {
            chars.next();
            *char_column += 1;

            break;
        }
    }

    let mut closing_end_keyword = false;

    loop {
        match chars.peek() {
            Some(char) => {
                string_value.push(*char);

                chars.next();
                *char_column += 1;
            }
            None => {
                if !closing_end_keyword {
                    return Err(CompileError {
                        msg: format!("block must end with '{}' keyword", END_KEYWORD),
                        start_pos: TokenPosition {
                            line_number: *line_number,
                            char_column: *char_column,
                        },
                        end_pos: TokenPosition {
                            line_number: *line_number,
                            char_column: *char_column,
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
    line_number: &mut u32,
    char_column: &mut u32,
) -> Result<Token, CompileError> {
    let mut token_value = String::new();
    
    // Currently should be at the character that started the String
    while let Some(ch) = chars.next() {
        *char_column += 1;

        if ch == '\n' {
            *line_number += 1;
            *char_column = 1;
        }

        // Check for escape characters
        if ch == '\\' {
            if let Some(next_char) = chars.next() {
                *char_column += 1;

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
    line_number: &mut u32,
    char_column: &mut u32,
) -> Result<Token, CompileError> {
    let mut token_value = String::from(current_char);

    // Currently should be at the character that started the String
    while let Some(ch) = chars.peek() {
        if ch == &'\n' {
            *line_number += 1;
            *char_column = 1;
        }

        // Check for escape characters
        if ch == &'\\' {
            chars.next();
            *char_column += 1;

            if let Some(next_char) = chars.next() {
                *char_column += 1;
                token_value.push(next_char);
            }
        } else if ch == &'[' || ch == &']' {
            return Ok(Token::StringLiteral(token_value));
        }

        *char_column += 1;

        // Should always be a valid char
        token_value.push(chars.next().unwrap());
    }

    Ok(Token::StringLiteral(token_value))
}

// Assumes there MUST be an explicit type declaration or will throw an error
fn tokenize_export_datatype(
    chars: &mut Peekable<Chars>,
    line_number: &mut u32,
    char_column: &mut u32,
) -> Result<DataType, CompileError> {
    let mut token_value = String::new();

    // Skip whitespace
    while let Some(&c) = chars.peek() {
        if c == '\n' {
            return Err(CompileError {
                msg: "Cannot have whitespace between the type keyword and the data type".to_string(),
                start_pos: TokenPosition {
                    line_number: *line_number,
                    char_column: *char_column,
                },
                end_pos: TokenPosition {
                    line_number: *line_number,
                    char_column: *char_column + 1,
                },
                error_type: ErrorType::Syntax,
            });
        }

        if c.is_whitespace() {
            chars.next();
            *char_column += 1;
            continue;
        }
        break;
    }
    
    loop {
        match chars.peek() {
            // If there is a char that is not None
            // And is an underscore or alphabetic, add it to the token value
            Some(char) => {
                if char.is_alphanumeric() || char == &'_' || char == &'~' {
                    token_value.push(chars.next().unwrap());
                    *char_column += 1;
                    continue;
                }
            }
            None => {},
        };

        return match token_value.as_str() {
            "Float" => Ok(DataType::Float(false)),
            "~Float" => Ok(DataType::Float(true)),
            "Int" => Ok(DataType::Int(false)),
            "~Int" => Ok(DataType::Int(true)),
            "String" => Ok(DataType::String(false)),
            "~String" => Ok(DataType::String(true)),
            "Bool" => Ok(DataType::Bool(false)),
            "~Bool" => Ok(DataType::Bool(true)),

            "Type" => Ok(DataType::Type),
            "None" => Ok(DataType::None),
            "Function" => {
                Ok(DataType::Function(
                    Vec::new(),
                    Box::new(DataType::None),
                ))
            }

            "Scene" => Ok(DataType::Scene(false)),
            "~Scene" => Ok(DataType::Scene(true)),

            _ => {
                Err(CompileError {
                    msg: format!("Unknown datatype keyword: '{}'. Expected a valid datatype keyword here. Exports must have an explicit datatype set", token_value),
                    start_pos: TokenPosition {
                        line_number: *line_number,
                        char_column: *char_column,
                    },
                    end_pos: TokenPosition {
                        line_number: *line_number,
                        char_column: *char_column + token_value.len() as u32,
                    },
                    error_type: ErrorType::Rule,
                })
            }
        }
    }
}