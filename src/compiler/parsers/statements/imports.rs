use crate::compiler::compiler_errors::CompileError;
use crate::compiler::interned_path::InternedPath;
use crate::compiler::parsers::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler::string_interning::{StringId, StringTable};
use crate::return_rule_error;

// Parses tokens after the "Import" token
// Each Import is a path to a file and the name of the header being imported.
// #import(@libraries/math/sqrt)
// The TokenKind::Import must be followed by a TokenKind::path(PathBuf)
// The header being imported from the file is just the last part of the path,
// and the same symbol will be used as the header (sqrt in this example).
pub fn parse_import(
    token_stream: &mut FileTokens,
    string_table: &mut StringTable,
) -> Result<(StringId, InternedPath), CompileError> {
    // Starts after the import token
    let path = if let TokenKind::PathLiteral(p) = token_stream.current_token_kind().to_owned() {
        p
    } else {
        return_rule_error!(
            format!(
                "Expected a path after the import keyword. Found: {:?}",
                token_stream.current_token_kind()
            ),
            token_stream.current_location().to_error_location(&string_table),
            {
                CompilationStage => "Import Parsing",
                PrimarySuggestion => "Use #import @path/to/file syntax to import a file",
            }
        )
    };

    token_stream.advance();
    
    let import_name = match path.file_name() {
        Some(name) => name,
        None => {
            let path_str: &'static str = Box::leak(format!("{:?}", path).into_boxed_str());
            return_rule_error!(
                format!(
                    "Invalid import path: {:?}. You might be forgetting to add the name of what you are importing at the end of the path!",
                    path
                ),
                token_stream.current_location().to_error_location(&string_table),
                {
                    CompilationStage => "Import Parsing",
                    PrimarySuggestion => "Add the file name at the end of the import path",
                    SuggestedLocation => path_str,
                }
            )
        }
    };

    Ok((import_name, path))
}
