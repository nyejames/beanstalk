//! Grouped block and entry parsing for path syntax.
//!
//! WHAT: parses `{ ... }` grouped blocks and individual entries with optional aliases.
//! WHY: grouped path syntax is recursive sugar that expands into explicit suffix lists;
//!      keeping it separate from ordinary prefix parsing makes each module's responsibility
//!      clear.

use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, ImportClauseKind, InvalidImportClauseReason, PathKind,
};
use crate::compiler_frontend::keywords::{is_keyword, is_valid_identifier};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::lexer::consume_all_whitespace;
use crate::compiler_frontend::tokenizer::tokens::{SourceLocation, TokenStream};

use super::{GroupedPathExpansion, ParseComponentContext, RelativePathExpansions};

pub(super) fn parse_grouped_block(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<RelativePathExpansions, Box<CompilerDiagnostic>> {
    let mut expanded_suffixes: RelativePathExpansions = Vec::new();
    let mut saw_entry = false;
    let mut expect_entry = true;

    loop {
        consume_all_whitespace(stream);

        let Some(next) = stream.peek().copied() else {
            return Err(Box::new(CompilerDiagnostic::invalid_path(
                PathKind::MissingClosingBrace,
                stream.new_location(),
            )));
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
                    return Err(Box::new(CompilerDiagnostic::invalid_path(
                        PathKind::EntriesNeedCommas,
                        stream.new_location(),
                    )));
                }
            }
        }

        if next == '}' {
            if !saw_entry {
                return Err(Box::new(CompilerDiagnostic::invalid_path(
                    PathKind::EmptyGroupedBlock,
                    stream.new_location(),
                )));
            }

            stream.next();
            break;
        }

        if next == ',' {
            return Err(Box::new(CompilerDiagnostic::invalid_path(
                PathKind::MultipleCommas,
                stream.new_location(),
            )));
        }

        let parsed_entry = parse_grouped_entry(stream, string_table)?;

        consume_all_whitespace(stream);

        if stream.peek() == Some(&'{') {
            if parsed_entry.alias.is_some() {
                return Err(Box::new(CompilerDiagnostic::invalid_path(
                    PathKind::AliasOnlyOnLeaf,
                    stream.new_location(),
                )));
            }

            if parsed_entry.ended_with_separator {
                return Err(Box::new(CompilerDiagnostic::invalid_path(
                    PathKind::SlashBeforeGroup,
                    stream.new_location(),
                )));
            }

            if parsed_entry.components.is_empty() {
                return Err(Box::new(CompilerDiagnostic::invalid_path(
                    PathKind::NestedGroupNeedsPrefix,
                    stream.new_location(),
                )));
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
                return Err(Box::new(CompilerDiagnostic::invalid_path(
                    PathKind::GroupedPrefixTrailingSeparator,
                    stream.new_location(),
                )));
            }

            if parsed_entry.components.is_empty() {
                return Err(Box::new(CompilerDiagnostic::invalid_path(
                    PathKind::GroupedEntryEmpty,
                    stream.new_location(),
                )));
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
) -> Result<(), Box<CompilerDiagnostic>> {
    if !is_valid_identifier(alias) {
        return Err(Box::new(CompilerDiagnostic::invalid_import_clause(
            ImportClauseKind::Alias,
            InvalidImportClauseReason::AliasNotValidIdentifier,
            location,
        )));
    }

    if is_keyword(alias) {
        return Err(Box::new(CompilerDiagnostic::invalid_import_clause(
            ImportClauseKind::Alias,
            InvalidImportClauseReason::AliasIsKeyword,
            location,
        )));
    }

    Ok(())
}

/// WHAT: Parses one grouped entry, including optional `as alias`.
/// WHY: Grouped import aliases require each entry to carry its own alias metadata.
fn parse_grouped_entry(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<super::ParsedGroupedEntry, Box<CompilerDiagnostic>> {
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

            if super::is_grouped_entry_stop_char(stream, next, true) {
                break;
            }

            if matches!(next, '/' | '\\') {
                return Err(Box::new(CompilerDiagnostic::invalid_path(
                    PathKind::EmptyComponent,
                    stream.new_location(),
                )));
            }

            let parsed_component = super::components::parse_component(
                stream,
                ParseComponentContext::GroupedEntry,
                string_table,
            )?;
            super::components::push_validated_component(
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

        if super::is_grouped_entry_stop_char(stream, next, true) {
            break;
        }

        if matches!(next, '/' | '\\') {
            stream.next();
            expect_component = true;
            ended_with_separator = true;
            continue;
        }

        if skipped_whitespace {
            return Err(Box::new(CompilerDiagnostic::invalid_path(
                PathKind::WhitespaceMustBeQuoted,
                stream.new_location(),
            )));
        }

        return Err(Box::new(CompilerDiagnostic::invalid_path(
            PathKind::MissingSeparator,
            stream.new_location(),
        )));
    }

    let path_end = stream.position;

    // Check for optional `as alias` after the path components.
    let mut alias = None;
    let mut alias_location = None;

    consume_all_whitespace(stream);
    if stream.peek().copied() == Some('a') && super::peek_keyword_as(stream) {
        super::consume_keyword_as(stream);
        consume_all_whitespace(stream);

        // Give a targeted diagnostic when the alias name is missing entirely.
        if let Some(next) = stream.peek().copied()
            && super::is_grouped_entry_stop_char(stream, next, true)
        {
            return Err(Box::new(CompilerDiagnostic::invalid_import_clause(
                ImportClauseKind::Alias,
                InvalidImportClauseReason::MissingAlias,
                stream.new_location(),
            )));
        }

        let alias_start = stream.position;
        let alias_component = super::components::parse_bare_component(
            stream,
            ParseComponentContext::GroupedEntry,
            string_table,
        )?;
        let alias_end = stream.position;
        let location = SourceLocation::new(stream.file_path.to_owned(), alias_start, alias_end);

        validate_import_alias_symbol(&alias_component.value, location.clone())?;

        alias = Some(string_table.intern(&alias_component.value));
        alias_location = Some(location);

        // Reject a second `as` keyword inside a grouped entry.
        consume_all_whitespace(stream);
        if stream.peek().copied() == Some('a') && super::peek_keyword_as(stream) {
            return Err(Box::new(CompilerDiagnostic::invalid_import_clause(
                ImportClauseKind::Grouped,
                InvalidImportClauseReason::DoubleAliasInGroupedEntry,
                stream.new_location(),
            )));
        }
    }

    Ok(super::ParsedGroupedEntry {
        components,
        ended_with_separator,
        alias,
        path_location: SourceLocation::new(stream.file_path.to_owned(), entry_start, path_end),
        alias_location,
    })
}
