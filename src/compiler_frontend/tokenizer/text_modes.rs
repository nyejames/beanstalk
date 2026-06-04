//! String and template-body tokenization.
//!
//! WHAT: owns lexing for raw strings, quoted strings, and template body text
//! modes that treat most source characters as string content.
//! WHY: these modes are delimiter-state machines rather than ordinary token
//! dispatch, so keeping them outside `lexer.rs` makes the main lexer easier to
//! audit.

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::newline_handling::{
    consume_pending_carriage_return_newline, normalize_consumed_carriage_return_newline,
};
use crate::compiler_frontend::tokenizer::tokens::{Token, TokenKind, TokenStream};
use crate::return_token;

pub(super) fn tokenize_raw_string(
    stream: &mut TokenStream<'_>,
    string_table: &mut StringTable,
) -> Result<Token, CompilerDiagnostic> {
    let mut token_value = String::new();

    while let Some(ch) = stream.next() {
        if ch == '`' {
            let interned_string = string_table.intern(&token_value);
            return_token!(TokenKind::RawStringLiteral(interned_string), stream);
        }

        if ch == '\r' {
            let normalized_char = normalize_consumed_carriage_return_newline(stream);
            token_value.push_str(normalized_char);
            continue;
        }

        token_value.push(ch);
    }

    Err(CompilerDiagnostic::unterminated_string_literal(
        stream.new_location(),
    ))
}

pub(super) fn tokenize_string(
    stream: &mut TokenStream<'_>,
    string_table: &mut StringTable,
) -> Result<Token, CompilerDiagnostic> {
    let mut token_value = String::new();

    while let Some(ch) = stream.next() {
        if ch == '\\' {
            if let Some(next_char) = stream.next() {
                token_value.push(next_char);
            }
            continue;
        }

        if ch == '"' {
            let interned_string = string_table.intern(&token_value);
            return_token!(TokenKind::StringSliceLiteral(interned_string), stream);
        }

        if ch == '\r' {
            let normalized_char = normalize_consumed_carriage_return_newline(stream);
            token_value.push_str(normalized_char);
            continue;
        }

        token_value.push(ch);
    }

    Err(CompilerDiagnostic::unterminated_string_literal(
        stream.new_location(),
    ))
}

pub(super) fn tokenize_template_body(
    current_char: char,
    stream: &mut TokenStream<'_>,
    string_table: &mut StringTable,
) -> Result<Token, CompilerDiagnostic> {
    let mut token_value = String::new();
    append_template_body_char(current_char, &mut token_value, stream);

    while let Some(ch) = stream.peek() {
        match ch {
            '\\' => {
                stream.next();
                append_template_body_escape(&mut token_value, stream);
            }

            '[' | ']' => {
                let interned_string = string_table.intern(&token_value);
                return_token!(TokenKind::StringSliceLiteral(interned_string), stream);
            }

            '\r' => {
                let normalized_char = consume_pending_carriage_return_newline(stream);
                token_value.push_str(normalized_char);
            }

            _ => {
                let next_char = advance_after_peek(
                    stream,
                    "Tokenizer peeked a template-body character but could not advance the stream.",
                );
                token_value.push(next_char);
            }
        }
    }

    let interned_string = string_table.intern(&token_value);
    return_token!(TokenKind::StringSliceLiteral(interned_string), stream);
}

fn append_template_body_char(
    current_char: char,
    token_value: &mut String,
    stream: &mut TokenStream<'_>,
) {
    match current_char {
        '\\' => append_template_body_escape(token_value, stream),
        '\r' => token_value.push_str(normalize_consumed_carriage_return_newline(stream)),
        _ => token_value.push(current_char),
    }
}

fn append_template_body_escape(token_value: &mut String, stream: &mut TokenStream<'_>) {
    match stream.next() {
        Some('\r') => token_value.push_str(normalize_consumed_carriage_return_newline(stream)),
        Some(escaped_char) => token_value.push(escaped_char),
        None => token_value.push('\\'),
    }
}

pub(super) fn tokenize_code_template_body(
    current_char: char,
    stream: &mut TokenStream<'_>,
    string_table: &mut StringTable,
) -> Result<Token, CompilerDiagnostic> {
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

        let next_char = advance_after_peek(
            stream,
            "Tokenizer peeked a code-template body character but could not advance the stream.",
        );

        append_code_template_body_char(next_char, &mut token_value, stream);
    }

    let interned_string = string_table.intern(&token_value);
    return_token!(TokenKind::StringSliceLiteral(interned_string), stream);
}

pub(super) fn tokenize_discard_template_body(
    current_char: char,
    stream: &mut TokenStream<'_>,
) -> Result<Token, CompilerDiagnostic> {
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

fn append_code_template_body_char(
    ch: char,
    token_value: &mut String,
    stream: &mut TokenStream<'_>,
) {
    match ch {
        '[' => stream.register_template_body_open_square_bracket(),
        ']' => stream.register_template_body_close_square_bracket(),
        '\r' => {
            let normalized_char = normalize_consumed_carriage_return_newline(stream);
            token_value.push_str(normalized_char);
            return;
        }
        _ => {}
    }

    token_value.push(ch);
}

/// Advance after a successful `peek` in text-mode tokenizer loops.
///
/// WHAT: text-mode tokenization peeks delimiter characters before deciding
/// whether to consume them.
/// WHY: if `peek` returned `Some`, a subsequent `next` returning `None` would be
/// an internal stream invariant failure, not malformed user source.
fn advance_after_peek(stream: &mut TokenStream<'_>, invariant_message: &'static str) -> char {
    stream.next().expect(invariant_message)
}
