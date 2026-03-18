use crate::compiler_frontend::basic_utility_functions::is_valid_var_char;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::paths::parse_file_path;
use crate::compiler_frontend::tokenizer::tokens::{
    FileTokens, TemplateBodyMode, TextLocation, Token, TokenKind, TokenStream, TokenizeMode,
};
use crate::projects::settings;
use crate::{return_syntax_error, token_log};

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

    let mut tokens: Vec<Token> = Vec::with_capacity(initial_capacity);
    let mut stream = TokenStream::new(source_code, src_path, mode);

    let mut token: Token = Token::new(TokenKind::ModuleStart, TextLocation::default());

    loop {
        token_log!(#token);

        if token.kind == TokenKind::Eof {
            break;
        }

        tokens.push(token);
        token = get_token_kind(&mut stream, string_table)?;
    }

    tokens.push(token);

    // First creation of TokenContext
    Ok(FileTokens::new(src_path.to_owned(), tokens))
}

pub fn get_token_kind(
    stream: &mut TokenStream<'_>,
    string_table: &mut StringTable,
) -> Result<Token, CompilerError> {
    let mut current_char = match stream.next() {
        Some(ch) => ch,
        None => return_token!(TokenKind::Eof, stream),
    };

    let mut token_value: String = String::new();

    // Template bodies are intentionally tokenized as "mostly raw text" so the body
    // parser can treat everything between delimiters as string content unless a new
    // nested template begins or the current template closes.
    if stream.mode == TokenizeMode::TemplateBody {
        match stream.current_template_body_mode() {
            TemplateBodyMode::CodeBalanced | TemplateBodyMode::CssBalanced => {
                return tokenize_code_template_body(current_char, stream, string_table);
            }
            TemplateBodyMode::DiscardBalanced => {
                return tokenize_discard_template_body(current_char, stream);
            }
            TemplateBodyMode::DocBalanced | TemplateBodyMode::Normal => {
                if current_char != ']' && current_char != '[' {
                    return tokenize_template_body(current_char, stream, string_table);
                }
            }
        }
    }

    // Check for raw strings (backticks)
    // Also used in templates for raw outputs
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
            stream.new_location().to_error_location(string_table),
            {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Add closing backtick at the end of the raw string",
                SuggestedInsertion => "`",
                SuggestedLocation => "at end of raw string",
            }
        )
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
        // Start a fresh nested template and remember that we are now parsing
        // that nested template's head.
        stream.push_template_mode(TokenizeMode::TemplateHead);
        return_token!(TokenKind::TemplateHead, stream);
    }

    if current_char == ']' {
        // Closing a template restores whatever mode the parent template was in
        // (normal code, template head, or template body).
        stream.pop_template_mode();
        return_token!(TokenKind::TemplateClose, stream);
    }

    // Check if going into the template body
    if current_char == ':' {
        if stream.mode == TokenizeMode::TemplateHead {
            stream.set_current_template_mode(TokenizeMode::TemplateBody);

            return_token!(TokenKind::StartTemplateBody, stream);
        }

        // ::
        if let Some(&next_char) = stream.peek()
            && next_char == ':'
        {
            stream.next();

            return_token!(TokenKind::DoubleColon, stream);
        }

        return_token!(TokenKind::Colon, stream);
    }

    if current_char == '$' {
        if stream.mode != TokenizeMode::TemplateHead {
            return_syntax_error!(
                "The '$' style directive syntax is only valid inside template heads.",
                stream.new_location().to_error_location(string_table),
                {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Move this '$' directive into a template head or remove it",
                }
            )
        }

        let Some(&first_char) = stream.peek() else {
            return_syntax_error!(
                "Expected a style directive name after '$'.",
                stream.new_location().to_error_location(string_table),
                {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Use '$markdown', '$children(..)', '$ignore', '$slot', '$insert(..)', '$note', '$todo', '$doc', '$code', '$css', or '$formatter(...)' inside the template head",
                }
            )
        };

        if !first_char.is_alphabetic() && first_char != '_' {
            return_syntax_error!(
                "Expected a style directive name immediately after '$'.",
                stream.new_location().to_error_location(string_table),
                {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Write the directive without whitespace, for example '$markdown'",
                }
            )
        }

        token_value.push(
            stream
                .next()
                .expect("validated style directive should still expose its first identifier char"),
        );

        while let Some(&next_char) = stream.peek() {
            if !is_valid_var_char(&next_char) {
                break;
            }

            token_value.push(
                stream
                    .next()
                    .expect("peeked style directive character should remain available"),
            );
        }

        let directive = string_table.intern(&token_value);
        match token_value.as_str() {
            "code" => stream.mark_current_template_body_mode(TemplateBodyMode::CodeBalanced),
            "css" => stream.mark_current_template_body_mode(TemplateBodyMode::CssBalanced),
            "note" | "todo" => {
                stream.mark_current_template_body_mode(TemplateBodyMode::DiscardBalanced)
            }
            "doc" => stream.mark_current_template_body_mode(TemplateBodyMode::DocBalanced),
            _ => {}
        }
        // The parser validates which directives are currently supported. The lexer
        // only has to preserve the directive identifier as a distinct token.
        return_token!(TokenKind::StyleDirective(directive), stream);
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
        if let Some(c) = stream.next()
            && let Some(&char_after_next) = stream.peek()
            && char_after_next == '\''
        {
            stream.next(); // Consume the closing quote
            return_token!(TokenKind::CharLiteral(c), stream);
        };

        // If not correct declaration of char
        return_syntax_error!(
            format!("Expected a character after the single quote in a char literal. Found {current_char}"),
            stream.new_location().to_error_location(string_table),
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
        // =>
        if let Some(&next_char) = stream.peek()
            && next_char == '>'
        {
            stream.next();
            return_token!(TokenKind::CreateChannel, stream);
        }

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
    if current_char == '-'
        && let Some(&next_char) = stream.peek()
    {
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
            return get_token_kind(stream, string_table);
        }

        // Subtraction / Negative / Return / Subtract Assign
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

    // Mathematical operators
    // must peak ahead to check for exponentiation (**) or roots (//) and assign variations
    if current_char == '+' {
        if let Some(&next_char) = stream.peek()
            && next_char == '='
        {
            stream.next();
            return_token!(TokenKind::AddAssign, stream);
        }

        return_token!(TokenKind::Add, stream);
    }

    if current_char == '*' {
        if let Some(&next_char) = stream.peek()
            && next_char == '='
        {
            stream.next();
            return_token!(TokenKind::MultiplyAssign, stream);
        }

        return_token!(TokenKind::Multiply, stream);
    }

    if current_char == '/' {
        if let Some(&next_char) = stream.peek() {
            if next_char == '/' {
                stream.next();

                if let Some(&next_next_char) = stream.peek()
                    && next_next_char == '='
                {
                    stream.next();
                    return_token!(TokenKind::RootAssign, stream);
                }
                return_token!(TokenKind::Root, stream);
            }

            if next_char == '=' {
                stream.next();
                return_token!(TokenKind::DivideAssign, stream);
            }
        }

        return_token!(TokenKind::Divide, stream);
    }

    if current_char == '%' {
        if let Some(&next_char) = stream.peek() {
            if next_char == '=' {
                stream.next();
                return_token!(TokenKind::ModulusAssign, stream);
            }

            if next_char == '%' {
                stream.next();
                if let Some(&next_next_char) = stream.peek()
                    && next_next_char == '='
                {
                    stream.next();
                    return_token!(TokenKind::RemainderAssign, stream);
                }
                return_token!(TokenKind::Remainder, stream);
            }
        }

        return_token!(TokenKind::Modulus, stream);
    }

    if current_char == '^' {
        if let Some(&next_char) = stream.peek()
            && next_char == '='
        {
            stream.next();
            return_token!(TokenKind::ExponentAssign, stream);
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

            if next_char == '>' {
                stream.next();
                return_token!(TokenKind::ChannelSend, stream);
            }
        }

        return_token!(TokenKind::GreaterThan, stream);
    }

    if current_char == '<' {
        if let Some(&next_char) = stream.peek() {
            if next_char == '=' {
                stream.next();
                return_token!(TokenKind::LessThanOrEqual, stream);
            }

            if next_char == '<' {
                stream.next();
                return_token!(TokenKind::ChannelReceive, stream);
            }
        }

        return_token!(TokenKind::LessThan, stream);
    }

    if current_char == '~' {
        return_token!(TokenKind::Mutable, stream);
    }

    if current_char == '#' {
        return_token!(TokenKind::Hash, stream);
    }

    // Path
    if current_char == '@' {
        return parse_file_path(stream, string_table);
    }

    // Wildcard for pattern matching
    if current_char == '_' {
        return_token!(TokenKind::Wildcard, stream);
    }

    // Numbers
    if current_char.is_numeric() {
        token_value.push(current_char);
        let mut has_decimal_point = false;
        let mut saw_digit_after_decimal = false;
        let mut last_segment_was_digit = true;

        while let Some(&next_char) = stream.peek() {
            if next_char == '_' {
                if !last_segment_was_digit {
                    return_syntax_error!(
                        "Numeric separators must appear between digits",
                        stream.new_location().to_error_location(string_table),
                        {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Place underscores only between digits in numeric literals",
                        }
                    )
                }

                let _ = stream.next();
                last_segment_was_digit = false;
                continue;
            }

            if next_char == '.' {
                // TODO: need to handle range operator without backtracking through token stream
                // Or consuming too many dots.

                if has_decimal_point {
                    return_syntax_error!(
                        "Can't have more than one decimal point in a number",
                        stream.new_location().to_error_location(string_table),
                        {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Remove extra decimal points from the number",
                        }
                    )
                }

                if !last_segment_was_digit {
                    return_syntax_error!(
                        "A decimal point must follow a digit",
                        stream.new_location().to_error_location(string_table),
                        {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Remove the separator before the decimal point",
                        }
                    )
                }

                has_decimal_point = true;
                last_segment_was_digit = false;

                let Some(dot) = stream.next() else {
                    return Err(CompilerError::compiler_error(
                        "Tokenizer peeked a decimal point but could not advance the stream.",
                    ));
                };
                token_value.push(dot);
                continue;
            }

            if next_char.is_numeric() {
                let Some(digit) = stream.next() else {
                    return Err(CompilerError::compiler_error(
                        "Tokenizer peeked a numeric character but could not advance the stream.",
                    ));
                };
                token_value.push(digit);
                last_segment_was_digit = true;
                if has_decimal_point {
                    saw_digit_after_decimal = true;
                }
            } else {
                break;
            }
        }

        if !last_segment_was_digit {
            return_syntax_error!(
                "Number literals must end with a digit",
                stream.new_location().to_error_location(string_table),
                {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Remove the trailing separator or add a digit after the decimal point",
                }
            )
        }

        if has_decimal_point && !saw_digit_after_decimal {
            return_syntax_error!(
                "Float literals must include digits after the decimal point",
                stream.new_location().to_error_location(string_table),
                {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Add at least one digit after the decimal point",
                }
            )
        }

        if !has_decimal_point {
            let parsed_value = token_value.parse::<i64>().map_err(|error| {
                CompilerError::new_syntax_error(
                    format!("Invalid integer literal '{token_value}': {error}"),
                    stream.new_location().to_error_location(string_table),
                )
            })?;
            return_token!(TokenKind::IntLiteral(parsed_value), stream);
        }

        let parsed_value = token_value.parse::<f64>().map_err(|error| {
            CompilerError::new_syntax_error(
                format!("Invalid float literal '{token_value}': {error}"),
                stream.new_location().to_error_location(string_table),
            )
        })?;
        return_token!(TokenKind::FloatLiteral(parsed_value), stream);
    }

    if current_char.is_alphabetic() {
        token_value.push(current_char);
        return keyword_or_variable(&mut token_value, stream, string_table);
    }

    return_syntax_error!(
        format!("Invalid Token Used: '{}' this is not recognised or supported by the compiler_frontend", current_char),
        stream.new_location().to_error_location(string_table),
        {
            CompilationStage => "Tokenization",
            PrimarySuggestion => "Check for typos or unsupported characters",
        }
    )
}

pub(crate) fn keyword_or_variable(
    token_value: &mut String,
    stream: &mut TokenStream<'_>,
    string_table: &mut StringTable,
) -> Result<Token, CompilerError> {
    // Match variables or keywords
    loop {
        if let Some(char) = stream.peek()
            && is_valid_var_char(char)
        {
            token_value.push(
                stream
                    .next()
                    .expect("peeked identifier character should remain available"),
            );
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
            "import" => return_token!(TokenKind::Import, stream),

            // Control Flow
            // END_KEYWORD => return_token!(TokenKind::End, stream),
            "if" => return_token!(TokenKind::If, stream),
            "return" => return_token!(TokenKind::Return, stream),
            "yield" => return_token!(TokenKind::Yield, stream),
            "else" => return_token!(TokenKind::Else, stream),
            "as" => return_token!(TokenKind::As, stream),
            "copy" => return_token!(TokenKind::Copy, stream),

            // Loops
            "loop" => return_token!(TokenKind::Loop, stream),
            "in" => return_token!(TokenKind::In, stream),
            "to" => return_token!(TokenKind::ExclusiveRange, stream),
            "upto" => return_token!(TokenKind::InclusiveRange, stream),
            "by" => return_token!(TokenKind::By, stream),
            "break" => return_token!(TokenKind::Break, stream),
            "continue" => return_token!(TokenKind::Continue, stream),

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
                stream.new_location().to_error_location(string_table),
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
    stream: &mut TokenStream<'_>,
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
    stream: &mut TokenStream<'_>,
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
        token_value.push(
            stream
                .next()
                .expect("string tokenization loop should only consume available characters"),
        );
    }

    let interned_string = string_table.intern(&token_value);
    return_token!(TokenKind::StringSliceLiteral(interned_string), stream);
}

fn tokenize_code_template_body(
    current_char: char,
    stream: &mut TokenStream<'_>,
    string_table: &mut StringTable,
) -> Result<Token, CompilerError> {
    // `$code` template bodies treat square brackets as literal code characters.
    // The template only closes when the running bracket counts become balanced.
    if current_char == ']' && stream.template_body_next_close_balances_brackets() {
        stream.register_template_body_close_square_bracket();
        stream.pop_template_mode();
        return_token!(TokenKind::TemplateClose, stream);
    }

    let mut token_value = String::new();
    append_code_template_body_char(current_char, &mut token_value, stream);

    while let Some(&ch) = stream.peek() {
        if ch == ']' && stream.template_body_next_close_balances_brackets() {
            break;
        }

        let Some(next_char) = stream.next() else {
            return Err(CompilerError::compiler_error(
                "Tokenizer peeked a code-template body character but could not advance the stream.",
            ));
        };

        append_code_template_body_char(next_char, &mut token_value, stream);
    }

    let interned_string = string_table.intern(&token_value);
    return_token!(TokenKind::StringSliceLiteral(interned_string), stream);
}

fn append_code_template_body_char(
    ch: char,
    token_value: &mut String,
    stream: &mut TokenStream<'_>,
) {
    match ch {
        '[' => stream.register_template_body_open_square_bracket(),
        ']' => stream.register_template_body_close_square_bracket(),
        _ => {}
    }

    token_value.push(ch);
}

fn tokenize_discard_template_body(
    current_char: char,
    stream: &mut TokenStream<'_>,
) -> Result<Token, CompilerError> {
    match current_char {
        '[' => stream.register_template_body_open_square_bracket(),
        ']' => {
            if stream.template_body_next_close_balances_brackets() {
                stream.register_template_body_close_square_bracket();
                stream.pop_template_mode();
                return_token!(TokenKind::TemplateClose, stream);
            }
            stream.register_template_body_close_square_bracket();
        }
        _ => {}
    }

    while let Some(&ch) = stream.peek() {
        match ch {
            '[' => {
                stream.next();
                stream.register_template_body_open_square_bracket();
            }
            ']' => {
                if stream.template_body_next_close_balances_brackets() {
                    stream.next();
                    stream.register_template_body_close_square_bracket();
                    stream.pop_template_mode();
                    return_token!(TokenKind::TemplateClose, stream);
                }
                stream.next();
                stream.register_template_body_close_square_bracket();
            }
            _ => {
                stream.next();
            }
        }
    }

    return_token!(TokenKind::Eof, stream)
}

#[cfg(test)]
#[path = "tests/lexer_tests.rs"]
mod lexer_tests;
