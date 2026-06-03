//! Import clause parsing over path tokens.
//!
//! WHAT: validates alias placement and expands path-token items into import records.
//! WHY: Stage 0 import scanning and header parsing need the same alias-aware clause
//! parser, but this logic is separate from raw path-token lexing.

use super::*;

#[derive(Clone, Debug)]
pub struct ParsedImportItem {
    pub path: InternedPath,
    pub alias: Option<StringId>,
    pub path_location: SourceLocation,
    pub alias_location: Option<SourceLocation>,
    pub from_grouped: bool,
}

pub fn parse_import_clause_items(
    tokens: &[Token],
    start_index: usize,
    string_table: &StringTable,
) -> Result<(Vec<ParsedImportItem>, usize), CompilerDiagnostic> {
    parse_path_clause_items(
        tokens,
        start_index,
        TokenKind::Import,
        "import",
        string_table,
    )
}

/// Parse an `export @path { ... }` sugar clause into import items.
///
/// WHAT: `export @path { Symbol }` is syntactic sugar for `export import @path { Symbol }`.
/// WHY: Stage 0 path collection and header parsing both need to resolve the path items inside
///      `export @path` without an explicit `import` token.
pub fn parse_export_path_clause_items(
    tokens: &[Token],
    start_index: usize,
    string_table: &StringTable,
) -> Result<(Vec<ParsedImportItem>, usize), CompilerDiagnostic> {
    parse_path_clause_items(
        tokens,
        start_index,
        TokenKind::Export,
        "export",
        string_table,
    )
}

fn parse_path_clause_items(
    tokens: &[Token],
    start_index: usize,
    expected_token_kind: TokenKind,
    _clause_name: &str,
    _string_table: &StringTable,
) -> Result<(Vec<ParsedImportItem>, usize), CompilerDiagnostic> {
    let Some(clause_token) = tokens.get(start_index) else {
        return Err(CompilerDiagnostic::invalid_import_clause(
            ImportClauseKind::Import,
            InvalidImportClauseReason::MissingPath,
            SourceLocation::default(),
        ));
    };

    if clause_token.kind != expected_token_kind {
        return Err(CompilerDiagnostic::invalid_import_clause(
            ImportClauseKind::Import,
            InvalidImportClauseReason::ExpectedPath,
            clause_token.location.clone(),
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
        return Err(CompilerDiagnostic::invalid_import_clause(
            ImportClauseKind::Import,
            InvalidImportClauseReason::MissingPath,
            clause_token.location.clone(),
        ));
    };

    let TokenKind::Path(items) = &path_token.kind else {
        return Err(CompilerDiagnostic::invalid_import_clause(
            ImportClauseKind::Import,
            InvalidImportClauseReason::ExpectedPath,
            path_token.location.clone(),
        ));
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
            return Err(CompilerDiagnostic::invalid_import_clause(
                ImportClauseKind::Alias,
                InvalidImportClauseReason::MissingAlias,
                path_token.location.clone(),
            ));
        };
        let TokenKind::Symbol(alias_name) = alias_token.kind else {
            return Err(CompilerDiagnostic::invalid_import_clause(
                ImportClauseKind::Alias,
                InvalidImportClauseReason::ExpectedAliasName,
                alias_token.location.clone(),
            ));
        };
        let path_uses_grouped_syntax = items.iter().any(|item| item.from_grouped);

        if path_uses_grouped_syntax {
            return Err(CompilerDiagnostic::invalid_import_clause(
                ImportClauseKind::Grouped,
                InvalidImportClauseReason::GroupedWithTrailingAlias,
                alias_token.location.clone(),
            ));
        }
        trailing_alias = Some(alias_name);
        trailing_alias_location = Some(alias_token.location.clone());
        index += 1;

        // Reject a second trailing `as` in single-import clauses.
        if tokens
            .get(index)
            .is_some_and(|token| matches!(token.kind, TokenKind::As))
        {
            return Err(CompilerDiagnostic::invalid_import_clause(
                ImportClauseKind::Alias,
                InvalidImportClauseReason::MultipleTrailingAliases,
                tokens[index].location.clone(),
            ));
        }
    }

    // Reject double alias: per-entry alias + trailing alias.
    if trailing_alias.is_some() && items.iter().any(|item| item.alias.is_some()) {
        return Err(CompilerDiagnostic::invalid_import_clause(
            ImportClauseKind::Grouped,
            InvalidImportClauseReason::PerEntryAndTrailingAlias,
            path_token.location.clone(),
        ));
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
            from_grouped: item.from_grouped,
        })
        .collect();

    Ok((parsed_items, index))
}

