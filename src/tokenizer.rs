use super::tokens::{Token, TokenizeMode};
use crate::bs_types::DataType;
use crate::tokenize_scene::{tokenize_codeblock, tokenize_markdown};
use std::iter::Peekable;
use std::str::Chars;

pub fn tokenize(source_code: &str, module_name: &str) -> (Vec<Token>, Vec<u32>) {
    let mut tokens: Vec<Token> = Vec::new();
    let mut line_number: u32 = 1;
    let mut token_line_numbers: Vec<u32> = Vec::new();
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
        token_line_numbers.push(line_number);
        token = get_next_token(
            &mut chars,
            &mut tokenize_mode,
            &mut scene_nesting_level,
            &mut line_number,
        );
    }

    // Mark unused variables for removal in AST
    // DISABLED FOR NOW
    // for var_dec in var_names.iter() {
    //     if !var_dec.has_ref && !var_dec.is_exported {
    //         tokens[var_dec.index] = Token::DeadVarible(var_dec.name.to_string());
    //     }
    // }

    tokens.push(token);
    token_line_numbers.push(line_number);
    (tokens, token_line_numbers)
}

pub fn get_next_token(
    chars: &mut Peekable<Chars>,
    tokenize_mode: &mut TokenizeMode,
    scene_nesting_level: &mut i64,
    line_number: &mut u32,
) -> Token {
    let mut current_char = match chars.next() {
        Some(ch) => ch,
        None => return Token::EOF,
    };

    let mut token_value: String = String::new();

    // Check for raw strings (backticks)
    // Also used in scenes for raw outputs
    if current_char == '`' {
        while let Some(ch) = chars.next() {
            if ch == '`' {
                return Token::RawStringLiteral(token_value);
            }
            token_value.push(ch);
        }
    }

    if tokenize_mode == &TokenizeMode::Markdown && current_char != ']' && current_char != '[' {
        return tokenize_markdown(chars, &mut current_char, line_number);
    }

    // Whitespace
    if current_char == '\n' {
        *line_number += 1;
        return Token::Newline;
    }
    while current_char.is_whitespace() {
        current_char = match chars.next() {
            Some(ch) => ch,
            None => return Token::EOF,
        };
    }

    if current_char == '[' {
        *scene_nesting_level += 1;
        match tokenize_mode {
            TokenizeMode::SceneHead => {
                return Token::Error("Cannot have nested scenes inside of a scene head, must be inside the scene body. Use a colon to start the scene body.".to_string(), *line_number)
            }
            TokenizeMode::Codeblock => {
                return Token::Error("Cannot have nested scenes inside of a codeblock".to_string(), *line_number)
            }
            TokenizeMode::Normal => {
                *tokenize_mode = TokenizeMode::SceneHead;
                return Token::ParentScene
            }
            _ => {
                // [] is an empty scene
                if chars.peek() == Some(&']') {
                    chars.next();
                    let mut spaces_after_scene = 0;
                    while let Some(ch) = chars.peek() {
                        if !ch.is_whitespace() || ch == &'\n' {
                            break;
                        }
                        spaces_after_scene += 1;
                        chars.next();
                    }
                    return Token::EmptyScene(spaces_after_scene);
                }

                *tokenize_mode = TokenizeMode::SceneHead;
                return Token::SceneHead
            }
        }
    }

    if current_char == ']' {
        *scene_nesting_level -= 1;
        if *scene_nesting_level == 0 {
            *tokenize_mode = TokenizeMode::Normal;
            return Token::SceneClose(0);
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
        }
        return Token::SceneClose(spaces_after_scene);
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
                if scene_nesting_level == &0 {
                    *tokenize_mode = TokenizeMode::Normal;
                } else {
                    *tokenize_mode = TokenizeMode::Markdown;
                }
                return tokenize_codeblock(chars);
            }
            _ => {}
        }

        return Token::Colon;
    }

    //Window
    if current_char == '#' {
        *tokenize_mode = TokenizeMode::CompilerDirective;

        //Get compiler directive token
        return keyword_or_variable(&mut token_value, chars, tokenize_mode, line_number);
    }

    // Check for string literals
    if current_char == '"' {
        while let Some(ch) = chars.next() {
            // Check for escape characters
            if ch == '\\' {
                if let Some(next_char) = chars.next() {
                    token_value.push(next_char);
                }
            }
            if ch == '"' {
                return Token::StringLiteral(token_value);
            }
            token_value.push(ch);
        }
    }

    // Check for character literals
    if current_char == '\'' {
        let char_token = chars.next();
        if let Some(&char_after_next) = chars.peek() {
            if char_after_next == '\'' && char_token.is_some() {
                return Token::CharLiteral(char_token.unwrap());
            }
        }
    }

    // Functions and grouping expressions
    if current_char == '(' {
        return Token::OpenParenthesis;
    }
    if current_char == ')' {
        return Token::CloseParenthesis;
    }

    // Context Free Grammars
    if current_char == '=' {
        return Token::Assign;
    }
    if current_char == ',' {
        return Token::Comma;
    }
    if current_char == '.' {
        return Token::Dot;
    }

    if current_char == '$' {
        // Create new signal variable
        while let Some(&next_char) = chars.peek() {
            if next_char.is_alphanumeric() || next_char == '_' {
                token_value.push(chars.next().unwrap());
            } else {
                return Token::Signal(token_value);
            }
        }
    }

    // Collections
    if current_char == '{' {
        return Token::OpenCurly;
    }
    if current_char == '}' {
        return Token::CloseCurly;
    }

    //Error handling
    if current_char == '!' {
        return Token::Bang;
    }
    if current_char == '?' {
        return Token::QuestionMark;
    }

    // Comments / Subtraction / Negative / Scene Head / Arrow
    if current_char == '-' {
        if let Some(&next_char) = chars.peek() {
            // Comments
            if next_char == '-' {
                chars.next();

                // Check for multiline
                if let Some(&next_next_char) = chars.peek() {
                    if next_next_char == '-' {
                        // Mutliline Comment (---)
                        chars.next();

                        // Multiline Comment
                        while let Some(ch) = chars.next() {
                            token_value.push(ch);
                            if ch == '\n' {
                                *line_number += 1;
                            }
                            if token_value.ends_with("---") {
                                return Token::Comment(
                                    token_value.trim_end_matches("---").to_string(),
                                );
                            }
                        }
                    }

                    // Inline Comment
                    while let Some(ch) = chars.next() {
                        if ch == '\n' {
                            *line_number += 1;
                            return Token::Comment(token_value);
                        }
                        token_value.push(ch);
                    }
                }
            // Subtraction / Negative / Return / Subtract Assign
            } else {
                if next_char == '=' {
                    chars.next();
                    return Token::SubtractAssign;
                }
                if next_char == '>' {
                    chars.next();
                    return Token::Arrow;
                }
                if next_char.is_numeric() {
                    return Token::Negative;
                }
                return Token::Subtract;
            }
        }
    }

    // Mathematical operators,
    // must peak ahead to check for exponentiation (**) or roots (//) and assign variations
    if current_char == '+' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                return Token::AddAssign;
            }
        }
        return Token::Add;
    }
    if current_char == '*' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                return Token::MultiplyAssign;
            }
            return Token::Multiply;
        }
    }
    if current_char == '/' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '/' {
                chars.next();
                if let Some(&next_next_char) = chars.peek() {
                    if next_next_char == '=' {
                        chars.next();
                        return Token::RootAssign;
                    }
                }
                return Token::Root;
            }
            if next_char == '=' {
                chars.next();
                return Token::DivideAssign;
            }
            return Token::Divide;
        }
    }
    if current_char == '%' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                return Token::ModulusAssign;
            }
            if next_char == '%' {
                chars.next();
                if let Some(&next_next_char) = chars.peek() {
                    if next_next_char == '=' {
                        chars.next();
                        return Token::RemainderAssign;
                    }
                }
                return Token::Remainder;
            }
            return Token::Modulus;
        }
    }
    if current_char == '^' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                return Token::ExponentAssign;
            }
        }
        return Token::Exponent;
    }

    // Check for greater than and Less than logic operators
    // must also peak ahead to check it's not also equal to
    if current_char == '>' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                return Token::GreaterThanOrEqual;
            }
            return Token::GreaterThan;
        }
    }
    if current_char == '<' {
        if let Some(&next_char) = chars.peek() {
            if next_char == '=' {
                chars.next();
                return Token::LessThanOrEqual;
            }
            return Token::LessThan;
        }
    }

    // Exporting variables outside of the module or scope (public declaration)
    if current_char == '@' {
        return Token::Export;
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
                    return Token::Error(
                        "Cannot have more than one decimal point in a number".to_string(),
                        *line_number,
                    );
                }
                token_value.push(chars.next().unwrap());
                continue;
            }

            if next_char.is_numeric() {
                token_value.push(chars.next().unwrap());
            } else {
                break;
            }
        }

        if dot_count == 0 {
            return Token::IntLiteral(token_value.parse::<i64>().unwrap());
        }
        return Token::FloatLiteral(token_value.parse::<f64>().unwrap());
    }

    if current_char.is_alphabetic() {
        token_value.push(current_char);
        return keyword_or_variable(&mut token_value, chars, tokenize_mode, line_number);
    }

    if current_char == '_' {}

    Token::Error(
        format!(
            "Invalid Token Used (tokenizer). Token: '{}'. Tokenizer mode: {:?}",
            current_char, tokenize_mode
        ),
        *line_number,
    )
}

