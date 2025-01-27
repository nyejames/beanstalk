use super::tokens::{Token, TokenizeMode};
use crate::bs_types::DataType;
use crate::tokenize_scene::{tokenize_codeblock, tokenize_markdown};
use crate::{CompileError, ErrorType};
use std::iter::Peekable;
use std::str::Chars;

// Line number, how many chars in the line
#[derive(Clone, Debug, PartialEq)]
pub struct TokenPosition {
    pub line_number: u32,
    pub char_column: u32,
}

pub fn tokenize(
    source_code: &str,
    module_name: &str,
) -> Result<(Vec<Token>, Vec<TokenPosition>), CompileError> {
    let mut tokens: Vec<Token> = Vec::new();
    let mut line_number: u32 = 0;

    // Is zero because get_next_token will increment it at the start
    // Only ModuleStart will have a char column of zero
    let mut char_column: u32 = 0;

    let mut token_positions: Vec<TokenPosition> = Vec::new();
    let mut chars: Peekable<Chars<'_>> = source_code.chars().peekable();
    let mut tokenize_mode: TokenizeMode = TokenizeMode::Normal;
    let mut scene_nesting_level: &mut i64 = &mut 0;

    // For variable optimisation
    let mut token: Token = Token::ModuleStart(module_name.to_string());

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
            &mut scene_nesting_level,
            &mut line_number,
            &mut char_column,
        )?;
    }

    // Mark unused variables for removal in AST
    // DISABLED FOR NOW
    // for var_dec in var_names.iter() {
    //     if !var_dec.has_ref && !var_dec.is_exported {
    //         tokens[var_dec.index] = Token::DeadVariable(var_dec.name.to_string());
    //     }
    // }

    tokens.push(token);
    token_positions.push(TokenPosition {
        line_number,
        char_column,
    });

    assert_eq!(
        tokens.len(),
        token_positions.len(),
        "Compiler Bug: Tokens and line numbers not the same length"
    );

    Ok((tokens, token_positions))
}

