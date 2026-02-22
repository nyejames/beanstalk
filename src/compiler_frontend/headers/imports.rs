use crate::compiler_frontend::basic_utility_functions::NumericalParsing;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokenizer::keyword_or_variable;
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
    let mut full_path = InternedPath::with_capacity(2);

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

    let mut imports: Vec<InternedPath> = Vec::with_capacity(1);
    let mut current_item = String::with_capacity(2);
    let mut multi_import = false;
    while let Some(c) = stream.peek() {
        // Breakout on the first parenthesis that isn't escaped
        // TODO: support escaped parenthesis / balanced parenthesis
        if c == &')' {
            stream.next();
            break;
        }

        if c == &'{' {
            multi_import = true;
        }

        if c == &'\\' || c == &'/' {
            full_path.push(string_table.get_or_intern(current_item));
            current_item = String::with_capacity(2);
            continue;
        }

        current_item.push(c.to_owned());
        stream.next();
    }

    if full_path.is_empty() {
        return_syntax_error!(
            "Import path cannot be empty",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Provide a valid file path",
            }
        )
    }

    // If there was a curly brace encountered,
    // Then there are one or more imports from this path.
    // Switches into creating each import symbol.
    if multi_import {
        let mut next_import_symbol = true;

        while let Some(c) = stream.next() {
            if c == '}' {
                break;
            }

            if c == ')' {
                // Error due to missing curly brace
                return_syntax_error!(
                    "Missing closing curly brace after import path",
                    stream.new_location().to_error_location(string_table), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Add a closing curly brace after the import path",
                    }
                )
            }

            // Parse all import symbols from this path,
            // Create a new full path for each
            if c.is_alphabetic() {
                let token = keyword_or_variable(&mut c.to_string(), stream, string_table)?;
                match token.kind {
                    TokenKind::Symbol(name) => {
                        if !next_import_symbol {
                            return_syntax_error!(
                                "Multiple symbols in a row must be separated by a comma.",
                                stream.new_location().to_error_location(string_table), {
                                    CompilationStage => "Tokenization",
                                    PrimarySuggestion => "Separate the symbols with a comma",
                                }
                            )
                        }
                        next_import_symbol = false;

                        let new_path = full_path.clone().append(name);
                        imports.push(new_path)
                    }
                    TokenKind::Comma => {
                        if next_import_symbol {
                            return_syntax_error!(
                                "Multiple commas in a row are not allowed in the imports list",
                                stream.new_location().to_error_location(string_table), {
                                    CompilationStage => "Tokenization",
                                    PrimarySuggestion => "Remove the extra comma",
                                }
                            )
                        }
                        next_import_symbol = true;
                    }
                    _ => return_syntax_error!(
                        "Invalid character or import name, expected a symbol or comma inside the imports list. Make sure the import isn't also a reserved keyword.",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Add a valid import name after the import path",
                        }
                    ),
                }
            }

            if !c.is_whitespace() {
                return_syntax_error!(
                    "Invalid character, expected a comma or whitespace after the import name",
                    stream.new_location().to_error_location(string_table), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Add a comma or whitespace after the import name",
                    }
                )
            }
        }

        // Skip all whitespace after the closing curly brace
        while let Some(c) = stream.peek() {
            if c.is_whitespace() {
                stream.next();
            } else if c != &')' {
                // Must close the path with a closing parenthesis
                return_syntax_error!(
                    "Invalid character, expected a closing parenthesis after the import path",
                    stream.new_location().to_error_location(string_table), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Add a closing parenthesis after the import path",
                    }
                )
            } else {
                break;
            }
        }
    }

    if imports.is_empty() {
        imports.push(full_path);
    }

    return_token!(TokenKind::Path(imports), stream)
}
