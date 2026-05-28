//! Template-aware lexer for raw Beanstalk source text.
//!
//! WHAT: converts source text into token streams while switching modes for templates, strings, and directives.
//! WHY: lexing owns the first precise source-location mapping and all delimiter-balancing rules;
//! callers can run it against worker-local string tables before deterministic module aggregation.
#![allow(clippy::result_large_err)]

use crate::compiler_frontend::basic_utility_functions::CharacterParsing;
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::keywords::{
    is_identifier_continue, is_valid_identifier, keyword_token_kind,
};
use crate::compiler_frontend::paths::const_paths::parse_file_path;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::identity::FileId;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::newline_handling::normalize_consumed_carriage_return_newline;
use crate::compiler_frontend::tokenizer::numeric::tokenize_numeric_literal;
use crate::compiler_frontend::tokenizer::text_modes::{
    tokenize_code_template_body, tokenize_discard_template_body, tokenize_raw_string,
    tokenize_string, tokenize_template_body,
};
use crate::compiler_frontend::tokenizer::tokens::{
    FileTokens, SourceLocation, TemplateBodyMode, Token, TokenKind, TokenStream, TokenizeMode,
};
use crate::projects::settings;
use crate::token_log;

pub const END_SCOPE_CHAR: char = ';';

#[macro_export]
macro_rules! return_token {
    ($kind:expr, $stream:expr $(,)?) => {
        return Ok(Token::new($kind, $stream.new_location()))
    };
}

