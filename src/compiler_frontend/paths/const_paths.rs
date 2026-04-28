use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::lexer::{
    consume_all_whitespace, consume_non_newline_whitespace, is_keyword, is_valid_identifier,
};
use crate::compiler_frontend::tokenizer::tokens::{
    PathTokenItem, SourceLocation, Token, TokenKind, TokenStream, TokenizeMode,
};
use crate::{return_syntax_error, return_token};

type PathComponents = Vec<StringId>;

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

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

                return_syntax_error!(
                    "Only exact \"@/\" is supported as the public root path. Use '@name/...' for rooted paths.",
                    stream.new_location(), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Use exactly '@/' for the public root, or use '@name/...' for non-root paths",
                    }
                )
            }
        }
    }

    let parsed_prefix = parse_path_prefix(stream, string_table)?;

    if parsed_prefix.components.is_empty() {
        return_syntax_error!(
            "Path cannot be empty.",
            stream.new_location(), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Provide a valid file path",
            }
        )
    }

    if parsed_prefix.ended_with_separator && parsed_prefix.stop_reason == PathStopReason::GroupStart
    {
        return_syntax_error!(
            "Slash-before-group syntax is not supported. Use 'base { ... }'.",
            stream.new_location(), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Remove the separator before '{' and write the group as 'base { ... }'",
            }
        )
    }

    if parsed_prefix.ended_with_separator {
        return_syntax_error!(
            "Path cannot end with a separator.",
            stream.new_location(), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Remove the trailing '/' or '\\' from this path",
            }
        )
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

#[derive(Clone, Debug)]
pub struct ParsedImportItem {
    pub path: InternedPath,
    pub alias: Option<StringId>,
    pub path_location: SourceLocation,
    pub alias_location: Option<SourceLocation>,
}

pub fn parse_import_clause_items(
    tokens: &[Token],
    start_index: usize,
    string_table: &mut StringTable,
) -> Result<(Vec<ParsedImportItem>, usize), CompilerError> {
    parse_path_clause_items(
        tokens,
        start_index,
        TokenKind::Import,
        "import",
        string_table,
    )
}

pub fn parse_re_export_clause_items(
    tokens: &[Token],
    start_index: usize,
    string_table: &mut StringTable,
) -> Result<(Vec<ParsedImportItem>, usize), CompilerError> {
    parse_path_clause_items(
        tokens,
        start_index,
        TokenKind::Import,
        "#import",
        string_table,
    )
}

