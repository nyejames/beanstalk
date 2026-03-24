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
    GroupStart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParseComponentContext {
    OrdinaryPath,
    GroupedEntry,
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

#[derive(Debug)]
struct ParsedComponent {
    value: String,
    was_quoted: bool,
}

pub fn parse_file_path(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<Token, CompilerError> {
    // Path syntax accepted by the tokenizer.
    //
    // Canonical examples:
    // @path/to/file
    // @docs/"my file.md"
    //
    // @docs {
    //     intro.md,
    //     "my folder"/"my file.md",
    //     guides {
    //         ownership.md,
    //         memory.md,
    //     },
    // }

    consume_non_newline_whitespace(stream);

    let parsed_prefix = parse_path_prefix(stream, string_table)?;

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
        parsed_paths.push(InternedPath::from_components(parsed_prefix.components));
        return_token!(TokenKind::Path(parsed_paths), stream);
    }

    // Consume the opening grouped brace so the grouped parser starts at the first entry.
    stream.next();
    let grouped_suffixes = parse_grouped_block(stream, string_table)?;

    for suffix_components in grouped_suffixes {
        let mut full_components = parsed_prefix.components.clone();
        full_components.extend(suffix_components);
        parsed_paths.push(InternedPath::from_components(full_components));
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
                PrimarySuggestion => "Add an import path like '@folder/file' after 'import'",
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
                PrimarySuggestion => "Use import syntax like 'import @folder/file'",
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
/// WHY: Keeps context-sensitive ordinary-path stop conditions isolated from grouped expansion.
fn parse_path_prefix(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<ParsedPathPrefix, CompilerError> {
    let mut components = Vec::with_capacity(2);
    let mut seen_non_relative_component = false;
    let mut ended_with_separator = false;
    let mut expect_component = true;

    loop {
        if expect_component {
            consume_non_newline_whitespace(stream);

            let Some(next) = stream.peek().copied() else {
                return Ok(ParsedPathPrefix {
                    components,
                    stop_reason: PathStopReason::EndOfInput,
                    ended_with_separator,
                });
            };

            if let Some(stop_reason) = ordinary_stop_reason(stream.mode, next) {
                return Ok(ParsedPathPrefix {
                    components,
                    stop_reason,
                    ended_with_separator,
                });
            }

            if matches!(next, '/' | '\\') {
                return_syntax_error!(
                    "Empty path component is not allowed here.",
                    stream.new_location().to_error_location(string_table), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Remove repeated separators and keep exactly one '/' between components",
                    }
                )
            }

            let parsed_component =
                parse_component(stream, ParseComponentContext::OrdinaryPath, string_table)?;
            push_validated_component(
                &mut components,
                parsed_component,
                true,
                &mut seen_non_relative_component,
                stream,
                string_table,
            )?;

            expect_component = false;
            ended_with_separator = false;
            continue;
        }

        let skipped_whitespace = consume_non_newline_whitespace(stream);

        let Some(next) = stream.peek().copied() else {
            return Ok(ParsedPathPrefix {
                components,
                stop_reason: PathStopReason::EndOfInput,
                ended_with_separator,
            });
        };

        if let Some(stop_reason) = ordinary_stop_reason(stream.mode, next) {
            return Ok(ParsedPathPrefix {
                components,
                stop_reason,
                ended_with_separator,
            });
        }

        if matches!(next, '/' | '\\') {
            stream.next();
            expect_component = true;
            ended_with_separator = true;
            continue;
        }

        if skipped_whitespace {
            return_syntax_error!(
                "Path components with whitespace must be quoted.",
                stream.new_location().to_error_location(string_table), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Quote this path component, for example: \"my file.md\".",
                }
            )
        }

        return_syntax_error!(
            "Path components must be separated by '/'.",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Insert '/' between path components",
            }
        )
    }
}

/// WHAT: Parses one grouped `{ ... }` block into expanded relative-path suffixes.
/// WHY: Grouped path syntax is sugar; this recursive parser expands nested groups into
///      explicit suffix component lists that callers prepend to a base prefix.
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
/// WHY: Grouped entries share the same component parsing and validation rules as ordinary paths.
fn parse_grouped_entry_prefix(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<ParsedGroupedPrefix, CompilerError> {
    let mut components = Vec::new();
    let mut seen_non_relative_component = false;
    let mut ended_with_separator = false;
    let mut expect_component = true;

    loop {
        if expect_component {
            consume_all_whitespace(stream);

            let Some(next) = stream.peek().copied() else {
                return Ok(ParsedGroupedPrefix {
                    components,
                    ended_with_separator,
                });
            };

            if is_grouped_entry_stop_char(next) {
                return Ok(ParsedGroupedPrefix {
                    components,
                    ended_with_separator,
                });
            }

            if matches!(next, '/' | '\\') {
                return_syntax_error!(
                    "Empty path component is not allowed here.",
                    stream.new_location().to_error_location(string_table), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Remove repeated separators and keep exactly one '/' between components",
                    }
                )
            }

            let parsed_component =
                parse_component(stream, ParseComponentContext::GroupedEntry, string_table)?;
            push_validated_component(
                &mut components,
                parsed_component,
                false,
                &mut seen_non_relative_component,
                stream,
                string_table,
            )?;

            expect_component = false;
            ended_with_separator = false;
            continue;
        }

        let skipped_whitespace = consume_all_whitespace(stream);

        let Some(next) = stream.peek().copied() else {
            return Ok(ParsedGroupedPrefix {
                components,
                ended_with_separator,
            });
        };

        if is_grouped_entry_stop_char(next) {
            return Ok(ParsedGroupedPrefix {
                components,
                ended_with_separator,
            });
        }

        if matches!(next, '/' | '\\') {
            stream.next();
            expect_component = true;
            ended_with_separator = true;
            continue;
        }

        if skipped_whitespace {
            return_syntax_error!(
                "Path components with whitespace must be quoted.",
                stream.new_location().to_error_location(string_table), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Quote this path component, for example: \"my file.md\".",
                }
            )
        }

        return_syntax_error!(
            "Path components must be separated by '/'.",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Insert '/' between path components",
            }
        )
    }
}

