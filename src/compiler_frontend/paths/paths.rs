use crate::compiler_frontend::basic_utility_functions::NumericalParsing;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{Token, TokenKind, TokenStream, TokenizeMode};
use crate::{return_syntax_error, return_token};

pub fn parse_file_path(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<Token, CompilerError> {
    // Path syntax accepted by the tokenizer.
    //
    // Examples:
    // @path/to/file
    // @(path/to/file)
    //
    // Grouped entries expand a base path into multiple concrete paths.
    // The grouped entries are path components, not identifier-only symbols.
    // Depending on context these may refer to exported symbols, file names,
    // or other path-like entries.
    //
    // @path/to/base/{entry1, entry2}
    // @path/to/base/ {entry1, entry2}
    // @(path/to/base/{entry1, entry2})
    // @(path/to/base/ {
    //     entry1,
    //     entry2
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

    let mut parsed_paths: Vec<InternedPath> = Vec::with_capacity(1);
    let mut has_grouped_entries = false;
    let mut closed_wrapped_path = false;
    let mut closed_grouped_entries = false;

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

            // In template heads, stop before `]` or `:` so the enclosing template
            // parser can consume the closing delimiter or body separator.
            _ if stream.mode == TokenizeMode::TemplateHead && matches!(c, ']' | ':') => {
                push_segment_if_non_empty(&mut base_components, &mut segment, string_table);
                break;
            }

            '{' => {
                has_grouped_entries = true;
                push_segment_if_non_empty(&mut base_components, &mut segment, string_table);
                stream.next();
                break;
            }

            // Config lists use bare path syntax like `@lib, @assets`.
            // Stop before delimiters so the lexer can emit the comma/brace separately.
            ',' | '}' if !wrapped_in_parentheses => {
                push_segment_if_non_empty(&mut base_components, &mut segment, string_table);
                break;
            }

            _ if !wrapped_in_parentheses && c.is_non_newline_whitespace() => {
                push_segment_if_non_empty(&mut base_components, &mut segment, string_table);
                consume_non_newline_whitespace(stream);
                if stream.peek() == Some(&'{') {
                    has_grouped_entries = true;
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

    if wrapped_in_parentheses && !has_grouped_entries && !closed_wrapped_path {
        return_syntax_error!(
            "Invalid character, expected a closing parenthesis for this path.",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Close the path with ')' after the path target",
            }
        )
    }

    push_segment_if_non_empty(&mut base_components, &mut segment, string_table);

    if base_components.is_empty() {
        return_syntax_error!(
            "Path cannot be empty",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Provide a valid file path",
            }
        )
    }

    if !has_grouped_entries {
        parsed_paths.push(InternedPath::from_components(base_components));
        return_token!(TokenKind::Path(parsed_paths), stream);
    }

    // Parse grouped path entries after `base/{...}`.
    // Each entry is appended as one final path component and expanded into
    // its own InternedPath.
    let mut expect_grouped_entry = true;
    let mut parsed_grouped_entries = 0usize;

    while let Some(c) = stream.peek().copied() {
        if c.is_whitespace() {
            stream.next();
            continue;
        }

        match c {
            '}' => {
                if expect_grouped_entry && parsed_grouped_entries > 0 {
                    return_syntax_error!(
                        "Trailing comma is not allowed in grouped paths.",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Remove the trailing comma before '}'",
                        }
                    )
                }

                stream.next();
                closed_grouped_entries = true;
                break;
            }

            ',' => {
                if expect_grouped_entry {
                    return_syntax_error!(
                        "Multiple commas in a row are not allowed in grouped paths.",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Remove the extra comma",
                        }
                    )
                }

                expect_grouped_entry = true;
                stream.next();
            }

            _ => {
                if !expect_grouped_entry {
                    return_syntax_error!(
                        "Grouped path entries must be separated by commas.",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Add a comma between grouped path symbols",
                        }
                    )
                }

                if !is_grouped_path_component_char(c) {
                    return_syntax_error!(
                        "Invalid grouped path entry",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Use a valid grouped path entry",
                        }
                    )
                }

                let mut grouped_entry = String::new();
                grouped_entry.push(c);
                stream.next();

                while let Some(next) = stream.peek().copied() {
                    if is_grouped_path_component_char(next) {
                        grouped_entry.push(next);
                        stream.next();
                    } else {
                        break;
                    }
                }

                let mut entry_components = base_components.clone();
                entry_components.push(string_table.intern(&grouped_entry));
                parsed_paths.push(InternedPath::from_components(entry_components));
                parsed_grouped_entries += 1;
                expect_grouped_entry = false;
            }
        }
    }

    if has_grouped_entries && !closed_grouped_entries {
        return_syntax_error!(
            "grouped path is missing a closing '}'",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Close grouped paths with '}'",
                SuggestedInsertion => "}",
            }
        )
    }

    if parsed_paths.is_empty() {
        return_syntax_error!(
            "grouped path require at least one symbol.",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Add at least one path symbol inside '{}'",
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
                "Invalid character, expected a closing parenthesis after grouped paths.",
                stream.new_location().to_error_location(string_table), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Add a closing parenthesis after the grouped paths",
                }
            )
        }

        stream.next();
    }

    return_token!(TokenKind::Path(parsed_paths), stream)
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

pub fn collect_paths_from_tokens(
    tokens: &[Token],
    string_table: &StringTable,
) -> Result<Vec<InternedPath>, CompilerError> {
    let mut parsed_paths = Vec::new();
    let mut index = 0usize;

    while index < tokens.len() {
        if matches!(tokens[index].kind, TokenKind::Import) {
            let (paths, next_index) = parse_import_clause_tokens(tokens, index, string_table)?;
            parsed_paths.extend(paths);
            index = next_index;
            continue;
        }

        index += 1;
    }

    Ok(parsed_paths)
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

fn is_grouped_path_component_char(c: char) -> bool {
    // Reject control characters outright.
    if c.is_control() {
        return false;
    }

    // Allow a normal space, but reject other whitespace like tabs/newlines.
    if c.is_whitespace() && c != ' ' {
        return false;
    }

    // Reject Beanstalk syntax delimiters and path separators.
    !matches!(
        c,
        '[' | ']' | '{' | '}' | ',' | '(' | ')' | '/' | '\\'
            // Reject Windows-forbidden filename characters too, to stay conservative
            // across operating systems.
            | '<' | '>' | ':' | '"' | '|' | '?' | '*'
    )
}

#[cfg(test)]
#[path = "tests/paths_tests.rs"]
mod paths_tests;
