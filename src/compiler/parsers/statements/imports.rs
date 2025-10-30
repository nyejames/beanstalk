use crate::compiler::compiler_errors::CompileError;
use crate::compiler::parsers::tokens::{FileTokens, TokenKind};
use crate::return_rule_error;
use std::path::Path;

// Parses tokens after the "Import" token
// Each Import is a path to a file and the name of the header being imported.
// import @libraries/math/sqrt
// The TokenKind::Import must be followed by a TokenKind::path(PathBuf)
// The header being imported from the file is just the last part of the path,
// and the same symbol will be used as the header (sqrt in this example).
pub fn parse_import(token_stream: &mut FileTokens) -> Result<String, CompileError> {
    // Starts after the import token
    let path = if let TokenKind::PathLiteral(p) = token_stream.current_token_kind().to_owned() {
        p
    } else {
        return_rule_error!(
            token_stream.current_location(),
            "Expected a path after the import keyword. Found: {:?}",
            token_stream.current_token_kind()
        )
    };

    token_stream.advance();
    
    Ok(path)
}
