// Compiler Directives
// These are special keywords that are started with a hash, e.g., #slot
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::parsers::tokenizer::tokens::{Token, TokenKind, TokenStream};
use crate::{return_syntax_error, return_token};
use crate::compiler::parsers::tokenizer::tokenizer::string_block;

// This used by the tokenizer stage
// Also used by the config file to set compiler settings
pub fn compiler_directive(
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
            // Special
            // Import Statement
            "import" => return_token!(TokenKind::Import, stream),

            // For exporting functions or constants out of the final Wasm module
            "export" => return_token!(TokenKind::Export, stream),

            "panic" => return_token!(TokenKind::Panic, stream),

            "async" => return_token!(TokenKind::Async, stream),

            // External language blocks
            "WAT" => return_token!(TokenKind::Wat(string_block(stream)?), stream),

            // Special template tokens
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