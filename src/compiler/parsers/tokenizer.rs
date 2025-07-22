use crate::compiler::compiler_errors::ErrorType;
#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::tokens::{
    TextLocation, Token, TokenContext, TokenKind, TokenStream, TokenizeMode,
};
use crate::{return_syntax_error, settings};

macro_rules! return_token {
    ($kind:expr, $stream:expr $(,)?) => {
        return Ok(Token::new($kind, $stream.new_location()))
    };
}

pub fn tokenize<'a>(
    source_code: &str,
    src_path: &'a Path,
) -> Result<TokenContext<'a>, CompileError> {
    // About 1/6 of the source code seems to be tokens roughly from some very small preliminary tests
    let initial_capacity = source_code.len() / settings::SRC_TO_TOKEN_RATIO;
    let imports_initial_capacity = settings::IMPORTS_CAPACITY;

    let mut tokens: Vec<Token> = Vec::with_capacity(initial_capacity);
    let mut stream = TokenStream::new(source_code);
    let mut imports = HashSet::with_capacity(imports_initial_capacity);

    let template_nesting_level: &mut i64 = &mut 0;

    let mut token: Token = Token::new(
        TokenKind::ModuleStart(String::new()),
        TextLocation::default(),
    );

    loop {
        if token.kind == TokenKind::EOF {
            break;
        }

        tokens.push(token);
        token = get_token_kind(&mut stream, template_nesting_level, &mut imports)?;
    }

    tokens.push(token);

    // First creation of TokenContext
    Ok(TokenContext::new(src_path, tokens, imports))
}

pub fn get_token_kind(
    stream: &mut TokenStream,
    template_nesting_level: &mut i64,
    imports: &mut HashSet<PathBuf>,
) -> Result<Token, CompileError> {
    let mut current_char = match stream.next() {
        Some(ch) => ch,
        None => return_token!(TokenKind::EOF, stream),
    };

    let mut token_value: String = String::new();

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

    if stream.context == TokenizeMode::TemplateBody && current_char != ']' && current_char != '[' {
        return tokenize_template_body(current_char, stream);
    }

    // Whitespace

    if current_char == '\n' {
        return_token!(TokenKind::Newline, stream);
    } else if current_char == 'r' {
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
            None => return_token!(TokenKind::EOF, stream),
        };
    }

    // To ignore leading whitespace for the next token position
    stream.update_start_position();

    if current_char == '[' {
        *template_nesting_level += 1;
        match stream.context {
            TokenizeMode::TemplateHead => {
                return_syntax_error!(
                    stream.new_location(),
                    "Cannot have nested templates inside of a template head, must be inside the template body. \
                    Use a colon to start the template body.",
                )
            }

            TokenizeMode::Normal => {
                stream.context = TokenizeMode::TemplateHead;
                return_token!(TokenKind::ParentTemplate, stream);
            }

            // Going into the scene head
            _ => {
                // [] is an empty scene
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

                stream.context = TokenizeMode::TemplateHead;

                return_token!(TokenKind::TemplateHead, stream);
            }
        };
    }

    if current_char == ']' {
        *template_nesting_level -= 1;

        if *template_nesting_level == 0 {
            stream.context = TokenizeMode::Normal;
        } else {
            stream.context = TokenizeMode::TemplateBody;
        }

        return_token!(TokenKind::TemplateClose, stream);
    }

    // Check if going into the scene body
    if current_char == ':' {
        if stream.context == TokenizeMode::TemplateHead {
            stream.context = TokenizeMode::TemplateBody;

            return_token!(TokenKind::Colon, stream);
        }

        // ::
        if let Some(&next_char) = stream.peek() {
            if next_char == ':' {
                stream.next();

                return_token!(TokenKind::Choice, stream);
            }
        }

        return_token!(TokenKind::Colon, stream);
    }

    // Compiler Directives
    if current_char == '#' {
        return compiler_directive(&mut token_value, stream);
    }

    // Check for string literals
    if current_char == '"' {
        return tokenize_string(stream);
    }

    // Check for character literals
    if current_char == '\'' {
        if let Some(c) = stream.next() {
            if let Some(&char_after_next) = stream.peek() {
                if char_after_next == '\'' {
                    return_token!(TokenKind::CharLiteral(c), stream);
                }
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
        return_token!(TokenKind::StructDefinition, stream);
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
                        stream.next();
                        return_token!(TokenKind::Comment, stream);
                    }

                    stream.next();
                }

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

    // Exporting variables out of the module or scope (public declaration)
    // When used in a scene head, it's an ID for that scene
    if current_char == '@' {
        if stream.context == TokenizeMode::TemplateHead {
            while let Some(&next_char) = stream.peek() {
                if next_char.is_alphanumeric() || next_char == '_' {
                    token_value.push(stream.next().unwrap());
                    continue;
                }
                break;
            }
            return_token!(TokenKind::Id(token_value), stream);
        }

        return_syntax_error!(
            stream.new_location(),
            "Cannot use @ outside of a template head"
        )
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
                dot_count += 1;
                // Stop if too many dots
                if dot_count > 1 {
                    return_syntax_error!(
                        stream.new_location(),
                        "Can't have more than one decimal point in a number"
                    )
                }
                token_value.push(stream.next().unwrap());
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
                TokenKind::IntLiteral(token_value.parse::<i32>().unwrap()),
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

        return keyword_or_variable(&mut token_value, stream, imports);
    }

    return_syntax_error!(
        stream.new_location(),
        "Invalid Token Used: '{}' this is not recognised or supported by the compiler",
        current_char
    )
}