fn parse_path_clause_items(
    tokens: &[Token],
    start_index: usize,
    expected_token_kind: TokenKind,
    clause_name: &str,
    _string_table: &mut StringTable,
) -> Result<(Vec<ParsedImportItem>, usize), CompilerError> {
    let Some(clause_token) = tokens.get(start_index) else {
        return Err(CompilerError::compiler_error(format!(
            "{clause_name} clause parsing started past the end of the token stream."
        )));
    };

    if clause_token.kind != expected_token_kind {
        return Err(CompilerError::compiler_error(format!(
            "{clause_name} clause parsing expected to start on a '{clause_name}' token."
        )));
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
            &format!("Expected a path after the '{clause_name}' keyword."),
            clause_token.location.clone(), {
                CompilationStage => "Header Parsing",
                PrimarySuggestion => &format!("Add a path like '@folder/file' after '{clause_name}'"),
            }
        );
    };

    let TokenKind::Path(items) = &path_token.kind else {
        return_syntax_error!(
            format!(
                "Expected a path after the '{clause_name}' keyword, found '{:?}'.",
                path_token.kind
            ),
            path_token.location.clone(), {
                CompilationStage => "Header Parsing",
                PrimarySuggestion => &format!("Use syntax like '{clause_name} @folder/file'"),
            }
        );
    };

    let mut index = index + 1;
    let mut trailing_alias: Option<StringId> = None;
    let mut trailing_alias_location: Option<SourceLocation> = None;

    // Check for `as alias_name` after the path token.
    if tokens
        .get(index)
        .is_some_and(|token| matches!(token.kind, TokenKind::As))
    {
        index += 1;
        let Some(alias_token) = tokens.get(index) else {
            return_syntax_error!(
                &format!("Expected alias name after `as` in {clause_name}."),
                path_token.location.clone(), {
                    CompilationStage => "Header Parsing",
                    PrimarySuggestion => &format!("Provide an alias name after `as`, e.g. `{clause_name} @path/symbol as local_name`"),
                }
            );
        };
        let TokenKind::Symbol(alias_name) = alias_token.kind else {
            return_syntax_error!(
                &format!("Expected alias name after `as` in {clause_name}."),
                alias_token.location.clone(), {
                    CompilationStage => "Header Parsing",
                    PrimarySuggestion => &format!("Provide an alias name after `as`, e.g. `{clause_name} @path/symbol as local_name`"),
                }
            );
        };
        let path_uses_grouped_syntax = items.iter().any(|item| item.from_grouped);

        if path_uses_grouped_syntax {
            return_syntax_error!(
                &format!("Grouped {clause_name}s cannot use a group-level alias. Add `as ...` to each grouped entry that needs renaming."),
                alias_token.location.clone(), {
                    CompilationStage => "Header Parsing",
                    PrimarySuggestion => &format!("Write `{clause_name} @path {{ item as local_name }}`, or use `{clause_name} @path/item as local_name` for a single {clause_name}"),
                }
            );
        }
        trailing_alias = Some(alias_name);
        trailing_alias_location = Some(alias_token.location.clone());
        index += 1;

        // Reject a second trailing `as` in single-import clauses.
        if tokens
            .get(index)
            .is_some_and(|token| matches!(token.kind, TokenKind::As))
        {
            return_syntax_error!(
                &format!("{} clauses can only have one alias.", capitalize_first(clause_name)),
                tokens[index].location.clone(), {
                    CompilationStage => "Header Parsing",
                    PrimarySuggestion => "Remove the second `as ...` alias",
                }
            );
        }
    }

    // Reject double alias: per-entry alias + trailing alias.
    if trailing_alias.is_some() && items.iter().any(|item| item.alias.is_some()) {
        return_syntax_error!(
            "Cannot use both per-entry aliases and a group-level alias.",
            path_token.location.clone(), {
                CompilationStage => "Header Parsing",
                PrimarySuggestion => &format!("Use only per-entry aliases inside `{{}}`, or a single trailing alias for one {clause_name}"),
            }
        );
    }

    let parsed_items = items
        .iter()
        .map(|item| ParsedImportItem {
            path: item.path.clone(),
            alias: item.alias.or(trailing_alias),
            path_location: item.path_location.clone(),
            alias_location: item
                .alias_location
                .clone()
                .or(trailing_alias_location.clone()),
        })
        .collect();

    Ok((parsed_items, index))
}

pub fn parse_import_clause_tokens(
    tokens: &[Token],
    start_index: usize,
) -> Result<(Vec<InternedPath>, usize), CompilerError> {
    // WHAT: path-only import clause parsing for callers that do not need alias data.
    // WHY: avoids threading StringTable through legacy call sites like path tests.
    // Note: this intentionally loses alias information. Callers that need aliases
    // should use parse_import_clause_items directly.
    let mut index = start_index;
    while tokens
        .get(index)
        .is_some_and(|token| matches!(token.kind, TokenKind::Newline))
    {
        index += 1;
    }
    let Some(import_token) = tokens.get(index) else {
        return Err(CompilerError::compiler_error(
            "Import clause parsing started past the end of the token stream.",
        ));
    };
    if !matches!(import_token.kind, TokenKind::Import) {
        return Err(CompilerError::compiler_error(
            "Import clause parsing expected to start on an 'import' token.",
        ));
    }
    let mut string_table = StringTable::new();
    let (items, next_index) = parse_import_clause_items(tokens, index, &mut string_table)?;
    let paths = items.into_iter().map(|item| item.path).collect();
    Ok((paths, next_index))
}

