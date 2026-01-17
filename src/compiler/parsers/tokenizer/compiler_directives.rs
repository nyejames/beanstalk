// Compiler Directives
// These are special keywords that are started with a hash, e.g., #slot
use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::parsers::tokenizer::tokens::{Token, TokenKind, TokenStream};
use crate::compiler::string_interning::StringTable;
use crate::{return_syntax_error, return_token};

// Compiler directives are special keywords that start with a hash.
// These can be thought of as communicating with the build system, compiler or host environment.
// They are a flexible way to extend complex behaviours in the compiler or for build systems to be able to provide special commands.
// They are the only part of the Beanstalk syntax that can be inserted anywhere without disrupting the normal parsing.
#[derive(Clone, Debug, PartialEq)]
pub enum CompilerDirective {
    Panic,
    Export
}

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
            "panic" => return_token!(TokenKind::Directive(CompilerDirective::Panic), stream),
            "export" => return_token!(TokenKind::Directive(CompilerDirective::Export), stream),

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
