use crate::compiler::compiler_errors::CompileError;
use crate::{return_syntax_error, settings, token_log};
use colour::green_ln;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use crate::compiler::parsers::tokenizer::compiler_directives::compiler_directive;
use crate::compiler::parsers::tokenizer::tokens::{FileTokens, TextLocation, Token, TokenKind, TokenStream, TokenizeMode};

pub const END_SCOPE_CHAR: char = ';';

#[macro_export]
macro_rules! return_token {
    ($kind:expr, $stream:expr $(,)?) => {
        return Ok(Token::new($kind, $stream.new_location()))
    };
}

pub fn tokenize(
    source_code: &str,
    src_path: &Path,
    mode: TokenizeMode,
) -> Result<FileTokens, CompileError> {
    // About 1/6 of the source code seems to be tokens roughly from some very small preliminary tests
    let initial_capacity = source_code.len() / settings::SRC_TO_TOKEN_RATIO;
    let type_declarations_initial_capacity = settings::IMPORTS_CAPACITY;

    let mut template_nesting_level: i64 = if mode == TokenizeMode::Normal {
        0
    } else {
        // This is so .mt files or the repl can't break out of the template head/body when starting there
        i64::MAX / 2
    };

    let mut tokens: Vec<Token> = Vec::with_capacity(initial_capacity);
    let mut stream = TokenStream::new(source_code, src_path, mode);
    let mut type_declarations = HashSet::with_capacity(type_declarations_initial_capacity);

    let mut token: Token = Token::new(
        TokenKind::ModuleStart(String::new()),
        TextLocation::default(),
    );

    loop {
        #[cfg(feature = "show_tokens")]
        token_log!(token);

        if token.kind == TokenKind::Eof {
            break;
        }

        tokens.push(token);
        token = get_token_kind(
            &mut stream,
            &mut template_nesting_level,
            &mut type_declarations,
        )?;
    }

    tokens.push(token);

    // First creation of TokenContext
    Ok(FileTokens::new(src_path.to_owned(), tokens))
}

