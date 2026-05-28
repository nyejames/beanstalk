//! Shared frontend token-scanning utilities.
//!
//! WHAT: centralizes reusable delimiter-depth and balanced-template scan helpers.
//! WHY: declaration parsing, multi-bind parsing, header parsing, expression
//! boundary scanning, and template parsing previously maintained duplicate depth
//! bookkeeping logic.
//!
//! This module owns generic scan mechanics only.
//! It does NOT own statement/feature semantics or diagnostics policy.

#![allow(clippy::result_large_err)]
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NestingDepth {
    parenthesis: usize,
    curly: usize,
    template: usize,
}

impl NestingDepth {
    pub(crate) fn is_top_level(self) -> bool {
        self.parenthesis == 0 && self.curly == 0 && self.template == 0
    }

    fn expected_closing_delimiter(self) -> Option<&'static str> {
        if self.template > 0 {
            Some("]")
        } else if self.parenthesis > 0 {
            Some(")")
        } else if self.curly > 0 {
            Some("}")
        } else {
            None
        }
    }

    pub(crate) fn step(&mut self, token_kind: &TokenKind) {
        match token_kind {
            TokenKind::OpenParenthesis => self.parenthesis = self.parenthesis.saturating_add(1),
            TokenKind::CloseParenthesis => {
                self.parenthesis = self.parenthesis.saturating_sub(1);
            }
            TokenKind::OpenCurly => self.curly = self.curly.saturating_add(1),
            TokenKind::CloseCurly => {
                self.curly = self.curly.saturating_sub(1);
            }
            TokenKind::TemplateHead => self.template = self.template.saturating_add(1),
            TokenKind::TemplateClose => {
                self.template = self.template.saturating_sub(1);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExpressionBoundaryDepth {
    parenthesis: usize,
    curly: usize,
}

impl ExpressionBoundaryDepth {
    pub(crate) fn is_top_level(self) -> bool {
        self.parenthesis == 0 && self.curly == 0
    }

    pub(crate) fn step(&mut self, token_kind: &TokenKind) {
        match token_kind {
            TokenKind::OpenParenthesis => self.parenthesis = self.parenthesis.saturating_add(1),
            TokenKind::CloseParenthesis => self.parenthesis = self.parenthesis.saturating_sub(1),
            TokenKind::OpenCurly => self.curly = self.curly.saturating_add(1),
            TokenKind::CloseCurly => self.curly = self.curly.saturating_sub(1),
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TemplateBalance {
    opened: usize,
    closed: usize,
}

impl TemplateBalance {
    pub(crate) fn with_opening_template() -> Self {
        Self {
            opened: 1,
            closed: 0,
        }
    }

    pub(crate) fn has_unclosed_templates(self) -> bool {
        self.opened > self.closed
    }

    pub(crate) fn step(&mut self, token_kind: &TokenKind) {
        match token_kind {
            TokenKind::TemplateHead => {
                self.opened = self.opened.saturating_add(1);
            }
            TokenKind::TemplateClose => {
                self.closed = self.closed.saturating_add(1);
            }
            _ => {}
        }
    }
}

pub(crate) fn collect_declaration_initializer_tokens(
    token_stream: &mut FileTokens,
    string_table: &mut StringTable,
) -> Result<Vec<Token>, CompilerDiagnostic> {
    let mut collected = Vec::new();
    let mut depth = NestingDepth::default();
    let mut catch_block_depth = 0usize;
    let mut catch_header_pending = false;
    let mut value_if_block_depth = 0usize;
    let mut value_if_header_pending = false;
    let mut initializer_closed_by_statement_block = false;

    while token_stream.index < token_stream.length {
        if initializer_closed_by_statement_block {
            break;
        }

        let token_kind = token_stream.current_token_kind().clone();
        let at_top_level =
            depth.is_top_level() && catch_block_depth == 0 && value_if_block_depth == 0;

        let continues_multiline_expression = if matches!(token_kind, TokenKind::Newline) {
            let prev_continues = collected
                .last()
                .is_some_and(|token: &Token| token.kind.continues_expression());
            let next_continues = token_stream
                .peek_next_token()
                .is_some_and(|next| next.continues_expression());
            prev_continues || next_continues
        } else {
            false
        };

        if at_top_level
            && matches!(
                token_kind,
                TokenKind::Comma | TokenKind::End | TokenKind::Eof
            )
        {
            break;
        }

        if at_top_level
            && matches!(token_kind, TokenKind::Newline)
            && !continues_multiline_expression
        {
            break;
        }

        if matches!(token_kind, TokenKind::Eof) && !at_top_level {
            let expected_delimiter = if catch_block_depth > 0 {
                ";"
            } else {
                depth.expected_closing_delimiter().unwrap_or("]")
            };
            // Intern the expected delimiter so the EOF diagnostic can name it.
            // This is diagnostic-only string-table mutation.
            return Err(CompilerDiagnostic::unexpected_end_of_file(
                Some(string_table.get_or_intern(expected_delimiter.to_owned())),
                token_stream.current_location(),
            ));
        }

        // Declaration initializers can end with receiver-owned statement blocks such as
        // `catch:` and value-producing `if ...:`. Their bodies belong to the initializer even
        // though they are statement-shaped, so newline termination is suspended until the
        // matching outer `;` is collected.
        if depth.is_top_level() {
            match token_kind {
                TokenKind::Catch => catch_header_pending = true,
                TokenKind::If if catch_block_depth == 0 => value_if_header_pending = true,
                TokenKind::Colon if catch_header_pending => {
                    catch_header_pending = false;
                    catch_block_depth = catch_block_depth.saturating_add(1);
                }
                TokenKind::Colon if catch_block_depth > 0 => {
                    catch_block_depth = catch_block_depth.saturating_add(1);
                }
                TokenKind::Colon if value_if_header_pending => {
                    value_if_header_pending = false;
                    value_if_block_depth = value_if_block_depth.saturating_add(1);
                }
                TokenKind::Colon if value_if_block_depth > 0 => {
                    value_if_block_depth = value_if_block_depth.saturating_add(1);
                }
                TokenKind::End if catch_block_depth > 0 => {
                    let closing_outer_catch_block = catch_block_depth == 1;
                    catch_block_depth = catch_block_depth.saturating_sub(1);
                    catch_header_pending = false;
                    initializer_closed_by_statement_block = closing_outer_catch_block;
                }
                TokenKind::End if value_if_block_depth > 0 => {
                    let closing_outer_value_if_block = value_if_block_depth == 1;
                    value_if_block_depth = value_if_block_depth.saturating_sub(1);
                    value_if_header_pending = false;
                    initializer_closed_by_statement_block = closing_outer_value_if_block;
                }
                TokenKind::Then | TokenKind::Arrow | TokenKind::Newline | TokenKind::Eof => {
                    catch_header_pending = false;
                    value_if_header_pending = false;
                }
                _ => {}
            }
        }

        depth.step(&token_kind);

        collected.push(token_stream.current_token());
        token_stream.advance();
    }

    Ok(collected)
}

pub(crate) fn has_top_level_comma_before_statement_end(token_stream: &FileTokens) -> bool {
    let mut depth = NestingDepth::default();
    let mut index = token_stream.index;

    while index < token_stream.length {
        let token_kind = &token_stream.tokens[index].kind;
        let at_top_level = depth.is_top_level();

        if at_top_level
            && matches!(
                token_kind,
                TokenKind::Newline | TokenKind::End | TokenKind::Eof
            )
        {
            break;
        }

        if at_top_level && matches!(token_kind, TokenKind::Comma) {
            return true;
        }

        depth.step(token_kind);
        index += 1;
    }

    false
}

pub(crate) fn find_expression_end_index(
    tokens: &[Token],
    start_index: usize,
    stop_tokens: &[TokenKind],
) -> usize {
    let mut index = start_index;
    let mut depth = ExpressionBoundaryDepth::default();

    while index < tokens.len() {
        let token_kind = &tokens[index].kind;

        if depth.is_top_level() && stop_tokens.iter().any(|stop| token_kind == stop) {
            break;
        }

        depth.step(token_kind);

        if matches!(token_kind, TokenKind::Eof) {
            break;
        }

        index += 1;
    }

    index
}

pub(crate) fn consume_balanced_template_region<E>(
    token_stream: &mut FileTokens,
    mut on_token: impl FnMut(Token, &TokenKind),
    on_eof_error: impl Fn(SourceLocation) -> E,
) -> Result<(), E> {
    let mut balance = TemplateBalance::with_opening_template();

    while balance.has_unclosed_templates() {
        let token_kind = token_stream.current_token_kind().clone();
        if matches!(token_kind, TokenKind::Eof) {
            return Err(on_eof_error(token_stream.current_location()));
        }

        balance.step(&token_kind);
        on_token(token_stream.current_token(), &token_kind);
        token_stream.advance();
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/token_scan_tests.rs"]
mod token_scan_tests;