/// Tokenize one source file and optionally attach stable file identity metadata.
///
/// WHAT: wraps lexing output in `FileTokens` carrying both logical path and optional `FileId`.
/// WHY: later frontend stages should prefer explicit file identity over path string comparisons.
pub fn tokenize(
    source_code: &str,
    src_path: &InternedPath,
    mode: TokenizeMode,
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
    file_id: Option<FileId>,
) -> Result<FileTokens, CompilerDiagnostic> {
    // WHY: Estimating token capacity reduces reallocations for large files.
    // Preliminary tests suggest a ratio of roughly 6 characters per token.
    let initial_capacity = source_code.len() / settings::SRC_TO_TOKEN_RATIO;

    let mut tokens: Vec<Token> = Vec::with_capacity(initial_capacity);
    let mut stream = TokenStream::new(source_code, src_path, mode);

    let mut token: Token = Token::new(TokenKind::ModuleStart, SourceLocation::default());

    loop {
        token_log!(#token);

        if token.kind == TokenKind::Eof {
            break;
        }

        tokens.push(token);
        token = get_token_kind(&mut stream, style_directives, string_table)?;
    }

    tokens.push(token);

    Ok(FileTokens::new_with_file_id(
        src_path.to_owned(),
        file_id,
        tokens,
    ))
}

pub fn get_token_kind(
    stream: &mut TokenStream<'_>,
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
) -> Result<Token, CompilerDiagnostic> {
    // WHY: Comments do not produce tokens. A labeled loop allows the comment handler
    // to restart tokenization with `continue` instead of a recursive call, preventing
    // stack overflow in files with deep comment blocks.
    'next_token: loop {
        let mut current_char = match stream.next() {
            Some(ch) => ch,
            None => return_token!(TokenKind::Eof, stream),
        };

        let mut token_value = String::new();

        // -----------------
        //  Template bodies
        // -----------------

        // Template bodies are tokenized as "mostly raw text" so the body parser can
        // treat everything between delimiters as string content unless a nested
        // template begins or the current template closes.
        if stream.mode == TokenizeMode::TemplateBody {
            match stream.current_template_body_mode() {
                TemplateBodyMode::Balanced => {
                    return tokenize_code_template_body(current_char, stream, string_table);
                }
                TemplateBodyMode::DiscardBalanced => {
                    return tokenize_discard_template_body(current_char, stream);
                }
                TemplateBodyMode::Normal => {
                    if current_char != ']' && current_char != '[' {
                        return tokenize_template_body(current_char, stream, string_table);
                    }
                }
            }
        }

        // -----------------------
        //  Raw strings (backticks)
        // -----------------------

        // Raw strings are used for pre-formatted text and raw template outputs.
        if current_char == '`' {
            return tokenize_raw_string(stream, string_table);
        }

        // ------------
        //  Whitespace
        // ------------

        while current_char.is_whitespace() {
            if current_char == '\n' {
                // Skip trailing whitespace after a newline to reduce redundant tokens.
                // The parser treats consecutive newlines as a single boundary.
                consume_all_whitespace(stream);
                return_token!(TokenKind::Newline, stream);
            } else if current_char == '\r' {
                let _ = normalize_consumed_carriage_return_newline(stream);
                consume_all_whitespace(stream);
                return_token!(TokenKind::Newline, stream);
            } else {
                current_char = match stream.next() {
                    Some(ch) => ch,
                    None => return_token!(TokenKind::Eof, stream),
                };
            }
        }

        // Ignore leading whitespace for the next token's source location.
        stream.update_start_position();

        // ---------------------
        //  Template delimiters
        // ---------------------

        if current_char == '[' {
            // Nested templates begin with '[' and switch to TemplateHead mode.
            stream.push_template_mode(TokenizeMode::TemplateHead);
            return_token!(TokenKind::TemplateHead, stream);
        }

        if current_char == ']' {
            // Closing a template restores the parent template's mode.
            stream.pop_template_mode();
            return_token!(TokenKind::TemplateClose, stream);
        }

        // Colon handling: StartTemplateBody (:) vs DoubleColon (::) vs Colon (:)
        if current_char == ':' {
            if stream.mode == TokenizeMode::TemplateHead {
                stream.set_current_template_mode(TokenizeMode::TemplateBody);
                return_token!(TokenKind::StartTemplateBody, stream);
            }

            if let Some(&next_char) = stream.peek()
                && next_char == ':'
            {
                stream.next();
                return_token!(TokenKind::DoubleColon, stream);
            }

            return_token!(TokenKind::Colon, stream);
        }

        // ------------------
        //  Style directives
        // ------------------

        if current_char == '$' {
            return tokenize_style_directive(stream, style_directives, string_table);
        }

        if current_char == END_SCOPE_CHAR {
            return_token!(TokenKind::End, stream);
        }

        // ------------------
        //  String literals
        // ------------------

        if current_char == '"' {
            return tokenize_string(stream, string_table);
        }

        if current_char == '\'' {
            if let Some(c) = stream.next()
                && let Some(&char_after_next) = stream.peek()
                && char_after_next == '\''
            {
                stream.next(); // Consume closing quote
                return_token!(TokenKind::CharLiteral(c), stream);
            };

            return Err(CompilerDiagnostic::invalid_char_literal(
                stream.new_location(),
            ));
        }

        // -----------------
        //  Basic operators
        // -----------------

        if current_char == '(' {
            return_token!(TokenKind::OpenParenthesis, stream);
        }

        if current_char == ')' {
            return_token!(TokenKind::CloseParenthesis, stream);
        }

        if current_char == '=' {
            if let Some(&next_char) = stream.peek()
                && next_char == '>'
            {
                stream.next();
                return_token!(TokenKind::FatArrow, stream);
            }

            return_token!(TokenKind::Assign, stream);
        }

        if current_char == ',' {
            return_token!(TokenKind::Comma, stream);
        }

        if current_char == '.' {
            if let Some(&peeked_char) = stream.peek()
                && peeked_char == '.'
            {
                stream.next();
                return_token!(TokenKind::Variadic, stream);
            }

            return_token!(TokenKind::Dot, stream);
        }

        if current_char == '{' {
            return_token!(TokenKind::OpenCurly, stream);
        }

        if current_char == '}' {
            return_token!(TokenKind::CloseCurly, stream);
        }

        if current_char == '|' {
            return_token!(TokenKind::TypeParameterBracket, stream);
        }

        if current_char == '!' {
            return_token!(TokenKind::Bang, stream);
        }

        if current_char == '?' {
            return_token!(TokenKind::QuestionMark, stream);
        }

        // ----------------------------
        //  Subtraction & Line comments
        // ----------------------------

        if current_char == '-'
            && let Some(&next_char) = stream.peek()
        {
            // Line comments (--)
            if next_char == '-' {
                stream.next();

                while let Some(ch) = stream.peek() {
                    if ch == &'\n' || ch == &'\r' {
                        break;
                    }

                    stream.next();
                }

                // WHY: Comments do not produce tokens. Loop back to lex the next item.
                continue 'next_token;
            }

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

        // ------------------------
        //  Mathematical operators
        // ------------------------

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
                // Integer division (//)
                if next_char == '/' {
                    stream.next();

                    if let Some(&next_next_char) = stream.peek()
                        && next_next_char == '='
                    {
                        stream.next();
                        return_token!(TokenKind::IntDivideAssign, stream);
                    }
                    return_token!(TokenKind::IntDivide, stream);
                }

                // Divide assign (/=)
                if next_char == '=' {
                    stream.next();
                    return_token!(TokenKind::DivideAssign, stream);
                }
            }

            return_token!(TokenKind::Divide, stream);
        }

        if current_char == '%' {
            if let Some(&next_char) = stream.peek()
                && next_char == '='
            {
                stream.next();
                return_token!(TokenKind::ModulusAssign, stream);
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

        // ------------------
        //  Logic & Channels
        // ------------------

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

        if current_char == '&' {
            return_token!(TokenKind::Ampersand, stream);
        }

        // ----------------------
        //  Identifiers & Values
        // ----------------------

        // Paths (@/path)
        if current_char == '@' {
            return parse_file_path(stream, string_table);
        }

        // Wildcard or Identifier starting with '_'
        if current_char == '_' {
            if let Some(next_char) = stream.peek()
                && is_identifier_continue(*next_char)
            {
                token_value.push(current_char);
                return tokenize_identifier_or_keyword(&mut token_value, stream, string_table);
            }

            return_token!(TokenKind::Wildcard, stream);
        }

        // Numeric literals
        if current_char.is_numeric() {
            return tokenize_numeric_literal(current_char, stream, string_table);
        }

        // Keywords or variables starting with a letter
        if current_char.is_alphabetic() {
            token_value.push(current_char);
            return tokenize_identifier_or_keyword(&mut token_value, stream, string_table);
        }

        return Err(CompilerDiagnostic::invalid_character(
            current_char,
            stream.new_location(),
        ));
    } // 'next_token loop
}

fn tokenize_style_directive(
    stream: &mut TokenStream<'_>,
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
) -> Result<Token, CompilerDiagnostic> {
    if stream.mode != TokenizeMode::TemplateHead {
        return Err(CompilerDiagnostic::invalid_character(
            '$',
            stream.new_location(),
        ));
    }

    let Some(&first_char) = stream.peek() else {
        return Err(CompilerDiagnostic::unexpected_end_of_file(
            None,
            stream.new_location(),
        ));
    };

    if !first_char.is_alphabetic() && first_char != '_' {
        return Err(CompilerDiagnostic::invalid_character(
            first_char,
            stream.new_location(),
        ));
    }

    let mut directive_text = String::new();
    let first_directive_char = advance_after_peek(
        stream,
        "Tokenizer validated a style directive name but failed to consume its first character.",
    );
    directive_text.push(first_directive_char);

    while let Some(&next_char) = stream.peek() {
        if !is_identifier_continue(next_char) {
            break;
        }

        let directive_char = advance_after_peek(
            stream,
            "Tokenizer peeked a style directive character but could not advance the stream.",
        );
        directive_text.push(directive_char);
    }

    let directive = string_table.intern(&directive_text);
    let Some(body_mode) = style_directives.body_mode_for(&directive_text) else {
        // Intern the supported-directives list for the error diagnostic payload.
        // This is diagnostic-only string-table mutation.
        let supported =
            string_table.intern(&style_directives.supported_directives_for_diagnostic());
        return Err(CompilerDiagnostic::invalid_style_directive(
            directive,
            supported,
            stream.new_location(),
        ));
    };

    stream.mark_current_template_body_mode(body_mode);
    return_token!(TokenKind::StyleDirective(directive), stream);
}

// ----------------------
//  Variables & Keywords
// ----------------------

pub(crate) fn tokenize_identifier_or_keyword(
    token_value: &mut String,
    stream: &mut TokenStream<'_>,
    string_table: &mut StringTable,
) -> Result<Token, CompilerDiagnostic> {
    // WHY: Variable names and keywords can contain alphanumeric characters or underscores.
    // We consume the entire identifier before deciding if it's a keyword or a symbol.
    loop {
        if let Some(char) = stream.peek()
            && is_identifier_continue(*char)
        {
            let identifier_char = advance_after_peek(
                stream,
                "Tokenizer peeked an identifier character but could not advance the stream.",
            );
            token_value.push(identifier_char);
            continue;
        }

        if let Some(keyword_kind) = keyword_token_kind(token_value.as_str()) {
            return_token!(keyword_kind, stream);
        }

        if is_valid_identifier(token_value) {
            let interned_symbol = string_table.intern(token_value);
            return_token!(TokenKind::Symbol(interned_symbol), stream);
        }

        return Err(CompilerDiagnostic::invalid_identifier(
            stream.new_location(),
        ));
    }
}

// -----------
//  Utilities
// -----------

/// Advance after a successful `peek` in tokenizer loops.
///
/// WHAT: Several lexer paths inspect the next character before consuming it so
/// they can decide whether the character belongs to the current token. Once
/// `peek` has returned `Some`, `next` returning `None` would mean the stream
/// invariant is broken, not that user source is malformed.
fn advance_after_peek(stream: &mut TokenStream<'_>, invariant_message: &'static str) -> char {
    stream.next().expect(invariant_message)
}

pub fn consume_non_newline_whitespace(stream: &mut TokenStream) -> bool {
    let mut consumed = false;

    while stream
        .peek()
        .is_some_and(|character| character.is_non_newline_whitespace())
    {
        stream.next();
        consumed = true;
    }

    consumed
}

pub fn consume_all_whitespace(stream: &mut TokenStream) -> bool {
    let mut consumed = false;

    while stream
        .peek()
        .is_some_and(|character| character.is_whitespace())
    {
        stream.next();
        consumed = true;
    }

    consumed
}

#[cfg(test)]
#[path = "tests/lexer_tests.rs"]
mod lexer_tests;