pub fn parse_re_export_clause_tokens(
    tokens: &[Token],
    start_index: usize,
) -> Result<(Vec<InternedPath>, usize), CompilerError> {
    let mut index = start_index;
    while tokens
        .get(index)
        .is_some_and(|token| matches!(token.kind, TokenKind::Newline))
    {
        index += 1;
    }
    let Some(import_token) = tokens.get(index) else {
        return Err(CompilerError::compiler_error(
            "#import clause parsing started past the end of the token stream.",
        ));
    };
    if !matches!(import_token.kind, TokenKind::Import) {
        return Err(CompilerError::compiler_error(
            "#import clause parsing expected to start on an '#import' token.",
        ));
    }
    let mut string_table = StringTable::new();
    let (items, next_index) = parse_re_export_clause_items(tokens, index, &mut string_table)?;
    let paths = items.into_iter().map(|item| item.path).collect();
    Ok((paths, next_index))
}

pub fn collect_paths_from_tokens(tokens: &[Token]) -> Result<Vec<InternedPath>, CompilerError> {
    let mut parsed_paths = Vec::new();
    let mut index = 0usize;

    while index < tokens.len() {
        if matches!(tokens[index].kind, TokenKind::Import) {
            let (paths, next_index) = if previous_significant_token_is_hash(tokens, index) {
                parse_re_export_clause_tokens(tokens, index)?
            } else {
                parse_import_clause_tokens(tokens, index)?
            };
            parsed_paths.extend(paths);
            index = next_index;
            continue;
        }

        index += 1;
    }

    Ok(parsed_paths)
}

fn previous_significant_token_is_hash(tokens: &[Token], index: usize) -> bool {
    tokens[..index]
        .iter()
        .rev()
        .find(|token| !matches!(token.kind, TokenKind::Newline))
        .is_some_and(|token| matches!(token.kind, TokenKind::Hash))
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
                    stream.new_location(), {
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
            return_syntax_error!(
                "Path components with whitespace must be quoted.",
                stream.new_location(), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Quote this path component, for example: \"my file.md\".",
                }
            )
        }

        return_syntax_error!(
            "Path components must be separated by '/'.",
            stream.new_location(), {
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
                stream.new_location(), {
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
                        stream.new_location(), {
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
                    stream.new_location(), {
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
                stream.new_location(), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Remove the extra comma",
                }
            )
        }

        let parsed_entry = parse_grouped_entry(stream, string_table)?;

        consume_all_whitespace(stream);

        if stream.peek() == Some(&'{') {
            if parsed_entry.alias.is_some() {
                return_syntax_error!(
                    "Path aliases are only valid on leaf entries.",
                    stream.new_location(), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Move the alias to the final leaf entry, e.g. 'base { entry as alias }'",
                    }
                )
            }

            if parsed_entry.ended_with_separator {
                return_syntax_error!(
                    "Slash-before-group syntax is not supported. Use 'base { ... }'.",
                    stream.new_location(), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Remove the separator before '{' and write the group as 'base { ... }'",
                    }
                )
            }

            if parsed_entry.components.is_empty() {
                return_syntax_error!(
                    "Nested grouped paths require a non-empty prefix before '{'.",
                    stream.new_location(), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Add a path prefix before the nested grouped block",
                    }
                )
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
                return_syntax_error!(
                    "A grouped path prefix cannot end with a separator.",
                    stream.new_location(), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Remove the trailing '/' or '\\' from this grouped path prefix",
                    }
                )
            }

            if parsed_entry.components.is_empty() {
                return_syntax_error!(
                    "Grouped path entry cannot be empty.",
                    stream.new_location(), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Provide a valid grouped path entry",
                    }
                )
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
) -> Result<(), CompilerError> {
    if !is_valid_identifier(alias) {
        return_syntax_error!(
            "Import alias must be a valid local binding name.",
            location, {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Use a normal identifier such as `render_component`",
            }
        );
    }

    if is_keyword(alias) {
        return_syntax_error!(
            format!("Import alias cannot be a reserved keyword: `{}`.", alias),
            location, {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Choose a different name that is not a reserved keyword",
            }
        );
    }

    Ok(())
}

