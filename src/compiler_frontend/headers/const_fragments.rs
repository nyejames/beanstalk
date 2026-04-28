//! Top-level const-template header creation.
//!
//! WHAT: turns entry-file `#[...]` templates into const-template headers plus placement metadata.
//! WHY: const fragments are folded by AST but ordered by header parsing through runtime insertion
//! indices, so this logic must stay in the header stage.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::headers::types::{Header, HeaderBuildContext, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::projects::settings::TOP_LEVEL_CONST_TEMPLATE_NAME;
use std::collections::HashSet;

pub(super) fn create_top_level_const_template(
    scope: InternedPath,
    opening_template_token: Token,
    const_template_number: usize,
    token_stream: &mut FileTokens,
    context: &mut HeaderBuildContext<'_>,
) -> Result<Header, CompilerError> {
    let const_template_name = context.string_table.intern(&format!(
        "{TOP_LEVEL_CONST_TEMPLATE_NAME}{const_template_number}"
    ));
    let mut dependencies: HashSet<InternedPath> = HashSet::new();

    // Keep the full template token stream (including open/close) so AST template parsing
    // can treat const templates exactly like regular templates.
    let mut body = Vec::with_capacity(10);
    body.push(opening_template_token);

    let start_location = token_stream.current_location();

    crate::compiler_frontend::token_scan::consume_balanced_template_region(
        token_stream,
        |token, token_kind| {
            if let TokenKind::Symbol(name_id) = token_kind
                && let Some(path) = context.file_imports.iter().find(|f| f.name() == Some(*name_id))
            {
                dependencies.insert(path.to_owned());
            }
            body.push(token);
        },
        |location| {
            CompilerError::new_rule_error(
                "Unexpected end of file while parsing top-level const template. Missing ']' to close the template.",
                location,
            )
        },
    )
    .map_err(|mut error| {
        error.new_metadata_entry(
            crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
            String::from("Close the template with ']'"),
        );
        error.new_metadata_entry(
            crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::SuggestedInsertion,
            String::from("]"),
        );
        error
    })?;

    // Add an EOF sentinel so downstream parsers can safely terminate even if
    // expression parsing consumed to the end of this synthetic token stream.
    body.push(Token {
        kind: TokenKind::Eof,
        location: token_stream.current_location(),
    });

    let full_name = scope.append(const_template_name);
    let name_location = SourceLocation {
        scope,
        start_pos: start_location.start_pos,
        end_pos: token_stream.current_location().end_pos,
    };

    let mut template_tokens = FileTokens::new_with_file_id(full_name, token_stream.file_id, body);
    template_tokens.canonical_os_path = token_stream.canonical_os_path.clone();

    let re_exports = context.file_re_export_entries.to_vec();
    Ok(Header {
        kind: HeaderKind::ConstTemplate,
        exported: true,
        dependencies,
        name_location,
        tokens: template_tokens,
        source_file: context.source_file.to_owned(),
        file_imports: context.file_import_entries.to_vec(),
        file_re_exports: re_exports,
    })
}
