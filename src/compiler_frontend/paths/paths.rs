use crate::compiler_frontend::basic_utility_functions::NumericalParsing;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{Token, TokenKind, TokenStream, TokenizeMode};
use crate::{return_syntax_error, return_token};

type PathComponents = Vec<StringId>;
type RelativePathExpansions = Vec<PathComponents>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PathStopReason {
    EndOfInput,
    Newline,
    TemplateHeadDelimiter,
    ConfigDelimiter,
    WrappedCloseParen,
    GroupStart,
}

#[derive(Debug)]
struct ParsedPathPrefix {
    components: PathComponents,
    stop_reason: PathStopReason,
    ended_with_separator: bool,
}

#[derive(Debug)]
struct ParsedGroupedPrefix {
    components: PathComponents,
    ended_with_separator: bool,
}

pub fn parse_file_path(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<Token, CompilerError> {
    // Path syntax accepted by the tokenizer.
    //
    // Canonical examples:
    // @path/to/file
    // @(path/to/file)
    //
    // @docs {
    //     intro.md,
    //     guides/getting-started.md,
    //     guides/advanced {
    //         ownership.md,
    //         memory.md,
    //     },
    // }

    // Skip initial non-newline whitespace before the path contents.
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

    let parsed_prefix = parse_path_prefix(stream, string_table, wrapped_in_parentheses)?;

    if parsed_prefix.components.is_empty() {
        return_syntax_error!(
            "Path cannot be empty.",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Provide a valid file path",
            }
        )
    }

    if parsed_prefix.ended_with_separator && parsed_prefix.stop_reason == PathStopReason::GroupStart
    {
        return_syntax_error!(
            "Slash-before-group syntax is not supported. Use 'base { ... }'.",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Remove the separator before '{' and write the group as 'base { ... }'",
            }
        )
    }

    if parsed_prefix.ended_with_separator {
        return_syntax_error!(
            "Path cannot end with a separator.",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Remove the trailing '/' or '\\' from this path",
            }
        )
    }

    let mut parsed_paths = Vec::with_capacity(1);

    if parsed_prefix.stop_reason != PathStopReason::GroupStart {
        if wrapped_in_parentheses && parsed_prefix.stop_reason != PathStopReason::WrappedCloseParen
        {
            return_syntax_error!(
                "Invalid character, expected a closing parenthesis for this path.",
                stream.new_location().to_error_location(string_table), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Add a closing ')' after the path",
                }
            )
        }

        parsed_paths.push(InternedPath::from_components(parsed_prefix.components));
        return_token!(TokenKind::Path(parsed_paths), stream);
    }

    let grouped_suffixes = parse_grouped_block(stream, string_table)?;

    for suffix_components in grouped_suffixes {
        let mut full_components = parsed_prefix.components.clone();
        full_components.extend(suffix_components);
        parsed_paths.push(InternedPath::from_components(full_components));
    }

    if wrapped_in_parentheses {
        consume_all_whitespace(stream);

        if stream.peek() != Some(&')') {
            return_syntax_error!(
                "Invalid character, expected a closing parenthesis after the grouped path.",
                stream.new_location().to_error_location(string_table), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Add a closing ')' after the grouped path",
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
            "Expected a path after the 'import' keyword.",
            import_token.location.to_error_location(string_table), {
                CompilationStage => "Header Parsing",
                PrimarySuggestion => "Add an import path like '@(folder/file)' after 'import'",
            }
        );
    };

    let TokenKind::Path(paths) = &path_token.kind else {
        return_syntax_error!(
            format!(
                "Expected a path after the 'import' keyword, found '{:?}'.",
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

/// WHAT: Parses the base path prefix before an optional grouped block.
/// WHY: This isolates context-sensitive stop conditions (wrapped paths, template-head delimiters,
///      config-list delimiters) from grouped expansion parsing.
fn parse_path_prefix(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
    wrapped_in_parentheses: bool,
) -> Result<ParsedPathPrefix, CompilerError> {
    let mut components = Vec::with_capacity(2);
    let mut component_buffer = String::new();
    let mut seen_non_relative_component = false;
    let mut ended_with_separator = false;

    loop {
        let Some(c) = stream.peek().copied() else {
            let _ = push_component_if_present(
                &mut components,
                &mut component_buffer,
                true,
                &mut seen_non_relative_component,
                stream,
                string_table,
            )?;

            return Ok(ParsedPathPrefix {
                components,
                stop_reason: PathStopReason::EndOfInput,
                ended_with_separator,
            });
        };

        if !wrapped_in_parentheses && matches!(c, '\n' | '\r') {
            let _ = push_component_if_present(
                &mut components,
                &mut component_buffer,
                true,
                &mut seen_non_relative_component,
                stream,
                string_table,
            )?;

            return Ok(ParsedPathPrefix {
                components,
                stop_reason: PathStopReason::Newline,
                ended_with_separator,
            });
        }

        if stream.mode == TokenizeMode::TemplateHead && matches!(c, ']' | ':') {
            let _ = push_component_if_present(
                &mut components,
                &mut component_buffer,
                true,
                &mut seen_non_relative_component,
                stream,
                string_table,
            )?;

            return Ok(ParsedPathPrefix {
                components,
                stop_reason: PathStopReason::TemplateHeadDelimiter,
                ended_with_separator,
            });
        }

        if !wrapped_in_parentheses && matches!(c, ',' | '}') {
            let _ = push_component_if_present(
                &mut components,
                &mut component_buffer,
                true,
                &mut seen_non_relative_component,
                stream,
                string_table,
            )?;

            return Ok(ParsedPathPrefix {
                components,
                stop_reason: PathStopReason::ConfigDelimiter,
                ended_with_separator,
            });
        }

        if wrapped_in_parentheses && c == ')' {
            let _ = push_component_if_present(
                &mut components,
                &mut component_buffer,
                true,
                &mut seen_non_relative_component,
                stream,
                string_table,
            )?;
            stream.next();

            return Ok(ParsedPathPrefix {
                components,
                stop_reason: PathStopReason::WrappedCloseParen,
                ended_with_separator,
            });
        }

        if c == '{' {
            let _ = push_component_if_present(
                &mut components,
                &mut component_buffer,
                true,
                &mut seen_non_relative_component,
                stream,
                string_table,
            )?;
            stream.next();

            return Ok(ParsedPathPrefix {
                components,
                stop_reason: PathStopReason::GroupStart,
                ended_with_separator,
            });
        }

        if matches!(c, '/' | '\\') {
            let pushed_component = push_component_if_present(
                &mut components,
                &mut component_buffer,
                true,
                &mut seen_non_relative_component,
                stream,
                string_table,
            )?;

            if !pushed_component {
                return_syntax_error!(
                    "Empty path component is not allowed here.",
                    stream.new_location().to_error_location(string_table), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Remove repeated separators and keep exactly one '/' between components",
                    }
                )
            }

            stream.next();
            ended_with_separator = true;
            continue;
        }

        component_buffer.push(c);
        stream.next();

        if !c.is_whitespace() {
            ended_with_separator = false;
        }
    }
}

/// WHAT: Parses one grouped `{ ... }` block into expanded relative-path suffixes.
/// WHY: Grouped path syntax is pure sugar; this recursive parser expands nested groups into
///      explicit suffix component lists that the caller can prepend to a base prefix.
fn parse_grouped_block(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<RelativePathExpansions, CompilerError> {
    let mut expanded_suffixes: RelativePathExpansions = Vec::new();
    let mut saw_entry = false;
    let mut expect_entry = true;

    loop {
        consume_all_whitespace(stream);

        let Some(next) = stream.peek().copied() else {
            return_syntax_error!(
                "Grouped path is missing a closing '}'.",
                stream.new_location().to_error_location(string_table), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Close the grouped path with '}'",
                    SuggestedInsertion => "}",
                }
            );
        };

        if !expect_entry {
            match next {
                ',' => {
                    stream.next();
                    expect_entry = true;
                    continue;
                }
                '}' => {
                    stream.next();
                    break;
                }
                _ => {
                    return_syntax_error!(
                        "Grouped path entries must be separated by commas.",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Add a comma between grouped path entries",
                        }
                    )
                }
            }
        }

        if next == '}' {
            if !saw_entry {
                return_syntax_error!(
                    "Grouped path requires at least one entry.",
                    stream.new_location().to_error_location(string_table), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Add at least one grouped path entry inside '{}'",
                    }
                )
            }

            stream.next();
            break;
        }

        if next == ',' {
            return_syntax_error!(
                "Multiple commas in a row are not allowed in grouped paths.",
                stream.new_location().to_error_location(string_table), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Remove the extra comma",
                }
            )
        }

        let parsed_prefix = parse_grouped_entry_prefix(stream, string_table)?;

        consume_all_whitespace(stream);

        if stream.peek() == Some(&'{') {
            if parsed_prefix.ended_with_separator {
                return_syntax_error!(
                    "Slash-before-group syntax is not supported. Use 'base { ... }'.",
                    stream.new_location().to_error_location(string_table), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Remove the separator before '{' and write the group as 'base { ... }'",
                    }
                )
            }

            if parsed_prefix.components.is_empty() {
                return_syntax_error!(
                    "Nested grouped paths require a non-empty prefix before '{'.",
                    stream.new_location().to_error_location(string_table), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Add a path prefix before the nested grouped block",
                    }
                )
            }

            stream.next();
            let child_suffixes = parse_grouped_block(stream, string_table)?;

            for child_suffix in child_suffixes {
                let mut combined = parsed_prefix.components.clone();
                combined.extend(child_suffix);
                expanded_suffixes.push(combined);
            }
        } else {
            if parsed_prefix.ended_with_separator {
                return_syntax_error!(
                    "A grouped path prefix cannot end with a separator.",
                    stream.new_location().to_error_location(string_table), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Remove the trailing '/' or '\\' from this grouped path prefix",
                    }
                )
            }

            if parsed_prefix.components.is_empty() {
                return_syntax_error!(
                    "Grouped path entry cannot be empty.",
                    stream.new_location().to_error_location(string_table), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Provide a valid grouped path entry",
                    }
                )
            }

            expanded_suffixes.push(parsed_prefix.components);
        }

        saw_entry = true;
        expect_entry = false;
    }

    Ok(expanded_suffixes)
}