/// WHAT: Parses exactly one path component (bare or quoted) from the current stream position.
/// WHY: Ordinary paths and grouped entries must share the same component grammar and escapes.
fn parse_component(
    stream: &mut TokenStream,
    context: ParseComponentContext,
    string_table: &StringTable,
) -> Result<ParsedComponent, CompilerError> {
    if stream.peek() == Some(&'"') {
        return parse_quoted_component(stream, string_table);
    }

    parse_bare_component(stream, context, string_table)
}

/// WHAT: Parses a quoted path component using path-literal escapes.
/// WHY: Quoted components are the only syntax that allows whitespace inside a component.
fn parse_quoted_component(
    stream: &mut TokenStream,
    string_table: &StringTable,
) -> Result<ParsedComponent, CompilerError> {
    let Some('"') = stream.peek().copied() else {
        return Err(CompilerError::compiler_error(
            "Quoted path component parsing expected to start on '\"'.",
        ));
    };

    stream.next();
    let mut value = String::new();

    loop {
        let Some(next) = stream.peek().copied() else {
            return_syntax_error!(
                "Unclosed quoted path component.",
                stream.new_location().to_error_location(string_table), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Add a closing double quote to finish this path component.",
                    SuggestedInsertion => "\"",
                }
            );
        };

        if next == '"' {
            stream.next();
            return Ok(ParsedComponent {
                value,
                was_quoted: true,
            });
        }

        if next == '\\' {
            stream.next();

            let Some(escaped) = stream.peek().copied() else {
                return_syntax_error!(
                    "Unclosed quoted path component.",
                    stream.new_location().to_error_location(string_table), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Add a closing double quote to finish this path component.",
                        SuggestedInsertion => "\"",
                    }
                );
            };

            match escaped {
                '"' | '\\' => {
                    value.push(escaped);
                    stream.next();
                }
                _ => {
                    return_syntax_error!(
                        "Invalid escape in quoted path component. Only '\\\"' and '\\\\' are supported.",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Use '\\\"' for a quote or '\\\\' for a backslash in quoted path components",
                        }
                    )
                }
            }

            continue;
        }

        value.push(next);
        stream.next();
    }
}

