// Compiler Directives
// These are special keywords that are started with a hash, e.g., #slot
use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::parsers::tokenizer::tokens::{Token, TokenKind, TokenStream};
use crate::compiler::string_interning::StringTable;
use crate::{return_syntax_error, return_token};

// This used by the tokenizer stage
// Also used by the config file to set compiler settings
pub fn compiler_directive(
    token_value: &mut String,
    stream: &mut TokenStream,
    string_table: &StringTable,
) -> Result<Token, CompilerError> {
    loop {
        if stream
            .peek()
            .is_some_and(|c| c.is_alphanumeric() || c == &'_')
        {
            token_value.push(stream.next().unwrap());
            continue;
        }

        match token_value.as_str() {
            "panic" => return_token!(TokenKind::Panic, stream),

            // External language blocks
            // PROBABLY WONT DO THIS
            // Will possibly allow wat files that can be imported into Beanstalk modules in the future,
            // But likely not.
            // "WAT" => return_token!(TokenKind::Wat(string_block(stream, string_table)?), stream),

            // Special template tokens
            "slot" => return_token!(TokenKind::Slot, stream),

            _ => {
                return_syntax_error!(
                    format!("Invalid compiler directive: #{}", token_value),
                    stream.new_location().to_error_location(string_table),
                    {
                        CompilationStage => "Tokenization",
                    }
                )
            }
        };
    }
}