pub fn get_next_token(
    chars: &mut Peekable<Chars>,
    tokenize_mode: &mut TokenizeMode,
    scene_nesting_level: &mut i64,
    line_number: &mut u32,
    char_column: &mut u32,
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
        while let Some(ch) = chars.next() {
            *char_column += 1;

            if ch == '`' {
                return Ok(Token::RawStringLiteral(token_value));
            }
            token_value.push(ch);
        }
    }

    if tokenize_mode == &TokenizeMode::Markdown && current_char != ']' && current_char != '[' {
        return Ok(tokenize_markdown(chars, &mut current_char, line_number));
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

            TokenizeMode::Codeblock => {
                return Err(CompileError {
                    msg: "Can't have nested scenes inside of a codeblock".to_string(),
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
            TokenizeMode::Normal => {
                *tokenize_mode = TokenizeMode::SceneHead;
                Ok(Token::ParentScene)
            }
            _ => {
                // [] is an empty scene
                if chars.peek() == Some(&']') {
                    chars.next();
                    *char_column += 1;

                    let mut spaces_after_scene = 0;

                    while let Some(ch) = chars.peek() {
                        if !ch.is_whitespace() || ch == &'\n' {
                            break;
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
            return Ok(Token::SceneClose(0));
        }

        *tokenize_mode = TokenizeMode::Markdown;

        // Track spaces after the scene close
        let mut spaces_after_scene = 0;
        while let Some(ch) = chars.peek() {
            if !ch.is_whitespace() || ch == &'\n' {
                break;
            }
            spaces_after_scene += 1;
            chars.next();
            *char_column += 1;
        }
        return Ok(Token::SceneClose(spaces_after_scene));
    }

    // Initialisation
    // Check if going into markdown mode
    if current_char == ':' {
        match &tokenize_mode {
            &TokenizeMode::SceneHead => {
                *tokenize_mode = TokenizeMode::Markdown;
            }
            &TokenizeMode::Codeblock => {
                chars.next();
                *char_column += 1;

                if scene_nesting_level == &0 {
                    *tokenize_mode = TokenizeMode::Normal;
                } else {
                    *tokenize_mode = TokenizeMode::Markdown;
                }
                return Ok(tokenize_codeblock(chars));
            }
            _ => {}
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
        );
    }

    // Check for string literals
    if current_char == '"' {
        while let Some(ch) = chars.next() {
            *char_column += 1;

            // Check for escape characters
            if ch == '\\' {
                if let Some(next_char) = chars.next() {
                    *char_column += 1;

                    token_value.push(next_char);
                }
            }
            if ch == '"' {
                return Ok(Token::StringLiteral(token_value));
            }
            token_value.push(ch);
        }
    }

    // Check for character literals
    if current_char == '\'' {
        let char_token = chars.next();
        *char_column += 1;

        if let Some(&char_after_next) = chars.peek() {
            if char_after_next == '\'' && char_token.is_some() {
                return Ok(Token::CharLiteral(char_token.unwrap()));
            }
        }
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

    if current_char == '$' {
        return Ok(Token::Signal(token_value));
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
                        while let Some(ch) = chars.next() {
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
                    while let Some(ch) = chars.next() {
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
    if current_char == '@' {
        return Ok(Token::Export);
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
            return Ok(Token::IntLiteral(token_value.parse::<i64>().unwrap()));
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
        );
    }

    if current_char == '_' {}

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
fn keyword_or_variable(
    token_value: &mut String,
    chars: &mut Peekable<Chars<'_>>,
    tokenize_mode: &mut TokenizeMode,
    line_number: &u32,
    char_column: &mut u32,
) -> Result<Token, CompileError> {
    // Match variables or keywords

    let name_starting_column = *char_column;

    loop {
        let is_a_char = match chars.peek() {
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

        // Always check if token value is a keyword in every other case
        // If there's whitespace or some termination
        // First check if there is a match to a keyword
        // Otherwise break out and check it is a valid variable name
        match token_value.as_str() {
            // Control Flow
            "return" => return Ok(Token::Return),
            "end" => return Ok(Token::End),
            "if" => return Ok(Token::If),
            "else" => return Ok(Token::Else),
            "for" => return Ok(Token::For),
            "import" => return Ok(Token::Import),
            "use" => return Ok(Token::Use),
            "break" => return Ok(Token::Break),
            "defer" => return Ok(Token::Defer),
            "in" => return Ok(Token::In),
            "as" => return Ok(Token::As),
            "copy" => return Ok(Token::Copy),

            // Logical
            "is" => return Ok(Token::Equal),
            "not" => return Ok(Token::Not),
            "and" => return Ok(Token::And),
            "or" => return Ok(Token::Or),

            // Data Types
            "fn" => return Ok(Token::FunctionKeyword),
            "true" | "True" => return Ok(Token::BoolLiteral(true)),
            "false" | "False" => return Ok(Token::BoolLiteral(false)),
            "Float" => return Ok(Token::TypeKeyword(DataType::Float)),
            "Int" => return Ok(Token::TypeKeyword(DataType::Int)),
            "String" => return Ok(Token::TypeKeyword(DataType::String)),
            "Bool" => return Ok(Token::TypeKeyword(DataType::Bool)),
            "type" | "Type" => return Ok(Token::TypeKeyword(DataType::Type)),

            // To be moved to standard library in future
            "print" => return Ok(Token::Print),
            "assert" => return Ok(Token::Assert),
            "math" => return Ok(Token::Math),

            _ => {}
        }

        // only bother tokenizing / reserving these keywords if inside a scene head
        match tokenize_mode {
            TokenizeMode::SceneHead => match token_value.as_str() {
                // Style
                "code" => {
                    *tokenize_mode = TokenizeMode::Codeblock;
                    return Ok(Token::CodeKeyword);
                }
                "id" => return Ok(Token::Id),
                "blank" => return Ok(Token::Blank),
                "bg" => return Ok(Token::BG),

                // Theme stuff
                "clr" => return Ok(Token::Color),

                // Colour keywords (all have optional alpha)
                "rgb" => return Ok(Token::Rgb),
                "hsv" => return Ok(Token::Hsv),
                "hsl" => return Ok(Token::Hsl),

                "red" => return Ok(Token::Red),
                "green" => return Ok(Token::Green),
                "blue" => return Ok(Token::Blue),
                "yellow" => return Ok(Token::Yellow),
                "cyan" => return Ok(Token::Cyan),
                "magenta" => return Ok(Token::Magenta),
                "white" => return Ok(Token::White),
                "black" => return Ok(Token::Black),
                "orange" => return Ok(Token::Orange),
                "pink" => return Ok(Token::Pink),
                "purple" => return Ok(Token::Purple),
                "grey" => return Ok(Token::Grey),

                // Layout
                "pad" => return Ok(Token::Padding),
                "space" => return Ok(Token::Margin),
                "center" => return Ok(Token::Center),
                "size" => return Ok(Token::Size), // Changes text size or content (vid/img) size depending on context
                "hide" => return Ok(Token::Hide),
                "nav" => return Ok(Token::Nav),
                "table" => return Ok(Token::Table),

                // Interactive
                "link" => return Ok(Token::Link),
                "button" => return Ok(Token::Button),
                "input" => return Ok(Token::Input),
                "click" => return Ok(Token::Click), // The action performed when clicked (any element)
                "form" => return Ok(Token::Form),
                "option" => return Ok(Token::Option),
                "dropdown" => return Ok(Token::Dropdown),

                // Media
                "img" => return Ok(Token::Img),
                "alt" => return Ok(Token::Alt),
                "video" => return Ok(Token::Video),
                "audio" => return Ok(Token::Audio),

                "order" => return Ok(Token::Order),
                "title" => return Ok(Token::Title),

                // Structure of the page
                "main" => return Ok(Token::Main),
                "header" => return Ok(Token::Header),
                "footer" => return Ok(Token::Footer),
                "section" => return Ok(Token::Section),

                // Other
                "ignore" => return Ok(Token::Ignore),
                "canvas" => return Ok(Token::Canvas),
                "redirect" => return Ok(Token::Redirect),

                _ => {}
            },

            TokenizeMode::CompilerDirective => match token_value.as_str() {
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
            },

            _ => {}
        }

        // Finally, if this was None, then break at end or make new variable
        if is_a_char && is_valid_identifier(&token_value) {
            return Ok(Token::Variable(token_value.to_string()));
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
        .map_or(false, |c| c.is_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

// A block that starts with : and ends with the 'end' keyword
// Everything inbetween is returned as a string
// Throws an error if there is no starting colon or ending 'end' keyword
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
                        msg: "block must end with 'end' keyword".to_string(),
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
        let end_keyword = "\nend";
        if string_value.ends_with(end_keyword) {
            closing_end_keyword = true;
            string_value = string_value
                .split_at(string_value.len() - end_keyword.len())
                .0
                .to_string();
        }
    }

    Ok(string_value)
}