pub fn get_token_kind(
    stream: &mut TokenStream,
    template_nesting_level: &mut i64,
    type_declarations: &mut HashSet<String>,
) -> Result<Token, CompileError> {
    let mut current_char = match stream.next() {
        Some(ch) => ch,
        None => return_token!(TokenKind::Eof, stream),
    };

    let mut token_value: String = String::new();

    #[cfg(feature = "show_char_stream")]
    green_ln!("get_token_kind starting with: '{current_char}'");

    // Check for raw strings (backticks)
    // Also used in scenes for raw outputs
    if current_char == '`' {
        while let Some(ch) = stream.next() {
            if ch == '`' {
                return_token!(TokenKind::RawStringLiteral(token_value), stream);
            }

            token_value.push(ch);
        }
    }

    if stream.mode == TokenizeMode::TemplateBody && current_char != ']' && current_char != '[' {
        return tokenize_template_body(current_char, stream);
    }

    // Whitespace

    if current_char == '\n' {
        return_token!(TokenKind::Newline, stream);
    } else if current_char == '\r' {
        if stream.peek() == Some(&'\n') {
            stream.next();
            return_token!(TokenKind::Newline, stream);
        } else {
            // Ignore naked CR (throw warning in future?)
            stream.next();
        }
    }

    while current_char.is_whitespace() {
        current_char = match stream.next() {
            Some(ch) => ch,
            None => return_token!(TokenKind::Eof, stream),
        };
    }

    // To ignore leading whitespace for the next token position
    stream.update_start_position();

    if current_char == '[' {
        *template_nesting_level += 1;
        match stream.mode {
            TokenizeMode::TemplateHead => {
                return_syntax_error!(
                    stream.new_location(),
                    "Cannot have nested templates inside of a template head, must be inside the template body. \
                    Use a colon to start the template body.",
                )
            }

            TokenizeMode::Normal => {
                // Going into the template head
                stream.mode = TokenizeMode::TemplateHead;
                return_token!(TokenKind::ParentTemplate, stream);
            }

            _ => {
                // This is a slot
                if stream.peek() == Some(&']') {
                    stream.next();

                    let mut spaces_after_template = 0;

                    while let Some(ch) = stream.peek() {
                        if !ch.is_whitespace() {
                            break;
                        }

                        spaces_after_template += 1;

                        stream.next();
                    }

                    return_token!(TokenKind::EmptyTemplate(spaces_after_template), stream);
                }

                // Starting a new template head from inside a template body
                stream.mode = TokenizeMode::TemplateHead;
                return_token!(TokenKind::TemplateHead, stream);
            }
        };
    }

    if current_char == ']' {
        *template_nesting_level -= 1;

        if *template_nesting_level == 0 {
            stream.mode = TokenizeMode::Normal;
        } else {
            stream.mode = TokenizeMode::TemplateBody;
        }

        return_token!(TokenKind::TemplateClose, stream);
    }

    // Check if going into the template body
    if current_char == ':' {
        if stream.mode == TokenizeMode::TemplateHead {
            stream.mode = TokenizeMode::TemplateBody;

            return_token!(TokenKind::EndTemplateHead, stream);
        }

        // :: (not currently using)
        // if let Some(&next_char) = stream.peek() {
        //     if next_char == ':' {
        //         stream.next();
        //
        //         return_token!(TokenKind::Choice, stream);
        //     }
        // }

        return_token!(TokenKind::Colon, stream);
    }

    if current_char == END_SCOPE_CHAR {
        return_token!(TokenKind::End, stream);
    }

    // Check for string literals
    if current_char == '"' {
        return tokenize_string(stream);
    }

    // Check for character literals
    if current_char == '\'' {
        if let Some(c) = stream.next() {
            if let Some(&char_after_next) = stream.peek()
                && char_after_next == '\''
            {
                return_token!(TokenKind::CharLiteral(c), stream);
            }
        };

        // If not correct declaration of char
        return_syntax_error!(
            stream.new_location(),
            "Expected a character after the single quote in a char literal. Found {current_char}",
        )
    }

    // Functions and grouping expressions
    if current_char == '(' {
        return_token!(TokenKind::OpenParenthesis, stream);
    }

    if current_char == ')' {
        return_token!(TokenKind::CloseParenthesis, stream);
    }

    // Context Free Grammars
    if current_char == '=' {
        return_token!(TokenKind::Assign, stream);
    }

    if current_char == ',' {
        return_token!(TokenKind::Comma, stream);
    }

    if current_char == '.' {
        // Check if variadic
        if let Some(&peeked_char) = stream.peek()
            && peeked_char == '.'
        {
            stream.next();
            return_token!(TokenKind::Variadic, stream);
        }

        return_token!(TokenKind::Dot, stream);
    }

    // Collections
    if current_char == '{' {
        return_token!(TokenKind::OpenCurly, stream);
    }

    if current_char == '}' {
        return_token!(TokenKind::CloseCurly, stream);
    }

    // Structs
    if current_char == '|' {
        return_token!(TokenKind::TypeParameterBracket, stream);
    }

    // Currently not using bangs
    if current_char == '!' {
        return_token!(TokenKind::Bang, stream);
    }

    // Option type
    if current_char == '?' {
        return_token!(TokenKind::QuestionMark, stream);
    }

    // Comments / Subtraction / Negative / Scene Head / Arrow
    if current_char == '-' {
        if let Some(&next_char) = stream.peek() {
            // Comments
            if next_char == '-' {
                stream.next();

                while let Some(ch) = stream.peek() {
                    if ch == &'\n' {
                        break;
                    }

                    stream.next();
                }

                // Do not add any token to the stream, call this function again
                return get_token_kind(stream, template_nesting_level, type_declarations);

            // Subtraction / Negative / Return / Subtract Assign
            } else {
                if next_char == '=' {
                    stream.next();
                    return_token!(TokenKind::SubtractAssign, stream);
                }

                if next_char == '>' {
                    stream.next();
                    return_token!(TokenKind::Arrow, stream);
                }

                if next_char.is_numeric() {
                    return_token!(TokenKind::Negative, stream);
                }

                return_token!(TokenKind::Subtract, stream);
            }
        }
    }

    // Mathematical operators
    // must peak ahead to check for exponentiation (**) or roots (//) and assign variations
    if current_char == '+' {
        if let Some(&next_char) = stream.peek() {
            if next_char == '=' {
                stream.next();
                return_token!(TokenKind::AddAssign, stream);
            }
        }

        return_token!(TokenKind::Add, stream);
    }

    if current_char == '*' {
        if let Some(&next_char) = stream.peek() {
            if next_char == '=' {
                stream.next();
                return_token!(TokenKind::MultiplyAssign, stream);
            }
            return_token!(TokenKind::Multiply, stream);
        }
    }

    if current_char == '/' {
        if let Some(&next_char) = stream.peek() {
            if next_char == '/' {
                stream.next();

                if let Some(&next_next_char) = stream.peek() {
                    if next_next_char == '=' {
                        stream.next();
                        return_token!(TokenKind::RootAssign, stream);
                    }
                }
                return_token!(TokenKind::Root, stream);
            }
            if next_char == '=' {
                stream.next();
                return_token!(TokenKind::DivideAssign, stream);
            }
            return_token!(TokenKind::Divide, stream);
        }
    }

    if current_char == '%' {
        if let Some(&next_char) = stream.peek() {
            if next_char == '=' {
                stream.next();
                return_token!(TokenKind::ModulusAssign, stream);
            }
            if next_char == '%' {
                stream.next();
                if let Some(&next_next_char) = stream.peek() {
                    if next_next_char == '=' {
                        stream.next();
                        return_token!(TokenKind::RemainderAssign, stream);
                    }
                }
                return_token!(TokenKind::Remainder, stream);
            }
            return_token!(TokenKind::Modulus, stream);
        }
    }

    if current_char == '^' {
        if let Some(&next_char) = stream.peek() {
            if next_char == '=' {
                stream.next();
                return_token!(TokenKind::ExponentAssign, stream);
            }
        }
        return_token!(TokenKind::Exponent, stream);
    }

    // Check for greater than and Less than logic operators
    // must also peak ahead to check it's not also equal to
    if current_char == '>' {
        if let Some(&next_char) = stream.peek() {
            if next_char == '=' {
                stream.next();
                return_token!(TokenKind::GreaterThanOrEqual, stream);
            }
            return_token!(TokenKind::GreaterThan, stream);
        }
    }

    if current_char == '<' {
        if let Some(&next_char) = stream.peek() {
            if next_char == '=' {
                stream.next();
                return_token!(TokenKind::LessThanOrEqual, stream);
            }
            return_token!(TokenKind::LessThan, stream);
        }
    }

    if current_char == '~' {
        return_token!(TokenKind::Mutable, stream);
    }

    // Compiler Directives
    if current_char == '#' {
        return compiler_directive(&mut token_value, stream);
    }

    // Used for paths outside of template heads
    if current_char == '@' {
        if stream.mode == TokenizeMode::TemplateHead {
            while let Some(&next_char) = stream.peek() {
                if next_char.is_alphanumeric() || next_char == '_' {
                    token_value.push(stream.next().unwrap());
                    continue;
                }
                break;
            }
            return_token!(TokenKind::Id(token_value), stream);
        }

        // The @ should always be followed by a path
        // Todo: allow spaces after the '@'?
        stream.next();

        //
        let path = tokenize_path(stream)?;
        return_token!(TokenKind::PathLiteral(path), stream);
    }

    // Wildcard for pattern matching
    if current_char == '_' {
        return_token!(TokenKind::Wildcard, stream);
    }

    // Numbers
    if current_char.is_numeric() {
        token_value.push(current_char);
        let mut dot_count = 0;

        while let Some(&next_char) = stream.peek() {
            if next_char == '_' {
                stream.next();
                continue;
            }

            if next_char == '.' {
                // TODO: need to handle range operator without backtracking through token stream
                // Or consuming too many dots.

                dot_count += 1;
                // Stop if too many dots in number
                if dot_count > 1 {
                    return_syntax_error!(
                        stream.new_location(),
                        "Can't have more than one decimal point in a number"
                    )
                }

                let dot = stream.next().unwrap();
                token_value.push(dot);
                continue;
            }

            if next_char.is_numeric() {
                token_value.push(stream.next().unwrap());
            } else {
                break;
            }
        }

        if dot_count == 0 {
            return_token!(
                TokenKind::IntLiteral(token_value.parse::<i64>().unwrap()),
                stream
            );
        }
        return_token!(
            TokenKind::FloatLiteral(token_value.parse::<f64>().unwrap()),
            stream
        );
    }

    if current_char.is_alphabetic() {
        token_value.push(current_char);
        return keyword_or_variable(&mut token_value, stream, type_declarations);
    }

    return_syntax_error!(
        stream.new_location(),
        "Invalid Token Used: '{}' this is not recognised or supported by the compiler",
        current_char
    )
}