// Nested function because may need multiple searches for variables
fn keyword_or_variable(
    token_value: &mut String,
    chars: &mut Peekable<Chars<'_>>,
    tokenize_mode: &mut TokenizeMode,
    line_number: &u32,
) -> Token {
    // Match variables or keywords
    loop {
        let is_a_char = match chars.peek() {
            // If there is a char that is not None
            // And is an underscore or alphabetic, add it to the token value
            Some(char) => {
                if char.is_alphanumeric() || *char == '_' {
                    token_value.push(chars.next().unwrap());
                    continue;
                }
                true
            }
            None => false,
        };

        // Always check if token value is a keyword in every other case
        // If their is whitespace or some termination
        // First check if there is a match to a keyword
        // Otherwise break out and check it is a valid variable name
        match token_value.as_str() {
            // Control Flow
            "return" => return Token::Return,
            "end" => return Token::End,
            "if" => return Token::If,
            "else" => return Token::Else,
            "for" => return Token::For,
            "import" => return Token::Import,
            "use" => return Token::Use,
            "break" => return Token::Break,
            "defer" => return Token::Defer,
            "in" => return Token::In,
            "as" => return Token::As,
            "copy" => return Token::Copy,

            // Logical
            "is" => return Token::Equal,
            "not" => return Token::Not,
            "and" => return Token::And,
            "or" => return Token::Or,

            // Data Types
            "fn" => return Token::FunctionKeyword,
            "true" | "True" => return Token::BoolLiteral(true),
            "false" | "False" => return Token::BoolLiteral(false),
            "Float" => return Token::TypeKeyword(DataType::Float),
            "Int" => return Token::TypeKeyword(DataType::Int),
            "String" => return Token::TypeKeyword(DataType::String),
            "Bool" => return Token::TypeKeyword(DataType::Bool),
            "type" | "Type" => return Token::TypeKeyword(DataType::Type),

            // To be moved to standard library in future
            "print" => return Token::Print,
            "assert" => return Token::Assert,
            "math" => return Token::Math,

            _ => {}
        }

        // only bother tokenizing / reserving these keywords if inside of a scene head
        match tokenize_mode {
            TokenizeMode::SceneHead => match token_value.as_str() {
                // Style
                "code" => {
                    *tokenize_mode = TokenizeMode::Codeblock;
                    return Token::CodeKeyword;
                }
                "id" => return Token::Id,
                "blank" => return Token::Blank,
                "bg" => return Token::BG,

                // Theme stuff
                "clr" => return Token::Color,

                // Colour keywords
                "rgb" => return Token::Rgb,
                "hsl" => return Token::Hsv,

                "red" => return Token::Red,
                "green" => return Token::Green,
                "blue" => return Token::Blue,
                "yellow" => return Token::Yellow,
                "cyan" => return Token::Cyan,
                "magenta" => return Token::Magenta,
                "white" => return Token::White,
                "black" => return Token::Black,
                "orange" => return Token::Orange,
                "pink" => return Token::Pink,
                "purple" => return Token::Purple,
                "grey" => return Token::Grey,

                // Layout
                "pad" => return Token::Padding,
                "space" => return Token::Margin,
                "center" => return Token::Center,
                "size" => return Token::Size, // Changes text size or content (vid/img) size depending on context
                "hide" => return Token::Hide,
                "nav" => return Token::Nav,
                "table" => return Token::Table,

                // Interactive
                "link" => return Token::A,
                "button" => return Token::Button,
                "input" => return Token::Input,
                "click" => return Token::Click, // The action performed when clicked (any element)
                "form" => return Token::Form,
                "option" => return Token::Option,
                "dropdown" => return Token::Dropdown,

                // Media
                "img" => return Token::Img,
                "alt" => return Token::Alt,
                "video" => return Token::Video,
                "audio" => return Token::Audio,

                "order" => return Token::Order,
                "title" => return Token::Title,

                // Structure of the page
                "main" => return Token::Main,
                "header" => return Token::Header,
                "footer" => return Token::Footer,
                "section" => return Token::Section,

                // Other
                "ignore" => return Token::Ignore,
                "canvas" => return Token::Canvas,
                "redirect" => return Token::Redirect,
                _ => {}
            },

            TokenizeMode::CompilerDirective => match token_value.as_str() {
                "settings" => {
                    *tokenize_mode = TokenizeMode::Normal;
                    return Token::Settings;
                }
                "title" => {
                    *tokenize_mode = TokenizeMode::Normal;
                    return Token::Title;
                }
                "date" => {
                    *tokenize_mode = TokenizeMode::Normal;
                    return Token::Date;
                }
                "JS" => {
                    *tokenize_mode = TokenizeMode::Normal;
                    return match string_block(chars, line_number) {
                        Ok(js_code) => Token::JS(js_code),
                        Err(err) => err,
                    };
                }
                "WASM" => {
                    *tokenize_mode = TokenizeMode::Normal;
                    return match string_block(chars, line_number) {
                        Ok(js_code) => Token::WASM(js_code),
                        Err(err) => err,
                    };
                }
                "CSS" => {
                    *tokenize_mode = TokenizeMode::Normal;
                    return match string_block(chars, line_number) {
                        Ok(css_code) => Token::CSS(css_code),
                        Err(err) => err,
                    };
                }
                _ => {}
            },

            _ => {}
        }

        // Finally, if this was None, then break at end or make new variable
        if is_a_char && is_valid_identifier(&token_value) {
            return Token::Variable(token_value.to_string());
        } else {
            break;
        }
    }

    // Failing all of that, this is an error
    return Token::Error(
        format!("Invalid variable name: {}", token_value),
        *line_number,
    );
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
fn string_block(chars: &mut Peekable<Chars>, line_number: &u32) -> Result<String, Token> {
    let mut string_value = String::new();

    while let Some(ch) = chars.peek() {
        // Skip whitespace before the first colon that starts the block
        if ch.is_whitespace() {
            chars.next();
            continue;
        }

        // Start the code block at the colon
        if *ch != ':' {
            return Err(Token::Error(
                "Block must start with a colon".to_string(),
                *line_number,
            ));
        } else {
            chars.next();
            break;
        }
    }

    let mut closing_end_keyword = false;
    loop {
        match chars.peek() {
            Some(char) => {
                string_value.push(*char);
                chars.next();
            }
            None => {
                if !closing_end_keyword {
                    return Err(Token::Error(
                        "block must end with 'end' keyword".to_string(),
                        *line_number,
                    ));
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
