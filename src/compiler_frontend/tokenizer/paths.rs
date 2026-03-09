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
    // @path/to/file
    // @(path/to/file)
    // Path to multiple items within the same directory (used for imports)
    // @path/to/file/{import1, import2, import3}
    // @path/to/file/ {import1, import2, import3}
    // @(path/to/file/ {import1, import2, import3})
    // or @(path/to/file/ {
    //        import1,
    //        import2
    // })
    let mut base_components = Vec::with_capacity(2);
    let mut segment = String::new();

    // Skip initial non-newline whitespace
    while let Some(c) = stream.peek() {
        if c.is_non_newline_whitespace() && c != &'\n' {
            stream.next();
            continue;
        }
        break;
    }

    let wrapped_in_parentheses = if stream.peek() == Some(&'(') {
        stream.next();
        true
    } else {
        false
    };

    let mut imports: Vec<InternedPath> = Vec::with_capacity(1);
    let mut grouped_imports = false;
    let mut closed_wrapped_path = false;
    let mut closed_grouped_import = false;

    while let Some(c) = stream.peek().copied() {
        if !wrapped_in_parentheses && matches!(c, '\n' | '\r') {
            break;
        }

        match c {
            ')' if wrapped_in_parentheses => {
                push_segment_if_non_empty(&mut base_components, &mut segment, string_table);
                stream.next();
                closed_wrapped_path = true;
                break;
            }

            '{' => {
                grouped_imports = true;
                push_segment_if_non_empty(&mut base_components, &mut segment, string_table);
                stream.next();
                break;
            }

            _ if !wrapped_in_parentheses && c.is_non_newline_whitespace() => {
                push_segment_if_non_empty(&mut base_components, &mut segment, string_table);
                consume_non_newline_whitespace(stream);
                if stream.peek() == Some(&'{') {
                    grouped_imports = true;
                    stream.next();
                }
                break;
            }

            '/' | '\\' => {
                push_segment_if_non_empty(&mut base_components, &mut segment, string_table);
                stream.next();
            }

            _ => {
                segment.push(c);
                stream.next();
            }
        }
    }

    if wrapped_in_parentheses && !grouped_imports && !closed_wrapped_path {
        return_syntax_error!(
            "Invalid character, expected a closing parenthesis for this path.",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Close the path with ')' after the import target",
            }
        )
    }

    push_segment_if_non_empty(&mut base_components, &mut segment, string_table);

    if base_components.is_empty() {
        return_syntax_error!(
            "Import path cannot be empty",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Provide a valid file path",
            }
        )
    }

    if !grouped_imports {
        imports.push(InternedPath::from_components(base_components));
        return_token!(TokenKind::Path(imports), stream);
    }

    let mut expect_symbol = true;
    let mut parsed_symbols = 0usize;

    while let Some(c) = stream.peek().copied() {
        if c.is_whitespace() {
            stream.next();
            continue;
        }

        match c {
            '}' => {
                if expect_symbol && parsed_symbols > 0 {
                    return_syntax_error!(
                        "Trailing comma is not allowed in grouped imports.",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Remove the trailing comma before '}'",
                        }
                    )
                }

                stream.next();
                closed_grouped_import = true;
                break;
            }

            ',' => {
                if expect_symbol {
                    return_syntax_error!(
                        "Multiple commas in a row are not allowed in grouped imports.",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Remove the extra comma",
                        }
                    )
                }

                expect_symbol = true;
                stream.next();
            }

            _ => {
                if !expect_symbol {
                    return_syntax_error!(
                        "Grouped import symbols must be separated by commas.",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Add a comma between grouped import symbols",
                        }
                    )
                }

                if !(c.is_alphabetic() || c == '_') {
                    return_syntax_error!(
                        "Invalid grouped import symbol. Symbols must start with a letter or underscore.",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Use a valid grouped import symbol name",
                        }
                    )
                }

                let mut symbol = String::new();
                symbol.push(c);
                stream.next();

                while let Some(next) = stream.peek().copied() {
                    if next.is_alphanumeric() || next == '_' {
                        symbol.push(next);
                        stream.next();
                    } else {
                        break;
                    }
                }

                let mut full_components = base_components.clone();
                full_components.push(string_table.intern(&symbol));
                imports.push(InternedPath::from_components(full_components));
                parsed_symbols += 1;
                expect_symbol = false;
            }
        }
    }

    if grouped_imports && !closed_grouped_import {
        return_syntax_error!(
            "Grouped import is missing a closing '}'",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Close grouped imports with '}'",
                SuggestedInsertion => "}",
            }
        )
    }

    if imports.is_empty() {
        return_syntax_error!(
            "Grouped imports require at least one symbol.",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Add at least one import symbol inside '{}'",
            }
        )
    }

    if wrapped_in_parentheses {
        // Skip trailing whitespace and enforce final ')'
        while let Some(c) = stream.peek() {
            if c.is_whitespace() {
                stream.next();
            } else {
                break;
            }
        }

        if stream.peek() != Some(&')') {
            return_syntax_error!(
                "Invalid character, expected a closing parenthesis after grouped imports.",
                stream.new_location().to_error_location(string_table), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Add a closing parenthesis after the grouped imports",
                }
            )
        }

        stream.next();
    }

    return_token!(TokenKind::Path(imports), stream)
}

