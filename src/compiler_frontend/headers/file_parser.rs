#![allow(clippy::result_large_err)]

//! Per-file header splitting.
//!
//! WHAT: orchestrates one tokenized Beanstalk file into top-level declaration headers, import
//! records, const-fragment metadata, and the implicit entry `start` body.
//! WHY: file-level control flow is different from declaration parsing, import recording, and hash
//! item handling; this module keeps the high-level loop visible while delegated modules own details.

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::headers::file_imports::{
    parse_and_record_export_path_clause, parse_and_record_imports, parse_and_record_public_imports,
};
use crate::compiler_frontend::headers::file_state::HeaderFileParseState;
use crate::compiler_frontend::headers::hash_items::handle_hash_item;
use crate::compiler_frontend::headers::header_dispatch::create_header;
use crate::compiler_frontend::headers::start_capture::push_runtime_template_tokens_to_start_function;
use crate::compiler_frontend::headers::top_level_classifier::{
    HeaderFileItem, classify_current_item, starts_duplicate_top_level_header_declaration,
    starts_specialized_generic_conformance_declaration, starts_trait_declaration_after_must,
};
use crate::compiler_frontend::headers::types::{
    FileFrontendPrepareError, FileFrontendPrepareOutput, FileRole, HeaderBuildContext,
    HeaderExportMode, HeaderKind, HeaderParseContext,
};
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword, reserved_trait_keyword_error,
};
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};

// Top-level declarations are module-visible; non-declaration statements are collected into the
// implicit start-function header for that file.
pub(super) fn parse_headers_in_file(
    token_stream: &mut FileTokens,
    context: &mut HeaderParseContext<'_>,
) -> Result<FileFrontendPrepareOutput, FileFrontendPrepareError> {
    let mut state = HeaderFileParseState::new(token_stream.length);

    let result = parse_headers_in_file_inner(token_stream, context, &mut state);

    match result {
        Ok(()) => finish_file_output(token_stream, context, state),
        Err(diagnostic) => Err(state.into_error(diagnostic)),
    }
}

fn parse_headers_in_file_inner(
    token_stream: &mut FileTokens,
    context: &mut HeaderParseContext<'_>,
    state: &mut HeaderFileParseState,
) -> Result<(), CompilerDiagnostic> {
    loop {
        let current_token = token_stream.current_token();
        let current_location = token_stream.current_location();
        token_stream.advance();

        match classify_current_item(token_stream, &current_token) {
            HeaderFileItem::Symbol(name_id) => {
                handle_symbol_item(
                    token_stream,
                    state,
                    context,
                    current_token,
                    name_id,
                    current_location,
                )?;
            }

            HeaderFileItem::BuiltinTypeConformanceTarget(type_name) => {
                let name_id = context.string_table.intern(type_name);
                handle_symbol_item(
                    token_stream,
                    state,
                    context,
                    current_token,
                    name_id,
                    current_location,
                )?;
            }

            HeaderFileItem::Import => {
                parse_and_record_imports(token_stream, state, context, current_location)?;
            }

            HeaderFileItem::Export => {
                handle_export_item(
                    token_stream,
                    state,
                    context,
                    current_token,
                    current_location,
                )?;
            }

            HeaderFileItem::Hash {
                at_statement_boundary,
            } => {
                handle_hash_item(
                    token_stream,
                    state,
                    context,
                    current_token,
                    current_location,
                    at_statement_boundary,
                )?;
            }

            HeaderFileItem::ReservedTraitSyntax => {
                handle_reserved_trait_syntax(&current_token, current_location)?;
            }

            HeaderFileItem::RuntimeTemplate => {
                handle_runtime_template_item(
                    token_stream,
                    state,
                    context,
                    current_token,
                    current_location,
                )?;
            }

            HeaderFileItem::Eof => {
                state.push_start_body_token(current_token);
                break;
            }

            HeaderFileItem::StartBodyToken => {
                state.push_start_body_token(current_token);
            }
        }
    }

    Ok(())
}

