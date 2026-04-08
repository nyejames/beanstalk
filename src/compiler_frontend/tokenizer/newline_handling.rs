//! Newline normalization policy for tokenizer string/template bodies.
//!
//! WHAT: controls how `\r` and `\r\n` are represented in emitted token text payloads.

use crate::compiler_frontend::tokenizer::tokens::TokenStream;

/// Controls how raw source line endings are emitted into string/template bodies
/// when a `\r` is encountered in the source stream.
///
/// `NormalizeToLf` is the default and recommended mode for compiler stability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewlineMode {
    #[default]
    NormalizeToLf,
}

/// Consume a newline that started with `\r`.
///
/// Automatically handles both `\r\n` and bare `\r` newlines,
/// and advances the stream position accordingly.
/// Returns the appropriate newline string based on the specified `NewlineMode`.
pub fn consume_carriage_return_newline(stream: &mut TokenStream) -> &'static str {
    // Consume the \r and move past it in the stream
    // Not invoking stream.next() here so column isn't advanced
    stream.chars.next();

    let has_following_lf = matches!(stream.chars.peek(), Some('\n'));

    if has_following_lf {
        stream.next(); // consume the '\n' in a CRLF pair (also advanced the line)
    } else {
        // Advance the line number for a bare \r, but don't consume any more chars
        stream.position.line_number += 1;
        stream.position.char_column = 0;
    }

    let _ = stream.newline_mode;
    "\n"
}
