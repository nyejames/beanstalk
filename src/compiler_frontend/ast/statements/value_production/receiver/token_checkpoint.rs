//! Token stream rollback helper for speculative value-block parsing.
//!
//! WHAT: captures the current token index before trying an optional parse branch.
//! WHY: inline single-predicate value matches are syntax-selected after parsing
//! a possible scrutinee; failed speculation must restore the token stream exactly.

use crate::compiler_frontend::tokenizer::tokens::FileTokens;

/// A lightweight snapshot of the token stream index.
///
/// WHAT: records `token_stream.index` so a speculative parse can be rolled back.
/// WHY: this is cheaper and clearer than manual `let start = stream.index;`
/// followed by `stream.index = start;` at every speculative site.
pub(super) struct TokenCheckpoint {
    index: usize,
}

impl TokenCheckpoint {
    /// Capture the current token index.
    pub(super) fn capture(token_stream: &FileTokens) -> Self {
        Self {
            index: token_stream.index,
        }
    }

    /// Restore the token stream to the captured index.
    pub(super) fn restore(self, token_stream: &mut FileTokens) {
        token_stream.index = self.index;
    }

    /// Consume the checkpoint without restoring, marking speculation as successful.
    pub(super) fn commit(self) {}
}