fn handle_export_item(
    token_stream: &mut FileTokens,
    state: &mut HeaderFileParseState,
    context: &mut HeaderParseContext<'_>,
    _export_token: Token,
    export_location: SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    // `export` is a facade-only keyword; ordinary files cannot use it.
    if context.file_role != FileRole::ModuleFacade {
        return Err(CompilerDiagnostic::export_outside_module_facade(
            export_location,
        ));
    }

    // `export` must have a target on the same logical line.
    if matches!(
        token_stream.current_token_kind(),
        TokenKind::Newline | TokenKind::End | TokenKind::Eof
    ) {
        return Err(CompilerDiagnostic::missing_export_target(export_location));
    }

    match token_stream.current_token_kind() {
        // `export import @path { ... }` — public re-export of imported symbols.
        TokenKind::Import => {
            parse_and_record_public_imports(
                token_stream,
                state,
                context,
                export_location,
                token_stream.index,
            )?;
        }

        // `export @path { ... }` — syntactic sugar for public grouped imports.
        TokenKind::Path(_) => {
            let has_grouped = if let TokenKind::Path(items) = &token_stream.current_token().kind {
                items.iter().any(|item| item.from_grouped)
            } else {
                false
            };

            if !has_grouped {
                return Err(CompilerDiagnostic::deferred_namespace_export(
                    export_location,
                ));
            }

            parse_and_record_export_path_clause(
                token_stream,
                state,
                context,
                export_location,
                token_stream.index.saturating_sub(1),
            )?;
        }

        // Exported authored declaration: `export name = ...`, `export name #= ...`, etc.
        TokenKind::Symbol(name_id) => {
            let name_id = *name_id;
            let symbol_token = token_stream.current_token();
            let symbol_location = token_stream.current_location();
            token_stream.advance();

            if starts_exported_trait_conformance(token_stream) {
                return Err(CompilerDiagnostic::invalid_export_target(export_location));
            }

            if !starts_duplicate_top_level_header_declaration(token_stream) {
                return Err(CompilerDiagnostic::invalid_export_target(export_location));
            }

            handle_symbol_item_with_export_mode(
                token_stream,
                state,
                context,
                symbol_token,
                name_id,
                symbol_location,
                HeaderExportMode::Public,
            )?;
        }

        // `export` before a runtime template is invalid in a facade.
        TokenKind::TemplateHead => {
            return Err(CompilerDiagnostic::runtime_template_in_module_facade(
                export_location,
            ));
        }

        // `export` before reserved trait syntax.
        TokenKind::Must | TokenKind::TraitThis => {
            if let Some(keyword) = reserved_trait_keyword(token_stream.current_token_kind()) {
                return Err(reserved_trait_keyword_error(keyword, export_location));
            }
            return Err(CompilerDiagnostic::invalid_export_target(export_location));
        }

        // `export` before any other token is unsupported.
        _ => {
            return Err(CompilerDiagnostic::invalid_export_target(export_location));
        }
    }

    Ok(())
}

fn starts_exported_trait_conformance(token_stream: &FileTokens) -> bool {
    (token_stream.current_token_kind() == &TokenKind::Must
        && !starts_trait_declaration_after_must(token_stream))
        || starts_specialized_generic_conformance_declaration(token_stream)
}

fn handle_symbol_item(
    token_stream: &mut FileTokens,
    state: &mut HeaderFileParseState,
    context: &mut HeaderParseContext<'_>,
    current_token: Token,
    name_id: StringId,
    current_location: SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    handle_symbol_item_with_export_mode(
        token_stream,
        state,
        context,
        current_token,
        name_id,
        current_location,
        HeaderExportMode::Private,
    )
}