pub fn parse_import_clause_tokens(
    tokens: &[Token],
    start_index: usize,
) -> Result<(Vec<InternedPath>, usize), CompilerDiagnostic> {
    // WHAT: path-only import clause parsing for callers that do not need alias data.
    // WHY: module reachability only needs canonical target paths; header import preparation is
    // the owner for alias visibility and uses `parse_import_clause_items` directly.
    let mut index = start_index;
    while tokens
        .get(index)
        .is_some_and(|token| matches!(token.kind, TokenKind::Newline))
    {
        index += 1;
    }

    let Some(import_token) = tokens.get(index) else {
        return Err(CompilerDiagnostic::invalid_import_clause(
            ImportClauseKind::Import,
            InvalidImportClauseReason::MissingPath,
            SourceLocation::default(),
        ));
    };
    if !matches!(import_token.kind, TokenKind::Import) {
        return Err(CompilerDiagnostic::invalid_import_clause(
            ImportClauseKind::Import,
            InvalidImportClauseReason::ExpectedPath,
            import_token.location.clone(),
        ));
    }
    let string_table = StringTable::new();
    let (items, next_index) = parse_import_clause_items(tokens, index, &string_table)?;
    let paths = items.into_iter().map(|item| item.path).collect();
    Ok((paths, next_index))
}

pub fn collect_paths_from_tokens(
    tokens: &[Token],
) -> Result<Vec<InternedPath>, CompilerDiagnostic> {
    let mut parsed_paths = Vec::new();
    let mut index = 0usize;

    while index < tokens.len() {
        if matches!(tokens[index].kind, TokenKind::Import) {
            if previous_significant_token_is_hash(tokens, index) {
                // Stage 0 only gathers reachable files. Legacy `#import` is diagnosed by header
                // parsing so users get the explicit removed-syntax error instead of a path error.
                index += 1;
                continue;
            }

            let (paths, next_index) = parse_import_clause_tokens(tokens, index)?;
            parsed_paths.extend(paths);
            index = next_index;
            continue;
        }

        // Collect paths from `export import @path { ... }` and `export @path { ... }`.
        // Exported authored declarations do not carry import paths and are ignored here.
        if matches!(tokens[index].kind, TokenKind::Export) {
            if previous_significant_token_is_hash(tokens, index) {
                index += 1;
                continue;
            }

            // `export import @path { ... }`
            if tokens
                .get(index + 1)
                .is_some_and(|token| matches!(token.kind, TokenKind::Import))
            {
                let (paths, next_index) = parse_import_clause_tokens(tokens, index + 1)?;
                parsed_paths.extend(paths);
                index = next_index;
                continue;
            }

            // `export @path { ... }` sugar — only if the next token is a path.
            if tokens
                .get(index + 1)
                .is_some_and(path_token_uses_grouped_syntax)
            {
                let string_table = StringTable::new();
                let (items, next_index) =
                    parse_export_path_clause_items(tokens, index, &string_table)?;
                parsed_paths.extend(items.into_iter().map(|item| item.path));
                index = next_index;
                continue;
            }

            index += 1;
            continue;
        }

        index += 1;
    }

    Ok(parsed_paths)
}

fn path_token_uses_grouped_syntax(token: &Token) -> bool {
    match &token.kind {
        TokenKind::Path(items) => items.iter().any(|item| item.from_grouped),
        _ => false,
    }
}

fn previous_significant_token_is_hash(tokens: &[Token], index: usize) -> bool {
    tokens[..index]
        .iter()
        .rev()
        .find(|token| !matches!(token.kind, TokenKind::Newline))
        .is_some_and(|token| matches!(token.kind, TokenKind::Hash))
}
