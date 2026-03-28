use crate::compiler_frontend::tokenizer::tokens::TokenStream;

/// Controls how raw source line endings are emitted into string/template bodies
/// when a `\r` is encountered in the source stream.
///
/// `NormalizeToLf` is the default and recommended mode for compiler stability.
///
/// `PreserveRaw` is an escape hatch for cases where the build system wants
/// string/template bodies to retain the original source line-ending form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewlineMode {
    #[default]
    NormalizeToLf,

    #[allow(dead_code)] // for now, since this is only used in the REPL currently which is not yet implemented
    PreserveRaw,
}

/// Consume a newline that started with `\r`.
///
/// Automatically handles both `\r\n` and bare `\r` newlines, 
/// and advances the stream position accordingly.
/// Returns the appropriate newline string based on the specified `NewlineMode`.
pub fn consume_carriage_return_newline(
    stream: &mut TokenStream,
) -> &'static str {
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

    match stream.newline_mode {
        NewlineMode::NormalizeToLf => "\n",
        NewlineMode::PreserveRaw => if has_following_lf { "\r\n" } else { "\r" },
    }
}