fn handle_symbol_item_with_export_mode(
    token_stream: &mut FileTokens,
    state: &mut HeaderFileParseState,
    context: &mut HeaderParseContext<'_>,
    current_token: Token,
    name_id: StringId,
    current_location: SourceLocation,
    export_mode: HeaderExportMode,
) -> Result<(), CompilerDiagnostic> {
    // Only prelude-visible external symbols block local declarations; package-scoped symbols that
    // are not imported should not prevent a file from declaring its own symbol with the same name.
    if context
        .external_package_registry
        .is_prelude_function(context.string_table.resolve(name_id))
    {
        return handle_prelude_symbol_item(token_stream, state, current_token, name_id);
    }

    if let Some(first_location) = state.encountered_symbols.get(&name_id) {
        let is_conformance_declaration = (token_stream.current_token_kind() == &TokenKind::Must
            && !starts_trait_declaration_after_must(token_stream))
            || starts_specialized_generic_conformance_declaration(token_stream);

        // Conformance declarations reuse the target type name (`Type must TRAIT`).
        // They do not conflict with the type declaration itself.
        // AST evidence validation catches duplicate semantic conformance facts later.
        if !is_conformance_declaration
            && starts_duplicate_top_level_header_declaration(token_stream)
        {
            return Err(CompilerDiagnostic::duplicate_declaration(
                name_id,
                first_location.clone(),
                token_stream.current_location(),
            ));
        }

        if !is_conformance_declaration {
            state.push_start_body_token(current_token);
            // Body-level symbol/import resolution belongs to AST passes. Header parsing only validates
            // duplicate top-level declaration starts at this stage.
            return Ok(());
        }

        // Fall through for conformance declarations so they are parsed as real headers.
    }

    if state.start_body_symbols.contains(&name_id)
        && !starts_duplicate_top_level_header_declaration(token_stream)
    {
        state.push_start_body_token(current_token);
        return Ok(());
    }

    let source_file = token_stream.src_path.to_owned();
    let mut build_context = HeaderBuildContext {
        external_package_registry: context.external_package_registry,
        warnings: &mut state.warnings,
        source_file: &source_file,
        file_imports: &state.file_import_paths,
        file_import_entries: &state.file_imports,
        file_constant_order: &mut state.file_constant_order,
        string_table: context.string_table,
        file_role: context.file_role,
    };
    let header = create_header(
        token_stream.src_path.append(name_id),
        token_stream,
        current_location,
        export_mode,
        &mut build_context,
    )?;

    match header.kind {
        HeaderKind::StartFunction => {
            state.push_start_body_token(current_token);
            state.register_start_body_symbol(name_id);
        }
        HeaderKind::TraitConformance { .. } => {
            // Conformance declarations reuse the target type name and must not shadow
            // the type's entry in encountered_symbols for duplicate detection.
            state.register_header(header);
        }

        _ => {
            let name_location = header.name_location.clone();
            state.register_header(header);
            state.encountered_symbols.insert(name_id, name_location);
        }
    }

    Ok(())
}

fn handle_prelude_symbol_item(
    token_stream: &FileTokens,
    state: &mut HeaderFileParseState,
    current_token: Token,
    name_id: StringId,
) -> Result<(), CompilerDiagnostic> {
    if starts_duplicate_top_level_header_declaration(token_stream) {
        return Err(CompilerDiagnostic::reserved_builtin_name(
            name_id,
            token_stream.current_location(),
        ));
    }

    state.push_start_body_token(current_token);

    Ok(())
}

fn handle_reserved_trait_syntax(
    current_token: &Token,
    current_location: SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    if let Some(keyword) = reserved_trait_keyword(&current_token.kind) {
        return Err(reserved_trait_keyword_error(keyword, current_location));
    }

    Ok(())
}

fn handle_runtime_template_item(
    token_stream: &mut FileTokens,
    state: &mut HeaderFileParseState,
    context: &mut HeaderParseContext<'_>,
    current_token: Token,
    current_location: SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    // Runtime top-level templates stay in the start-function body and are evaluated in source
    // order by entry start(). The runtime fragment count lets later const fragments record their
    // insertion point relative to already-seen runtime fragments.
    if context.file_role == FileRole::ModuleFacade {
        return Err(CompilerDiagnostic::runtime_template_in_module_facade(
            current_location,
        ));
    }

    push_runtime_template_tokens_to_start_function(
        current_token,
        token_stream,
        &mut state.start_function_body,
        context.string_table,
    )?;

    if context.file_role == FileRole::Entry {
        state.runtime_fragment_count += 1;
    }

    Ok(())
}

fn finish_file_output(
    token_stream: &FileTokens,
    context: &HeaderParseContext<'_>,
    state: HeaderFileParseState,
) -> Result<FileFrontendPrepareOutput, FileFrontendPrepareError> {
    // Non-entry files have no semantic consumer for an implicit start. Any non-trivial top-level
    // executable token is therefore rejected before output assembly.
    if context.file_role != FileRole::Entry && state.has_non_trivial_start_body() {
        let location = state
            .first_executable_start_body_location()
            .unwrap_or_default();
        return Err(
            state.into_error(CompilerDiagnostic::invalid_top_level_runtime_statement(
                location,
            )),
        );
    }

    if context.file_role == FileRole::Entry {
        Ok(state.into_entry_output(token_stream, context.file_role))
    } else {
        Ok(state.into_non_entry_output(token_stream, context.file_role))
    }
}
