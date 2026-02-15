use crate::compiler_frontend::basic_utility_functions::is_valid_var_char;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::headers::imports::parse_imports;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::compiler_directives::compiler_directive;
use crate::compiler_frontend::tokenizer::tokens::{
    FileTokens, TextLocation, Token, TokenKind, TokenStream, TokenizeMode,
};
use crate::{return_syntax_error, settings, token_log};

pub const END_SCOPE_CHAR: char = ';';

#[macro_export]
macro_rules! return_token {
    ($kind:expr, $stream:expr $(,)?) => {
        return Ok(Token::new($kind, $stream.new_location()))
    };
}

pub fn tokenize(
    source_code: &str,
    src_path: &InternedPath,
    mode: TokenizeMode,
    string_table: &mut StringTable,
) -> Result<FileTokens, CompilerError> {
    // About 1/6 of the source code seems to be tokens roughly from some very small preliminary tests
    let initial_capacity = source_code.len() / settings::SRC_TO_TOKEN_RATIO;

    let mut template_nesting_level: i64 = if mode == TokenizeMode::Normal {
        0
    } else {
        // This is so .mt files or the repl can't break out of the template head/body when starting there
        i64::MAX / 2
    };

    let mut tokens: Vec<Token> = Vec::with_capacity(initial_capacity);
    let mut stream = TokenStream::new(source_code, src_path, mode);

    let mut token: Token = Token::new(
        TokenKind::ModuleStart(String::new()),
        TextLocation::default(),
    );

    loop {
        token_log!(#token);

        if token.kind == TokenKind::Eof {
            break;
        }

        tokens.push(token);
        token = get_token_kind(&mut stream, &mut template_nesting_level, string_table)?;
    }

    tokens.push(token);

    // First creation of TokenContext
    Ok(FileTokens::new(src_path.to_owned(), tokens))
}

pub fn get_token_kind(
    stream: &mut TokenStream,
    template_nesting_level: &mut i64,
    string_table: &mut StringTable,
) -> Result<Token, CompilerError> {
    let mut current_char = match stream.next() {
        Some(ch) => ch,
        None => return_token!(TokenKind::Eof, stream),
    };

    let mut token_value: String = String::new();

    // Check for raw strings (backticks)
    // Also used in scenes for raw outputs
    if current_char == '`' {
        while let Some(ch) = stream.next() {
            if ch == '`' {
                let interned_string = string_table.intern(&token_value);
                return_token!(TokenKind::RawStringLiteral(interned_string), stream);
            }

            token_value.push(ch);
        }

        // If we reach here, the raw string was not terminated
        return_syntax_error!(
            "Unterminated raw string literal - missing closing backtick",
            stream.new_location().to_error_location(&string_table),
            {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Add closing backtick at the end of the raw string",
                SuggestedInsertion => "`",
                SuggestedLocation => "at end of raw string",
            }
        )
    }

    if stream.mode == TokenizeMode::TemplateBody && current_char != ']' && current_char != '[' {
        return tokenize_template_body(current_char, stream, string_table);
    }

    // Whitespace
    while current_char.is_whitespace() {
        if current_char == '\n' {
            // Skip any whitespace after this before returning it to save on tokens.
            // There is no semantic reason that the parser needs to distinguish multiple newlines.
            // Scene Bodies are already parsed separately above this.
            while let Some(next_char) = stream.peek() {
                if next_char.is_whitespace() {
                    stream.next();
                } else {
                    break;
                }
            }

            return_token!(TokenKind::Newline, stream);
        } else if current_char == '\r' {
            if stream.peek() == Some(&'\n') {
                stream.next();

                while let Some(next_char) = stream.peek() {
                    if next_char.is_whitespace() {
                        stream.next();
                    } else {
                        break;
                    }
                }

                return_token!(TokenKind::Newline, stream);
            } else {
                // Count as a newline?
                // This should maybe be a warning or something in the future as this is weird
                current_char = match stream.next() {
                    Some(ch) => ch,
                    None => return_token!(TokenKind::Newline, stream),
                };
            }
        } else {
            current_char = match stream.next() {
                Some(ch) => ch,
                None => return_token!(TokenKind::Eof, stream),
            };
        }
    }

    // To ignore leading whitespace for the next token position
    stream.update_start_position();

    if current_char == '[' {
        *template_nesting_level += 1;
        match stream.mode {
            TokenizeMode::TemplateHead => {
                return_syntax_error!(
                    "Cannot have nested templates inside of a template head, must be inside the template body. \
                    Use a colon to start the template body.",
                    stream.new_location().to_error_location(string_table),
                    {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Add ':' after the template head to start the template body",
                        SuggestedInsertion => ":",
                    }
                )
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

        // ::
        if let Some(&next_char) = stream.peek() {
            if next_char == ':' {
                stream.next();

                return_token!(TokenKind::DoubleColon, stream);
            }
        }

        return_token!(TokenKind::Colon, stream);
    }

    if current_char == END_SCOPE_CHAR {
        return_token!(TokenKind::End, stream);
    }

    // Check for string literals
    if current_char == '"' {
        return tokenize_string(stream, string_table);
    }

    // Check for character literals
    if current_char == '\'' {
        if let Some(c) = stream.next() {
            if let Some(&char_after_next) = stream.peek()
                && char_after_next == '\''
            {
                stream.next(); // Consume the closing quote
                return_token!(TokenKind::CharLiteral(c), stream);
            }
        };

        // If not correct declaration of char
        return_syntax_error!(
            format!("Expected a character after the single quote in a char literal. Found {current_char}"),
            stream.new_location().to_error_location(&string_table),
            {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Character literals must be exactly one character between single quotes",
                SuggestedReplacement => "'x'",
            }
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

            // This represents a slot in the template head
            if stream.mode == TokenizeMode::TemplateHead {
                return_token!(TokenKind::Slot, stream);
            }

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
                return get_token_kind(stream, template_nesting_level, string_table);

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
    // These are special compiler_frontend commands that start with a hash
    // They are always followed by a stringslice literal
    if current_char == '#' {
        return compiler_directive(&mut token_value, stream, &string_table);
    }

    // Used for imports at the top level
    if current_char == '@' {
        // if stream.mode == TokenizeMode::TemplateHead {
        //     while let Some(&next_char) = stream.peek() {
        //         if next_char.is_alphanumeric() || next_char == '_' {
        //             token_value.push(stream.next().unwrap());
        //             continue;
        //         }
        //         break;
        //     }
        //     let interned = string_table.intern(&token_value);
        //     return_token!(TokenKind::Id(interned), stream);
        // }

        stream.next();

        return parse_imports(stream, string_table);
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
                        "Can't have more than one decimal point in a number",
                        stream.new_location().to_error_location(&string_table),
                        {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Remove extra decimal points from the number",
                        }
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
        return keyword_or_variable(&mut token_value, stream, string_table);
    }

    return_syntax_error!(
        format!("Invalid Token Used: '{}' this is not recognised or supported by the compiler_frontend", current_char),
        stream.new_location().to_error_location(&string_table),
        {
            CompilationStage => "Tokenization",
            PrimarySuggestion => "Check for typos or unsupported characters",
        }
    )
}

fn keyword_or_variable(
    token_value: &mut String,
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<Token, CompilerError> {
    // Match variables or keywords
    loop {
        if let Some(char) = stream.peek()
            && is_valid_var_char(char)
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
            "yield" => return_token!(TokenKind::Yield, stream),
            "else" => return_token!(TokenKind::Else, stream),
            "for" => return_token!(TokenKind::For, stream),
            "break" => return_token!(TokenKind::Break, stream),
            "continue" => return_token!(TokenKind::Continue, stream),
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
            "Fn" => return_token!(TokenKind::DatatypeFalse, stream),

            "Float" => return_token!(TokenKind::DatatypeFloat, stream),
            "Int" => return_token!(TokenKind::DatatypeInt, stream),
            "String" => return_token!(TokenKind::DatatypeString, stream),
            "Bool" => return_token!(TokenKind::DatatypeBool, stream),

            "None" => return_token!(TokenKind::DatatypeNone, stream),

            _ => {}
        }

        // VARIABLE
        if is_valid_identifier(token_value) {
            let interned_symbol = string_table.intern(token_value);
            return_token!(TokenKind::Symbol(interned_symbol), stream);
        } else {
            // Failing all of that, this is an invalid variable name
            return_syntax_error!(
                format!("Invalid variable name or keyword: '{}'", token_value),
                stream.new_location().to_error_location(&string_table),
                {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Variable names must start with a letter or underscore and contain only alphanumeric characters or underscores",
                }
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

fn tokenize_string(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<Token, CompilerError> {
    let mut token_value = String::new();

    // Currently should be at the character that started the String
    while let Some(ch) = stream.next() {
        // Check for escape characters
        if ch == '\\' {
            if let Some(next_char) = stream.next() {
                token_value.push(next_char);
            }
        } else if ch == '"' {
            let interned_string = string_table.intern(&token_value);
            return_token!(TokenKind::StringSliceLiteral(interned_string), stream);
        }

        token_value.push(ch);
    }

    // If we reach here, the string was not terminated
    return_syntax_error!(
        "Unterminated string literal - missing closing quote",
        stream.new_location().to_error_location(string_table),
        {
            CompilationStage => "Tokenization",
            PrimarySuggestion => "Add closing double quote at the end of the string",
            SuggestedInsertion => "\"",
            SuggestedLocation => "at end of string",
        }
    )
}

fn tokenize_template_body(
    current_char: char,
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<Token, CompilerError> {
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
            let interned_string = string_table.intern(&token_value);
            return_token!(TokenKind::StringSliceLiteral(interned_string), stream);
        }

        // Should always be a valid char
        token_value.push(stream.next().unwrap());
    }

    let interned_string = string_table.intern(&token_value);
    return_token!(TokenKind::StringSliceLiteral(interned_string), stream);
}
