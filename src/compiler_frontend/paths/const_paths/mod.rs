#![allow(clippy::result_large_err)]
//! Beanstalk path syntax parsing for path literals and imports.
//!
//! This parser sits directly on tokenizer tokens and returns typed `CompilerDiagnostic` values for
//! user-authored path mistakes. The large-result lint is allowed at this file boundary because
//! boxing diagnostics here would make the tokenizer-facing API less direct without reducing any
//! stage-owned complexity.

use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, ImportClauseKind, InvalidImportClauseReason, PathKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::keywords::{is_keyword, is_valid_identifier};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::lexer::{
    consume_all_whitespace, consume_non_newline_whitespace,
};
use crate::compiler_frontend::tokenizer::tokens::{
    PathTokenItem, SourceLocation, Token, TokenKind, TokenStream, TokenizeMode,
};
use crate::return_token;

mod import_clauses;

pub use import_clauses::*;

type PathComponents = Vec<StringId>;

/// WHAT: One expanded entry from a grouped block, with optional alias and source locations.
/// WHY: Grouped import aliases must preserve per-entry metadata through tokenization.
#[derive(Debug)]
struct GroupedPathExpansion {
    components: PathComponents,
    alias: Option<StringId>,
    path_location: SourceLocation,
    alias_location: Option<SourceLocation>,
}

type RelativePathExpansions = Vec<GroupedPathExpansion>;

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

/// WHAT: Result of parsing one grouped entry, including optional alias.
/// WHY: Grouped entries may end with `as alias`; this captures both the path
///      components and the alias metadata.
#[derive(Debug)]
struct ParsedGroupedEntry {
    components: PathComponents,
    ended_with_separator: bool,
    alias: Option<StringId>,
    path_location: SourceLocation,
    alias_location: Option<SourceLocation>,
}

#[derive(Debug)]
struct ParsedComponent {
    value: String,
    was_quoted: bool,
}

