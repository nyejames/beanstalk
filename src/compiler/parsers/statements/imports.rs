use std::path::PathBuf;

use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::interned_path::{self, InternedPath};
use crate::compiler::parsers::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler::string_interning::{StringId, StringTable};
use crate::return_rule_error;

pub struct FileImport {
    pub alias: Option<StringId>,
    pub header_path: InternedPath,
}

// Parses tokens after the "Import" directive
// Each Import is a path to a file and the name of the header being imported.
// import @libraries/math/sqrt"
// The header being imported from the file is just the last part of the path,
// and the same symbol will be used as the header (sqrt in this example).
pub fn parse_import(
    token_stream: &mut FileTokens,
    string_table: &mut StringTable,
) -> Result<FileImport, CompilerError> {
    // TODO: Support renaming imports
    // This might look exactly like declaration syntax:
    // newName = import @path/to/file
    // Or something else entirely. Has not been decided yet.

    // Starts after the import token
    // TODO: This now needs to be a path
    // Beanstalk uses a special path syntax: @path/to/file
    let string_id =
        if let TokenKind::StringSliceLiteral(p) = token_stream.current_token_kind().to_owned() {
            p
        } else {
            return_rule_error!(
                format!(
                    "Expected a path after the import keyword. Found: {:?}",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location().to_error_location(string_table),
                {
                    CompilationStage => "Import Parsing",
                    PrimarySuggestion => "Use import @path/to/file syntax to import a file",
                }
            )
        };

    token_stream.advance();

    let path = PathBuf::from(string_table.resolve(string_id));
    let header_path = InternedPath::from_path_buf(&path, string_table);
    Ok(FileImport {
        header_path,

        // Will be replaced with the rename when that is supported,
        // if a new symbol is provided
        alias: None,
    })
}