/// WHAT: Parses one grouped entry prefix up to `,`, `}`, or nested `{`.
/// WHY: A grouped entry can be either a leaf path or a nested-group prefix. This helper
///      captures the shared prefix parsing and validation logic for both forms.
fn parse_grouped_entry_prefix(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<ParsedGroupedPrefix, CompilerError> {
    let mut components = Vec::new();
    let mut component_buffer = String::new();
    let mut seen_non_relative_component = false;
    let mut ended_with_separator = false;

    while let Some(next) = stream.peek().copied() {
        if matches!(next, ',' | '}' | '{') {
            break;
        }

        if matches!(next, '/' | '\\') {
            let pushed_component = push_component_if_present(
                &mut components,
                &mut component_buffer,
                false,
                &mut seen_non_relative_component,
                stream,
                string_table,
            )?;

            if !pushed_component {
                return_syntax_error!(
                    "Empty path component is not allowed here.",
                    stream.new_location().to_error_location(string_table), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Remove repeated separators and keep exactly one '/' between components",
                    }
                )
            }

            stream.next();
            ended_with_separator = true;
            continue;
        }

        component_buffer.push(next);
        stream.next();

        if !next.is_whitespace() {
            ended_with_separator = false;
        }
    }

    let _ = push_component_if_present(
        &mut components,
        &mut component_buffer,
        false,
        &mut seen_non_relative_component,
        stream,
        string_table,
    )?;

    Ok(ParsedGroupedPrefix {
        components,
        ended_with_separator,
    })
}

