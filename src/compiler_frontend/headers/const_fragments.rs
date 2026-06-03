#![allow(clippy::result_large_err)]

//! Top-level const-template header creation.
//!
//! WHAT: turns entry-file `#[...]` templates into const-template headers plus placement metadata.
//! WHY: const fragments are folded by AST but ordered by header parsing through runtime insertion
//! indices, so this logic must stay in the header stage.

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::declaration_syntax::declaration_shell::{
    InitializerReference, collect_initializer_references,
};
use crate::compiler_frontend::headers::types::{
    Header, HeaderBuildContext, HeaderExportMode, HeaderKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::token_scan::NestingDepth;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::projects::settings::TOP_LEVEL_CONST_TEMPLATE_NAME;
use std::collections::HashSet;

pub(super) fn create_top_level_const_template(
    scope: InternedPath,
    opening_template_token: Token,
    const_template_number: usize,
    token_stream: &mut FileTokens,
    context: &mut HeaderBuildContext<'_>,
) -> Result<Header, CompilerDiagnostic> {
    let const_template_name = context.string_table.intern(&format!(
        "{TOP_LEVEL_CONST_TEMPLATE_NAME}{const_template_number}"
    ));
    let mut dependencies: HashSet<InternedPath> = HashSet::new();

    // Keep the full template token stream (including open/close) so AST template parsing
    // can treat const templates exactly like regular templates.
    let mut body = Vec::with_capacity(10);
    body.push(opening_template_token);

    let start_location = token_stream.current_location();

    let closing_bracket = context.string_table.intern("]");
    crate::compiler_frontend::token_scan::consume_balanced_template_region(
        token_stream,
        |token, token_kind| {
            if let TokenKind::Symbol(name_id) = token_kind
                && let Some(path) = context
                    .file_imports
                    .iter()
                    .find(|f| f.name() == Some(*name_id))
            {
                dependencies.insert(path.to_owned());
            }
            body.push(token);
        },
        |location| CompilerDiagnostic::unexpected_end_of_file(Some(closing_bracket), location),
    )?;

    // Add an EOF sentinel so downstream parsers can safely terminate even if
    // expression parsing consumed to the end of this synthetic token stream.
    body.push(Token {
        kind: TokenKind::Eof,
        location: token_stream.current_location(),
    });
    let condition_references = collect_template_if_condition_references(&body);

    let full_name = scope.append(const_template_name);
    let name_location = SourceLocation {
        scope,
        start_pos: start_location.start_pos,
        end_pos: token_stream.current_location().end_pos,
    };

    let mut template_tokens = FileTokens::new_with_file_id(full_name, token_stream.file_id, body);
    template_tokens.canonical_os_path = token_stream.canonical_os_path.clone();

    Ok(Header {
        kind: HeaderKind::ConstTemplate {
            condition_references,
            source_order: next_const_fragment_source_order(context),
        },
        file_role: context.file_role,
        export_mode: HeaderExportMode::Private,
        dependencies,
        name_location,
        tokens: template_tokens,
        source_file: context.source_file.to_owned(),
    })
}

fn next_const_fragment_source_order(context: &mut HeaderBuildContext<'_>) -> usize {
    let source_order = *context.file_constant_order;
    *context.file_constant_order += 1;
    source_order
}

fn collect_template_if_condition_references(tokens: &[Token]) -> Vec<InitializerReference> {
    let mut references = Vec::new();
    let mut index = 0;

    while index < tokens.len() {
        if matches!(tokens[index].kind, TokenKind::If) {
            let condition_start = index + 1;
            let condition_end = find_template_if_condition_end(tokens, condition_start);
            references.extend(collect_initializer_references(
                &tokens[condition_start..condition_end],
            ));
            index = condition_end;
            continue;
        }

        index += 1;
    }

    references
}

fn find_template_if_condition_end(tokens: &[Token], start: usize) -> usize {
    let mut nesting_depth = NestingDepth::default();
    let mut index = start;

    while index < tokens.len() {
        let token = &tokens[index];

        if nesting_depth.is_top_level() {
            match token.kind {
                // Template suffix bodies use StartTemplateBody. Colon is included for
                // defensive parity with other header scanners and tests.
                TokenKind::StartTemplateBody
                | TokenKind::Colon
                | TokenKind::TemplateClose
                | TokenKind::Eof => return index,

                // Option-present template `if` conditions only depend on the scrutinee
                // before `is`; capture names are branch-local, not header deps.
                TokenKind::Is => return index,

                _ => {}
            }
        }

        nesting_depth.step(&token.kind);
        index += 1;
    }

    index
}