pub fn parse_import_clause_tokens(
    tokens: &[Token],
    start_index: usize,
    string_table: &StringTable,
) -> Result<(Vec<InternedPath>, usize), CompilerError> {
    let Some(import_token) = tokens.get(start_index) else {
        return Err(CompilerError::compiler_error(
            "Import clause parsing started past the end of the token stream.",
        ));
    };

    if !matches!(import_token.kind, TokenKind::Import) {
        return Err(CompilerError::compiler_error(
            "Import clause parsing expected to start on an 'import' token.",
        ));
    }

    let mut index = start_index + 1;
    while tokens
        .get(index)
        .is_some_and(|token| matches!(token.kind, TokenKind::Newline))
    {
        index += 1;
    }

    let Some(path_token) = tokens.get(index) else {
        return_syntax_error!(
            "Expected a path after the 'import' keyword",
            import_token.location.to_error_location(string_table), {
                CompilationStage => "Header Parsing",
                PrimarySuggestion => "Add an import path like '@(folder/file)' after 'import'",
            }
        );
    };

    let TokenKind::Path(paths) = &path_token.kind else {
        return_syntax_error!(
            format!(
                "Expected a path after the 'import' keyword, found '{:?}'",
                path_token.kind
            ),
            path_token.location.to_error_location(string_table), {
                CompilationStage => "Header Parsing",
                PrimarySuggestion => "Use import syntax like 'import @(folder/file)'",
            }
        );
    };

    Ok((paths.to_owned(), index + 1))
}

pub fn collect_import_paths_from_tokens(
    tokens: &[Token],
    string_table: &StringTable,
) -> Result<Vec<InternedPath>, CompilerError> {
    let mut imports = Vec::new();
    let mut index = 0usize;

    while index < tokens.len() {
        if matches!(tokens[index].kind, TokenKind::Import) {
            let (paths, next_index) = parse_import_clause_tokens(tokens, index, string_table)?;
            imports.extend(paths);
            index = next_index;
            continue;
        }

        index += 1;
    }

    Ok(imports)
}

fn push_segment_if_non_empty(
    components: &mut Vec<crate::compiler_frontend::string_interning::StringId>,
    segment: &mut String,
    string_table: &mut StringTable,
) {
    let trimmed = segment.trim();
    if !trimmed.is_empty() {
        components.push(string_table.intern(trimmed));
    }
    segment.clear();
}

fn consume_non_newline_whitespace(stream: &mut TokenStream) {
    while stream
        .peek()
        .is_some_and(|character| character.is_non_newline_whitespace())
    {
        stream.next();
    }
}

#[cfg(test)]
#[path = "tests/paths_tests.rs"]
mod paths_tests;
