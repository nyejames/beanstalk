//! Result-propagation boundary helpers.
//!
//! WHAT: defines token boundaries where postfix `!` is parsed as propagation instead of fallback.
//! WHY: propagation boundary rules must stay shared between call and expression result handling.

use crate::compiler_frontend::tokenizer::tokens::TokenKind;

pub(crate) fn is_result_propagation_boundary(token: &TokenKind) -> bool {
    matches!(
        token,
        TokenKind::CloseParenthesis
            | TokenKind::Comma
            | TokenKind::Newline
            | TokenKind::End
            | TokenKind::Eof
            | TokenKind::Colon
            | TokenKind::TemplateClose
            | TokenKind::CloseCurly
            | TokenKind::Dot
    )
}