/// WHAT: Parses one grouped entry, including optional `as alias`.
/// WHY: Grouped import aliases require each entry to carry its own alias metadata.
fn parse_grouped_entry(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<ParsedGroupedEntry, CompilerError> {
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

            if is_grouped_entry_stop_char(stream, next) {
                break;
            }

            if matches!(next, '/' | '\\') {
                return_syntax_error!(
                    "Empty path component is not allowed here.",
                    stream.new_location(), {
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
            break;
        };

        if is_grouped_entry_stop_char(stream, next) {
            break;
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
                stream.new_location(), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Quote this path component, for example: \"my file.md\".",
                }
            )
        }

        return_syntax_error!(
            "Path components must be separated by '/'.",
            stream.new_location(), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Insert '/' between path components",
            }
        )
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
            && is_grouped_entry_stop_char(stream, next)
        {
            return_syntax_error!(
                "Expected alias name after `as` in grouped import entry.",
                stream.new_location(), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Provide an alias name after `as`, e.g. `import @path { item as local_name }`",
                }
            );
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
            return_syntax_error!(
                "Grouped import entries can only have one alias.",
                stream.new_location(), {
                    CompilationStage => "Tokenization",
                    PrimarySuggestion => "Remove the second `as ...` alias",
                }
            );
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
    _string_table: &StringTable,
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
                stream.new_location(), {
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
                    stream.new_location(), {
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
                        stream.new_location(), {
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
    _string_table: &StringTable,
) -> Result<ParsedComponent, CompilerError> {
    let mut value = String::new();

    while let Some(next) = stream.peek().copied() {
        if is_component_terminator(stream, context, next) || next.is_whitespace() {
            break;
        }

        value.push(next);
        stream.next();
    }

    if value.is_empty() {
        return_syntax_error!(
            "Path component cannot be empty.",
            stream.new_location(), {
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

        if let Some(next) = stream.peek().copied()
            && !is_component_terminator(stream, context, next)
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
                return_syntax_error!(
                    "Path components with whitespace must be quoted.",
                    stream.new_location(), {
                        CompilationStage => "Tokenization",
                        PrimarySuggestion => "Quote this path component, for example: \"my file.md\".",
                    }
                );
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
    _string_table: &StringTable,
) -> Result<(), CompilerError> {
    if component.is_empty() {
        return_syntax_error!(
            "Path component cannot be empty.",
            stream.new_location(), {
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
            stream.new_location(), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Use '.' and '..' only as leading relative path markers",
            }
        )
    }

    if component.ends_with('.') {
        return_syntax_error!(
            format!("Invalid path component '{}'.", component),
            stream.new_location(), {
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
            stream.new_location(), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Use path components without syntax delimiters or cross-platform reserved filename characters",
            }
        )
    }

    if is_reserved_windows_name(component) {
        return_syntax_error!(
            format!("Invalid path component '{}'.", component),
            stream.new_location(), {
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

/// WHAT: Checks whether the current character ends a grouped entry.
/// WHY: Entries stop at commas, braces, or the `as` keyword. The `as` check needs
///      the stream to peek ahead and verify it's the keyword, not a component like `assignment`.
fn is_grouped_entry_stop_char(stream: &TokenStream, character: char) -> bool {
    if matches!(character, ',' | '}' | '{') {
        return true;
    }
    // Stop at `as` keyword in grouped entries.
    if character == 'a' && peek_keyword_as(stream) {
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
) -> bool {
    if matches!(character, '/' | '\\') {
        return true;
    }

    match context {
        ParseComponentContext::OrdinaryPath => {
            ordinary_stop_reason(stream.mode, character).is_some()
        }
        ParseComponentContext::GroupedEntry => is_grouped_entry_stop_char(stream, character),
    }
}

#[cfg(test)]
#[path = "tests/paths_tests.rs"]
mod paths_tests;