pub fn parse_file_path(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<Token, CompilerDiagnostic> {
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

    // WHAT: Accept exact `@/` as the singleton public-root path literal.
    // WHY: Site templates commonly need the public root itself, and the existing
    // empty `InternedPath` representation models that case cleanly without
    // expanding path grammar into a generic slash-prefixed family.
    if stream.peek() == Some(&'/') {
        stream.next();

        match stream.peek().copied() {
            None => return_token!(
                TokenKind::Path(vec![PathTokenItem {
                    path: InternedPath::new(),
                    alias: None,
                    path_location: SourceLocation::new(
                        stream.file_path.to_owned(),
                        stream.start_position,
                        stream.position
                    ),
                    alias_location: None,
                    from_grouped: false,
                }]),
                stream
            ),
            Some(next) => {
                if let Some(stop_reason) = ordinary_stop_reason(stream.mode, next)
                    && stop_reason != PathStopReason::GroupStart
                {
                    return_token!(
                        TokenKind::Path(vec![PathTokenItem {
                            path: InternedPath::new(),
                            alias: None,
                            path_location: SourceLocation::new(
                                stream.file_path.to_owned(),
                                stream.start_position,
                                stream.position
                            ),
                            alias_location: None,
                            from_grouped: false,
                        }]),
                        stream
                    );
                }

                return Err(CompilerDiagnostic::invalid_path(
                    PathKind::OnlyRootSlashSupported,
                    stream.new_location(),
                ));
            }
        }
    }

    let parsed_prefix = parse_path_prefix(stream, string_table)?;

    if parsed_prefix.components.is_empty() {
        return Err(CompilerDiagnostic::invalid_path(
            PathKind::Empty,
            stream.new_location(),
        ));
    }

    if parsed_prefix.ended_with_separator && parsed_prefix.stop_reason == PathStopReason::GroupStart
    {
        return Err(CompilerDiagnostic::invalid_path(
            PathKind::SlashBeforeGroup,
            stream.new_location(),
        ));
    }

    if parsed_prefix.ended_with_separator {
        return Err(CompilerDiagnostic::invalid_path(
            PathKind::TrailingSeparator,
            stream.new_location(),
        ));
    }

    let mut parsed_paths = Vec::with_capacity(1);

    if parsed_prefix.stop_reason != PathStopReason::GroupStart {
        let path = InternedPath::from_components(parsed_prefix.components);
        let path_location = SourceLocation::new(
            stream.file_path.to_owned(),
            stream.start_position,
            stream.position,
        );
        parsed_paths.push(PathTokenItem {
            path,
            alias: None,
            path_location,
            alias_location: None,
            from_grouped: false,
        });
        return_token!(TokenKind::Path(parsed_paths), stream);
    }

    // Consume the opening grouped brace so the grouped parser starts at the first entry.
    stream.next();
    let grouped_suffixes = parse_grouped_block(stream, string_table)?;

    for suffix in grouped_suffixes {
        let mut full_components = parsed_prefix.components.clone();
        full_components.extend(suffix.components);
        let path = InternedPath::from_components(full_components);
        parsed_paths.push(PathTokenItem {
            path,
            alias: suffix.alias,
            path_location: suffix.path_location,
            alias_location: suffix.alias_location,
            from_grouped: true,
        });
    }

    return_token!(TokenKind::Path(parsed_paths), stream)
}

/// WHAT: Parses the base path prefix before an optional grouped block.
/// WHY: Keeps context-sensitive ordinary-path stop conditions isolated from grouped expansion.
fn parse_path_prefix(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<ParsedPathPrefix, CompilerDiagnostic> {
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
                return Err(CompilerDiagnostic::invalid_path(
                    PathKind::EmptyComponent,
                    stream.new_location(),
                ));
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

        // Stop path parsing at the `as` keyword (used in import aliases).
        // WHAT: `as` is a language keyword, not a valid path component.
        // WHY: without this, `import @path/symbol as alias` tokenizes the `as alias`
        //      as part of the path, producing a confusing "whitespace must be quoted" error.
        if next == 'a' && peek_keyword_as(stream) {
            return Ok(ParsedPathPrefix {
                components,
                stop_reason: PathStopReason::EndOfInput,
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
            return Err(CompilerDiagnostic::invalid_path(
                PathKind::WhitespaceMustBeQuoted,
                stream.new_location(),
            ));
        }

        return Err(CompilerDiagnostic::invalid_path(
            PathKind::MissingSeparator,
            stream.new_location(),
        ));
    }
}

/// WHAT: Parses one grouped `{ ... }` block into expanded relative-path suffixes.
/// WHY: Grouped path syntax is sugar; this recursive parser expands nested groups into
///      explicit suffix component lists that callers prepend to a base prefix.
fn parse_grouped_block(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<RelativePathExpansions, CompilerDiagnostic> {
    let mut expanded_suffixes: RelativePathExpansions = Vec::new();
    let mut saw_entry = false;
    let mut expect_entry = true;

    loop {
        consume_all_whitespace(stream);

        let Some(next) = stream.peek().copied() else {
            return Err(CompilerDiagnostic::invalid_path(
                PathKind::MissingClosingBrace,
                stream.new_location(),
            ));
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
                    return Err(CompilerDiagnostic::invalid_path(
                        PathKind::EntriesNeedCommas,
                        stream.new_location(),
                    ));
                }
            }
        }

        if next == '}' {
            if !saw_entry {
                return Err(CompilerDiagnostic::invalid_path(
                    PathKind::EmptyGroupedBlock,
                    stream.new_location(),
                ));
            }

            stream.next();
            break;
        }

        if next == ',' {
            return Err(CompilerDiagnostic::invalid_path(
                PathKind::MultipleCommas,
                stream.new_location(),
            ));
        }

        let parsed_entry = parse_grouped_entry(stream, string_table)?;

        consume_all_whitespace(stream);

        if stream.peek() == Some(&'{') {
            if parsed_entry.alias.is_some() {
                return Err(CompilerDiagnostic::invalid_path(
                    PathKind::AliasOnlyOnLeaf,
                    stream.new_location(),
                ));
            }

            if parsed_entry.ended_with_separator {
                return Err(CompilerDiagnostic::invalid_path(
                    PathKind::SlashBeforeGroup,
                    stream.new_location(),
                ));
            }

            if parsed_entry.components.is_empty() {
                return Err(CompilerDiagnostic::invalid_path(
                    PathKind::NestedGroupNeedsPrefix,
                    stream.new_location(),
                ));
            }

            stream.next();
            let child_suffixes = parse_grouped_block(stream, string_table)?;

            for child_suffix in child_suffixes {
                let mut combined = parsed_entry.components.clone();
                combined.extend(child_suffix.components);
                expanded_suffixes.push(GroupedPathExpansion {
                    components: combined,
                    alias: child_suffix.alias,
                    path_location: child_suffix.path_location,
                    alias_location: child_suffix.alias_location,
                });
            }
        } else {
            if parsed_entry.ended_with_separator {
                return Err(CompilerDiagnostic::invalid_path(
                    PathKind::GroupedPrefixTrailingSeparator,
                    stream.new_location(),
                ));
            }

            if parsed_entry.components.is_empty() {
                return Err(CompilerDiagnostic::invalid_path(
                    PathKind::GroupedEntryEmpty,
                    stream.new_location(),
                ));
            }

            expanded_suffixes.push(GroupedPathExpansion {
                components: parsed_entry.components,
                alias: parsed_entry.alias,
                path_location: parsed_entry.path_location,
                alias_location: parsed_entry.alias_location,
            });
        }

        saw_entry = true;
        expect_entry = false;
    }

    Ok(expanded_suffixes)
}

/// WHAT: Validates that a grouped import alias is a valid local binding name.
/// WHY: Grouped aliases must follow the same identifier rules as ordinary
///      `TokenKind::Symbol` names so invalid names like `bad-name` or `123x`
///      are rejected with a targeted diagnostic.
fn validate_import_alias_symbol(
    alias: &str,
    location: SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    if !is_valid_identifier(alias) {
        return Err(CompilerDiagnostic::invalid_import_clause(
            ImportClauseKind::Alias,
            InvalidImportClauseReason::AliasNotValidIdentifier,
            location,
        ));
    }

    if is_keyword(alias) {
        return Err(CompilerDiagnostic::invalid_import_clause(
            ImportClauseKind::Alias,
            InvalidImportClauseReason::AliasIsKeyword,
            location,
        ));
    }

    Ok(())
}

/// WHAT: Parses one grouped entry, including optional `as alias`.
/// WHY: Grouped import aliases require each entry to carry its own alias metadata.
fn parse_grouped_entry(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<ParsedGroupedEntry, CompilerDiagnostic> {
    let entry_start = stream.position;
    let mut components = Vec::new();
    let mut seen_non_relative_component = false;
    let mut ended_with_separator = false;
    let mut expect_component = true;

    // Parse path components until a grouped-entry stop character.
    loop {
        if expect_component {
            consume_all_whitespace(stream);

            let Some(next) = stream.peek().copied() else {
                break;
            };

            if is_grouped_entry_stop_char(stream, next, true) {
                break;
            }

            if matches!(next, '/' | '\\') {
                return Err(CompilerDiagnostic::invalid_path(
                    PathKind::EmptyComponent,
                    stream.new_location(),
                ));
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
            break;
        };

        if is_grouped_entry_stop_char(stream, next, true) {
            break;
        }

        if matches!(next, '/' | '\\') {
            stream.next();
            expect_component = true;
            ended_with_separator = true;
            continue;
        }

        if skipped_whitespace {
            return Err(CompilerDiagnostic::invalid_path(
                PathKind::WhitespaceMustBeQuoted,
                stream.new_location(),
            ));
        }

        return Err(CompilerDiagnostic::invalid_path(
            PathKind::MissingSeparator,
            stream.new_location(),
        ));
    }

    let path_end = stream.position;

    // Check for optional `as alias` after the path components.
    let mut alias = None;
    let mut alias_location = None;

    consume_all_whitespace(stream);
    if stream.peek().copied() == Some('a') && peek_keyword_as(stream) {
        consume_keyword_as(stream);
        consume_all_whitespace(stream);

        // Give a targeted diagnostic when the alias name is missing entirely.
        if let Some(next) = stream.peek().copied()
            && is_grouped_entry_stop_char(stream, next, true)
        {
            return Err(CompilerDiagnostic::invalid_import_clause(
                ImportClauseKind::Alias,
                InvalidImportClauseReason::MissingAlias,
                stream.new_location(),
            ));
        }

        let alias_start = stream.position;
        let alias_component =
            parse_bare_component(stream, ParseComponentContext::GroupedEntry, string_table)?;
        let alias_end = stream.position;
        let location = SourceLocation::new(stream.file_path.to_owned(), alias_start, alias_end);

        validate_import_alias_symbol(&alias_component.value, location.clone())?;

        alias = Some(string_table.intern(&alias_component.value));
        alias_location = Some(location);

        // Reject a second `as` keyword inside a grouped entry.
        consume_all_whitespace(stream);
        if stream.peek().copied() == Some('a') && peek_keyword_as(stream) {
            return Err(CompilerDiagnostic::invalid_import_clause(
                ImportClauseKind::Grouped,
                InvalidImportClauseReason::DoubleAliasInGroupedEntry,
                stream.new_location(),
            ));
        }
    }

    Ok(ParsedGroupedEntry {
        components,
        ended_with_separator,
        alias,
        path_location: SourceLocation::new(stream.file_path.to_owned(), entry_start, path_end),
        alias_location,
    })
}

/// WHAT: Parses exactly one path component (bare or quoted) from the current stream position.
/// WHY: Ordinary paths and grouped entries must share the same component grammar and escapes.
fn parse_component(
    stream: &mut TokenStream,
    context: ParseComponentContext,
    string_table: &StringTable,
) -> Result<ParsedComponent, CompilerDiagnostic> {
    if stream.peek() == Some(&'"') {
        return parse_quoted_component(stream, string_table);
    }

    parse_bare_component(stream, context, string_table)
}

/// WHAT: Parses a quoted path component using path-literal escapes.
/// WHY: Quoted components are the only syntax that allows whitespace inside a component.
fn parse_quoted_component(
    stream: &mut TokenStream,
    _string_table: &StringTable,
) -> Result<ParsedComponent, CompilerDiagnostic> {
    assert_eq!(
        stream.peek().copied(),
        Some('"'),
        "Quoted path component parsing expected to start on '\"'."
    );

    stream.next();
    let mut value = String::new();

    loop {
        let Some(next) = stream.peek().copied() else {
            return Err(CompilerDiagnostic::invalid_path(
                PathKind::MissingClosingQuote,
                stream.new_location(),
            ));
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
                return Err(CompilerDiagnostic::invalid_path(
                    PathKind::MissingClosingQuote,
                    stream.new_location(),
                ));
            };

            match escaped {
                '"' | '\\' => {
                    value.push(escaped);
                    stream.next();
                }
                _ => {
                    return Err(CompilerDiagnostic::invalid_path(
                        PathKind::InvalidEscape,
                        stream.new_location(),
                    ));
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
    _string_table: &StringTable,
) -> Result<ParsedComponent, CompilerDiagnostic> {
    let mut value = String::new();

    while let Some(next) = stream.peek().copied() {
        if next.is_whitespace() || is_component_terminator(stream, context, next, value.is_empty())
        {
            break;
        }

        value.push(next);
        stream.next();
    }

    if value.is_empty() {
        return Err(CompilerDiagnostic::invalid_path(
            PathKind::EmptyComponent,
            stream.new_location(),
        ));
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

        if let Some(next) = stream.peek().copied()
            && !is_component_terminator(stream, context, next, true)
        {
            // Allow the `as` keyword to follow a bare path component without quoting.
            // WHAT: `import @path/symbol as alias` is valid syntax; `as` is a keyword.
            // WHY: without this, the path tokenizer treats `as` as an unquoted multi-word
            //      path component and emits a confusing error.
            if matches!(context, ParseComponentContext::OrdinaryPath)
                && next == 'a'
                && peek_keyword_as(stream)
            {
                // Return the component normally; `parse_path_prefix` will stop at `as`.
            } else {
                return Err(CompilerDiagnostic::invalid_path(
                    PathKind::WhitespaceMustBeQuoted,
                    stream.new_location(),
                ));
            }
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
) -> Result<(), CompilerDiagnostic> {
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
    _string_table: &StringTable,
) -> Result<(), CompilerDiagnostic> {
    if component.is_empty() {
        return Err(CompilerDiagnostic::invalid_path(
            PathKind::EmptyComponent,
            stream.new_location(),
        ));
    }

    if component == "." || component == ".." {
        if allow_relative_marker {
            return Ok(());
        }

        return Err(CompilerDiagnostic::invalid_path(
            PathKind::InvalidComponent,
            stream.new_location(),
        ));
    }

    if component.ends_with('.') {
        return Err(CompilerDiagnostic::invalid_path(
            PathKind::InvalidComponent,
            stream.new_location(),
        ));
    }

    if component
        .chars()
        .any(|character| !is_valid_component_char(character, was_quoted))
    {
        return Err(CompilerDiagnostic::invalid_path(
            PathKind::InvalidComponent,
            stream.new_location(),
        ));
    }

    if is_reserved_windows_name(component) {
        return Err(CompilerDiagnostic::invalid_path(
            PathKind::InvalidComponent,
            stream.new_location(),
        ));
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

/// WHAT: Peeks ahead to check if the stream currently points at the keyword `as`.
/// WHY: `as` is a language keyword used for import aliases and type aliases. It must not
///      be consumed as part of a path component.
fn peek_keyword_as(stream: &TokenStream) -> bool {
    // stream.peek() is already 'a'; check the next character.
    let mut chars = stream.chars.clone();
    chars.next(); // skip 'a'
    let Some(second) = chars.next() else {
        return false;
    };
    if second != 's' {
        return false;
    }
    // `as` must be followed by whitespace, a path terminator, or EOF.
    match chars.next() {
        None => true,
        Some(c) => c.is_whitespace() || ordinary_stop_reason(stream.mode, c).is_some(),
    }
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

/// WHAT: Checks whether the current character ends a grouped entry component.
/// WHY: Entries stop at commas, braces, or the `as` keyword. The `as` check needs
///      the stream to peek ahead and verify it appears before a new component, not inside a
///      component like `ExportedAlias`.
fn is_grouped_entry_stop_char(
    stream: &TokenStream,
    character: char,
    component_is_empty: bool,
) -> bool {
    if matches!(character, ',' | '}' | '{') {
        return true;
    }

    if component_is_empty && character == 'a' && peek_keyword_as(stream) {
        return true;
    }

    false
}

/// WHAT: Consumes the `as` keyword from the stream.
/// WHY: After `peek_keyword_as` confirms the keyword is present, this advances past it.
fn consume_keyword_as(stream: &mut TokenStream) {
    stream.next(); // 'a'
    stream.next(); // 's'
}

fn is_component_terminator(
    stream: &TokenStream,
    context: ParseComponentContext,
    character: char,
    component_is_empty: bool,
) -> bool {
    if matches!(character, '/' | '\\') {
        return true;
    }

    match context {
        ParseComponentContext::OrdinaryPath => {
            ordinary_stop_reason(stream.mode, character).is_some()
        }
        ParseComponentContext::GroupedEntry => {
            is_grouped_entry_stop_char(stream, character, component_is_empty)
        }
    }
}

#[cfg(test)]
#[path = "../tests/paths_tests.rs"]
mod paths_tests;
