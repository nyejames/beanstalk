use crate::compiler_frontend::basic_utility_functions::NumericalParsing;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{Token, TokenKind, TokenStream};
use crate::{return_syntax_error, return_token};

pub fn parse_file_path(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<Token, CompilerError> {
    // Path Syntax
    // @(path/to/file)
    // Path to multiple items within the same directory (used for imports)
    // @(path/to/file/ {import1, import2, import3})
    // or @(path/to/file/ {
    //        import1,
    //        import2
    // })
    let mut full_import_string = String::new();

    // Skip initial non-newline whitespace
    while let Some(c) = stream.peek() {
        // Breakout on the first-detected whitespace or the end of the string
        if c.is_non_newline_whitespace() && c != &'\n' {
            continue;
        }

        break;
    }

    if stream.peek() == Some(&'(') {
        stream.next();
    } else {
        return_syntax_error!(
            "Path must start with an open parenthesis after '@'",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Use an open parenthesis after '@' to start a path",
            }
        )
    }

    let mut comma_indexes: Vec<usize> = Vec::new();
    while let Some(c) = stream.peek() {
        // Breakout on the first parenthesis that isn't escaped
        // TODO: support escaped parenthesis / balanced parenthesis
        if c == &')' {
            stream.next();
            break;
        }

        // If there is a curly brace encountered,
        // Then there are one or more imports from this path.
        // Switches into creating each import symbol.
        if c == &'{' {
            comma_indexes.push(full_import_string.len());
        }

        full_import_string.push(c.to_owned());
        stream.next();
    }

    if full_import_string.is_empty() {
        return_syntax_error!(
            "Import path cannot be empty",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Provide a valid file path",
            }
        )
    }

    // If there are comma indexes, then there are multiple imports from this path
    // So parse each import symbol
    // First find the last part of the path (the last '/' or '\' encountered
    let last_slash_index = full_import_string
        .chars()
        .rev()
        .position(|c| c == '/' || c == '\\')
        .unwrap_or(full_import_string.len());

    let mut imports = Vec::new();

    // Get the first import between the last slash index and first comma OR last char
    if comma_indexes.len() == 1 {
        // If there is at least one comma, get the first item
        let range_between = last_slash_index + 1..comma_indexes[0];
        let interned_string = string_table.intern(&full_import_string[range_between]);
        imports.push(interned_string);

        // TODO: Support multiple imports separated with a coma
    } else {
        // If no commas, just get the only item
        let range_between = last_slash_index + 1..full_import_string.len() - 1;
        let interned_string = string_table.intern(&full_import_string[range_between]);
        imports.push(interned_string);
    }

    let interned_path =
        InternedPath::from_components(vec![string_table.intern(&full_import_string)]);
    return_token!(TokenKind::Path(interned_path, imports), stream)
}