fn keyword_or_variable(
    token_value: &mut String,
    stream: &mut TokenStream,
    type_declarations: &mut HashSet<String>,
) -> Result<Token, CompileError> {
    // Match variables or keywords
    loop {
        if let Some(char) = stream.peek()
            && (char.is_alphanumeric() || *char == '_')
        {
            token_value.push(stream.next().unwrap());
            continue;
        }

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
            // Control Flow
            // END_KEYWORD => return_token!(TokenKind::End, stream),
            "if" => return_token!(TokenKind::If, stream),
            "return" => return_token!(TokenKind::Return, stream),

            "else" => return_token!(TokenKind::Else, stream),
            "for" => return_token!(TokenKind::For, stream),
            "break" => return_token!(TokenKind::Break, stream),
            "defer" => return_token!(TokenKind::Defer, stream),
            "in" => return_token!(TokenKind::In, stream),
            "as" => return_token!(TokenKind::As, stream),
            "copy" => return_token!(TokenKind::Copy, stream),
            "to" => return_token!(TokenKind::Range, stream),

            // Logical
            "is" => return_token!(TokenKind::Is, stream),
            "not" => return_token!(TokenKind::Not, stream),
            "and" => return_token!(TokenKind::And, stream),
            "or" => return_token!(TokenKind::Or, stream),

            // Data Types
            "true" => return_token!(TokenKind::BoolLiteral(true), stream),
            "True" => return_token!(TokenKind::DatatypeTrue, stream),
            "false" => return_token!(TokenKind::BoolLiteral(false), stream),
            "False" => return_token!(TokenKind::DatatypeFalse, stream),

            "Float" => return_token!(TokenKind::DatatypeFloat, stream),
            "Int" => return_token!(TokenKind::DatatypeInt, stream),
            "String" => return_token!(TokenKind::DatatypeString, stream),
            "Bool" => return_token!(TokenKind::DatatypeBool, stream),

            "None" => return_token!(TokenKind::DatatypeNone, stream),

            _ => {}
        }

        // VARIABLE
        if is_valid_identifier(token_value) {
            // If this has a capital letter at the start of it, it's a type declaration
            if token_value.chars().next().unwrap().is_uppercase() {
                type_declarations.insert(token_value.clone());
            }

            return_token!(TokenKind::Symbol(token_value.to_string()), stream);
        } else {
            // Failing all of that, this is an invalid variable name
            return_syntax_error!(
                stream.new_location(),
                "Invalid variable name or keyword: '{}'",
                token_value
            )
        }
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

// A block that starts with an open parenthesis and ends with a close parenthesis
// Everything in between is returned as a string
// Throws an error if there is no starting parenthesis or ending parenthesis
pub fn string_block(stream: &mut TokenStream) -> Result<String, CompileError> {
    let mut string_value = String::new();

    while let Some(ch) = stream.peek() {
        // Skip whitespace before the first colon that starts the block
        if ch.is_whitespace() {
            stream.next();
            continue;
        }

        // Start the code block at the colon
        if *ch != '(' {
            return_syntax_error!(
                stream.new_location(),
                "Expected ':' to start a new block, found '{}'",
                ch
            )
        } else {
            stream.next();
            break;
        }
    }

    let mut parenthesis_closed = 0;
    let mut parenthesis_opened = 1;

    loop {
        match stream.peek() {
            Some(char) => {
                if char == &')' {
                    parenthesis_closed += 1;
                }
                if char == &'(' {
                    parenthesis_opened += 1;
                }

                if parenthesis_opened == parenthesis_closed {
                    stream.next();
                    break;
                }
                string_value.push(*char);
                stream.next();
            }
            None => {
                if parenthesis_opened > parenthesis_closed {
                    return_syntax_error!(
                        stream.new_location(),
                        "File ended before closing the last parenthesis",
                    )
                }
                break;
            }
        };
    }

    Ok(string_value)
}

fn tokenize_string(stream: &mut TokenStream) -> Result<Token, CompileError> {
    let mut token_value = String::new();

    // Currently should be at the character that started the String
    while let Some(ch) = stream.next() {
        // Check for escape characters
        if ch == '\\' {
            if let Some(next_char) = stream.next() {
                token_value.push(next_char);
            }
        } else if ch == '"' {
            return_token!(TokenKind::StringSliceLiteral(token_value), stream);
        }

        token_value.push(ch);
    }

    return_token!(TokenKind::StringSliceLiteral(token_value), stream);
}

fn tokenize_template_body(
    current_char: char,
    stream: &mut TokenStream,
) -> Result<Token, CompileError> {
    let mut token_value = String::from(current_char);

    // Currently should be at the character that started the String
    while let Some(ch) = stream.peek() {
        // Check for escape characters
        if ch == &'\\' {
            stream.next();

            if let Some(next_char) = stream.next() {
                token_value.push(next_char);
            }
        } else if ch == &'[' || ch == &']' {
            return_token!(TokenKind::StringSliceLiteral(token_value), stream);
        }

        // Should always be a valid char
        token_value.push(stream.next().unwrap());
    }

    return_token!(TokenKind::StringSliceLiteral(token_value), stream);
}

fn tokenize_path(stream: &mut TokenStream) -> Result<String, CompileError> {
    let mut import_path = String::new();
    let mut break_on_whitespace = true;

    // If the path has to have whitespaces, it can be optionally surrounded by quotes
    if stream.peek() == Some(&'"') {
        break_on_whitespace = false;
    }

    while let Some(c) = stream.peek() {
        // Breakout on the first-detected whitespace or the end of the string
        if (c.is_whitespace() && break_on_whitespace) || (*c == '"' && !break_on_whitespace) {
            break;
        }

        import_path.push(c.to_owned());
        stream.next();
        continue;
    }

    if import_path.is_empty() {
        return_syntax_error!(stream.new_location(), "Import path cannot be empty")
    }

    Ok(import_path)
}