/// WHAT: Finalizes the buffered component, validates it, and interns it.
/// WHY: Grouped and non-grouped parsing both need identical component validation/normalization,
///      so this helper is the shared boundary for those rules.
fn push_component_if_present(
    components: &mut PathComponents,
    component_buffer: &mut String,
    allow_leading_relative_markers: bool,
    seen_non_relative_component: &mut bool,
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<bool, CompilerError> {
    let trimmed = component_buffer.trim();

    if trimmed.is_empty() {
        component_buffer.clear();
        return Ok(false);
    }

    let allow_relative_marker = allow_leading_relative_markers && !*seen_non_relative_component;

    validate_path_component(trimmed, allow_relative_marker, stream, string_table)?;

    if trimmed != "." && trimmed != ".." {
        *seen_non_relative_component = true;
    }

    components.push(string_table.intern(trimmed));
    component_buffer.clear();

    Ok(true)
}

fn validate_path_component(
    component: &str,
    allow_relative_marker: bool,
    stream: &mut TokenStream,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if component == "." || component == ".." {
        if allow_relative_marker {
            return Ok(());
        }

        return_syntax_error!(
            format!("Invalid path component '{}'.", component),
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Use '.' and '..' only as leading relative path markers",
            }
        )
    }

    if component.ends_with('.') {
        return_syntax_error!(
            format!("Invalid path component '{}'.", component),
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Remove trailing '.' from this path component",
            }
        )
    }

    if component.chars().any(|c| !is_valid_component_char(c)) {
        return_syntax_error!(
            format!("Invalid path component '{}'.", component),
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Use path components without syntax delimiters or cross-platform reserved filename characters",
            }
        )
    }

    if is_reserved_windows_name(component) {
        return_syntax_error!(
            format!("Invalid path component '{}'.", component),
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Use a file or directory name that is not a reserved Windows device name",
            }
        )
    }

    Ok(())
}

fn is_valid_component_char(c: char) -> bool {
    if c.is_control() {
        return false;
    }

    // Keep plain spaces as data in path components, but reject other whitespace.
    if c.is_whitespace() && c != ' ' {
        return false;
    }

    !matches!(
        c,
        '[' | ']'
            | '{'
            | '}'
            | ','
            | '('
            | ')'
            | '/'
            | '\\'
            | '<'
            | '>'
            | ':'
            | '"'
            | '|'
            | '?'
            | '*'
    )
}

fn is_reserved_windows_name(component: &str) -> bool {
    let prefix = component.split('.').next().unwrap_or(component);

    matches!(
        prefix.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn consume_all_whitespace(stream: &mut TokenStream) {
    while stream
        .peek()
        .is_some_and(|character| character.is_whitespace())
    {
        stream.next();
    }
}

#[cfg(test)]
#[path = "tests/paths_tests.rs"]
mod paths_tests;