/// WHAT: Parses an unquoted path component and enforces quote-required whitespace rules.
/// WHY: Bare components must remain unambiguous path tokens without internal whitespace.
fn parse_bare_component(
    stream: &mut TokenStream,
    context: ParseComponentContext,
    string_table: &StringTable,
) -> Result<ParsedComponent, CompilerError> {
    let mut value = String::new();

    loop {
        let Some(next) = stream.peek().copied() else {
            break;
        };

        if is_component_terminator(stream.mode, context, next) || next.is_whitespace() {
            break;
        }

        value.push(next);
        stream.next();
    }

    if value.is_empty() {
        return_syntax_error!(
            "Path component cannot be empty.",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Provide a valid path component",
            }
        );
    }

    if stream
        .peek()
        .is_some_and(|character| character.is_whitespace())
    {
        match context {
            ParseComponentContext::OrdinaryPath => {
                consume_non_newline_whitespace(stream);
            }
            ParseComponentContext::GroupedEntry => {
                consume_all_whitespace(stream);
            }
        }

        let mode = stream.mode;
        if stream
            .peek()
            .is_some_and(|next| !is_component_terminator(mode, context, *next))
        {
            return_syntax_error!(
                "Path components with whitespace must be quoted.",
                stream.new_location().to_error_location(string_table), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Quote this path component, for example: \"my file.md\".",
                }
            );
        }
    }

    Ok(ParsedComponent {
        value,
        was_quoted: false,
    })
}

/// WHAT: Validates and interns one parsed component.
/// WHY: Keeps grouped and ordinary paths aligned on one validation boundary.
fn push_validated_component(
    components: &mut PathComponents,
    parsed_component: ParsedComponent,
    allow_leading_relative_markers: bool,
    seen_non_relative_component: &mut bool,
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    let allow_relative_marker = allow_leading_relative_markers && !*seen_non_relative_component;

    validate_path_component(
        &parsed_component.value,
        allow_relative_marker,
        parsed_component.was_quoted,
        stream,
        string_table,
    )?;

    if parsed_component.value != "." && parsed_component.value != ".." {
        *seen_non_relative_component = true;
    }

    components.push(string_table.intern(&parsed_component.value));
    Ok(())
}

fn validate_path_component(
    component: &str,
    allow_relative_marker: bool,
    was_quoted: bool,
    stream: &mut TokenStream,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if component.is_empty() {
        return_syntax_error!(
            "Path component cannot be empty.",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Provide a valid path component",
            }
        )
    }

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

    if component
        .chars()
        .any(|character| !is_valid_component_char(character, was_quoted))
    {
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

fn is_valid_component_char(character: char, allow_spaces: bool) -> bool {
    if character.is_control() {
        return false;
    }

    if character.is_whitespace() {
        if allow_spaces && character == ' ' {
            return true;
        }

        return false;
    }

    !matches!(
        character,
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

fn ordinary_stop_reason(mode: TokenizeMode, character: char) -> Option<PathStopReason> {
    if matches!(character, '\n' | '\r') {
        return Some(PathStopReason::Newline);
    }

    if mode == TokenizeMode::TemplateHead && matches!(character, ']' | ':') {
        return Some(PathStopReason::TemplateHeadDelimiter);
    }

    if matches!(character, ',' | '}') {
        return Some(PathStopReason::ConfigDelimiter);
    }

    if character == '{' {
        return Some(PathStopReason::GroupStart);
    }

    None
}

fn is_grouped_entry_stop_char(character: char) -> bool {
    matches!(character, ',' | '}' | '{')
}

fn is_component_terminator(
    mode: TokenizeMode,
    context: ParseComponentContext,
    character: char,
) -> bool {
    if matches!(character, '/' | '\\') {
        return true;
    }

    match context {
        ParseComponentContext::OrdinaryPath => ordinary_stop_reason(mode, character).is_some(),
        ParseComponentContext::GroupedEntry => is_grouped_entry_stop_char(character),
    }
}

fn consume_non_newline_whitespace(stream: &mut TokenStream) -> bool {
    let mut consumed = false;

    while stream
        .peek()
        .is_some_and(|character| character.is_non_newline_whitespace())
    {
        stream.next();
        consumed = true;
    }

    consumed
}

fn consume_all_whitespace(stream: &mut TokenStream) -> bool {
    let mut consumed = false;

    while stream
        .peek()
        .is_some_and(|character| character.is_whitespace())
    {
        stream.next();
        consumed = true;
    }

    consumed
}

#[cfg(test)]
#[path = "tests/paths_tests.rs"]
mod paths_tests;