// Nested function because may need multiple searches for variables
const END_KEYWORD: &str = "zz";

fn keyword_or_variable(
    token_value: &mut String,
    stream: &mut TokenStream,
    imports: &mut HashSet<PathBuf>,
) -> Result<Token, CompileError> {
    // Match variables or keywords
    loop {
        let is_not_eof = match stream.peek() {
            // If there is a char that is not None
            // And is an underscore or alphabetic, add it to the token value
            Some(char) => {
                if char.is_alphanumeric() || *char == '_' {
                    token_value.push(stream.next().unwrap());
                    continue;
                }

                return_syntax_error!(
                    stream.new_location(),
                    "Unexpected end of file after : '{}'",
                    token_value
                )
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
            // Control Flow
            END_KEYWORD => return_token!(TokenKind::End, stream),
            "if" => return_token!(TokenKind::If, stream),
            "return" => return_token!(TokenKind::Return, stream),

            "else" => return_token!(TokenKind::Else, stream),
            "for" => return_token!(TokenKind::For, stream),
            "break" => return_token!(TokenKind::Break, stream),
            "defer" => return_token!(TokenKind::Defer, stream),
            "in" => return_token!(TokenKind::In, stream),
            "as" => return_token!(TokenKind::As, stream),
            "copy" => return_token!(TokenKind::Copy, stream),

            "import" => {
                imports.insert(tokenize_import(stream)?);
                return_token!(TokenKind::Import, stream)
            }

            // Logical
            "is" => return_token!(TokenKind::Is, stream),
            "not" => return_token!(TokenKind::Not, stream),
            "and" => return_token!(TokenKind::And, stream),
            "or" => return_token!(TokenKind::Or, stream),

            // Data Types
            "true" | "True" => return_token!(TokenKind::BoolLiteral(true), stream),
            "false" | "False" => return_token!(TokenKind::BoolLiteral(false), stream),

            "Float" => return_token!(TokenKind::DatatypeLiteral(DataType::Float(false)), stream),
            "Int" => return_token!(TokenKind::DatatypeLiteral(DataType::Int(false)), stream),
            "String" => return_token!(TokenKind::DatatypeLiteral(DataType::String(false)), stream),
            "Bool" => return_token!(TokenKind::DatatypeLiteral(DataType::Bool(false)), stream),

            "None" => return_token!(TokenKind::DatatypeLiteral(DataType::None), stream),

            "async" => return_token!(TokenKind::Async, stream),

            // Scene-related keywords
            "Template" => return_token!(
                TokenKind::DatatypeLiteral(DataType::Template(false)),
                stream
            ),

            _ => {}
        }

        // VARIABLE
        if is_not_eof && is_valid_identifier(token_value) {
            // Check if this declaration has any modifiers in front of it
            return_token!(TokenKind::Symbol(token_value.to_string()), stream);
        } else {
            // Failing all of that, this is an invalid variable name
            return_syntax_error!(
                stream.new_location(),
                "Invalid variable name: '{}'",
                token_value
            )
        }
    }
}

fn compiler_directive(
    token_value: &mut String,
    stream: &mut TokenStream,
) -> Result<Token, CompileError> {
    loop {
        if stream
            .peek()
            .is_some_and(|c| c.is_alphanumeric() || c == &'_')
        {
            token_value.push(stream.next().unwrap());
            continue;
        }

        match token_value.as_str() {
            // Built-in functions
            "io" => return_token!(TokenKind::Print, stream),
            "assert" => return_token!(TokenKind::Assert, stream),
            "panic" => return_token!(TokenKind::Panic, stream),
            "log" => return_token!(TokenKind::Log, stream),

            // Compiler settings
            "settings" => return_token!(TokenKind::Settings, stream),
            "title" => return_token!(TokenKind::Title, stream),
            "date" => return_token!(TokenKind::Date, stream),

            // External language blocks
            "JS" => return_token!(TokenKind::JS(string_block(stream)?), stream),
            "WASM" => return_token!(TokenKind::WASM(string_block(stream)?), stream),
            "CSS" => return_token!(TokenKind::CSS(string_block(stream)?), stream),

            // Scene Style properties
            "markdown" => return_token!(TokenKind::Markdown, stream),
            "child_default" => return_token!(TokenKind::ChildDefault, stream),
            "slot" => return_token!(TokenKind::Slot, stream),

            _ => {
                return_syntax_error!(
                    stream.new_location(),
                    "Invalid compiler directive: #{}",
                    token_value
                )
            }
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
fn string_block(stream: &mut TokenStream) -> Result<String, CompileError> {
    let mut string_value = String::new();

    while let Some(ch) = stream.peek() {
        // Skip whitespace before the first colon that starts the block
        if ch.is_whitespace() {
            stream.next();
            continue;
        }

        // Start the code block at the colon
        if *ch != ':' {
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

    let mut closing_end_keyword = false;

    loop {
        match stream.peek() {
            Some(char) => {
                string_value.push(*char);

                stream.next();
            }
            None => {
                if !closing_end_keyword {
                    return_syntax_error!(
                        stream.new_location(),
                        "Expected '{}' keyword to end the block",
                        END_KEYWORD
                    )
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
            return_token!(TokenKind::StringLiteral(token_value), stream);
        }

        token_value.push(ch);
    }

    return_token!(TokenKind::StringLiteral(token_value), stream);
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
            return_token!(TokenKind::StringLiteral(token_value), stream);
        }

        // Should always be a valid char
        token_value.push(stream.next().unwrap());
    }

    return_token!(TokenKind::StringLiteral(token_value), stream);
}

fn tokenize_import(stream: &mut TokenStream) -> Result<PathBuf, CompileError> {
    // Skip starting whitespace
    while let Some(c) = stream.peek() {
        if c.is_whitespace() {
            if c == &'\n' {
                return_syntax_error!(
                    stream.new_location(),
                    "Unexpected newline in import statement. Import statements must be on a single line. e.g import path/to/file"
                )
            }

            stream.next();
            continue;
        }

        break;
    }

    // Parse the import path
    // This assumes starting the path from the project root directory
    let mut import_path = String::new();
    while let Some(c) = stream.peek() {
        if c.is_whitespace() {
            break;
        }

        import_path.push(stream.next().unwrap());
    }

    if import_path.is_empty() {
        return_syntax_error!(stream.new_location(), "Import path cannot be empty")
    }

    Ok(PathBuf::from(import_path))
}
