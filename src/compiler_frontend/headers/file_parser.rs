#![allow(clippy::result_large_err)]

//! Per-file header splitting.
//!
//! WHAT: walks one tokenized Beanstalk file and separates top-level declarations from the implicit
//! entry `start` body.
//! WHY: file-level control flow is different from declaration-specific parsing; keeping it separate
//! prevents the header entry point from becoming a parser monolith.

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::headers::const_fragments::create_top_level_const_template;
use crate::compiler_frontend::headers::header_dispatch::create_header;
use crate::compiler_frontend::headers::imports::normalize_import_dependency_path;
use crate::compiler_frontend::headers::start_capture::push_runtime_template_tokens_to_start_function;
use crate::compiler_frontend::headers::types::{
    FileFrontendPrepareError, FileFrontendPrepareOutput, FileImport, FileRole, Header,
    HeaderBuildContext, HeaderKind, HeaderParseContext, TopLevelConstFragment,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::const_paths::parse_import_clause_items;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword, reserved_trait_keyword_error,
};
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::line_scanning::find_top_level_fat_arrow_on_line;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::projects::settings::{
    MINIMUM_LIKELY_DECLARATIONS, TOKEN_TO_DECLARATION_RATIO, TOKEN_TO_HEADER_RATIO,
};
use std::collections::{HashMap, HashSet};

// Top-level declarations are module-visible; non-declaration statements are collected into the
// implicit start-function header for that file.
pub(super) fn parse_headers_in_file(
    token_stream: &mut FileTokens,
    context: &mut HeaderParseContext<'_>,
) -> Result<FileFrontendPrepareOutput, FileFrontendPrepareError> {
    let mut file_warnings: Vec<CompilerDiagnostic> = Vec::new();

    let result = parse_headers_in_file_inner(token_stream, context, &mut file_warnings);

    match result {
        Ok(output) => Ok(output),
        Err(diagnostic) => Err(FileFrontendPrepareError {
            warnings: file_warnings,
            diagnostic: Box::new(diagnostic),
        }),
    }
}

fn parse_headers_in_file_inner(
    token_stream: &mut FileTokens,
    context: &mut HeaderParseContext<'_>,
    file_warnings: &mut Vec<CompilerDiagnostic>,
) -> Result<FileFrontendPrepareOutput, CompilerDiagnostic> {
    let token_count = token_stream.length;

    // Tracks names introduced by real top-level declarations/imports only.
    let mut headers = Vec::with_capacity(token_stream.length / TOKEN_TO_HEADER_RATIO);
    let mut encountered_symbols: HashMap<StringId, SourceLocation> = HashMap::with_capacity(
        MINIMUM_LIKELY_DECLARATIONS + (token_stream.tokens.len() / TOKEN_TO_DECLARATION_RATIO),
    );
    // Tracks names first seen in executable start-body statements so repeat uses don't get
    // reclassified as header declarations.
    let mut start_body_symbols: HashSet<StringId> = HashSet::new();

    let mut start_function_body = Vec::new();

    let mut seen_imports: HashSet<(InternedPath, Option<StringId>)> = HashSet::new();
    let mut file_import_paths: HashSet<InternedPath> = HashSet::new();
    let mut file_imports: Vec<FileImport> = Vec::new();
    let mut file_constant_order = 0usize;
    let mut top_level_const_fragments: Vec<TopLevelConstFragment> = Vec::new();
    let mut runtime_fragment_count = 0usize;
    let mut const_template_count = 0usize;

    loop {
        let current_token = token_stream.current_token();
        let current_location = token_stream.current_location();
        token_stream.advance();

        match current_token.kind.to_owned() {
            TokenKind::Symbol(name_id) => {
                let symbol_may_start_top_level_statement = token_stream
                    .tokens
                    .get(token_stream.index.saturating_sub(2))
                    .map(|previous_token| {
                        matches!(
                            previous_token.kind,
                            TokenKind::ModuleStart | TokenKind::Newline | TokenKind::End
                        )
                    })
                    .unwrap_or(true);
                if !symbol_may_start_top_level_statement {
                    start_function_body.push(current_token);
                    continue;
                }

                // Unique non-host registry symbol
                // Only prelude-visible external symbols block local declarations;
                // package-scoped symbols that are not imported should not prevent
                // a file from declaring its own symbol with the same name.
                if !context
                    .external_package_registry
                    .is_prelude_function(context.string_table.resolve(name_id))
                {
                    // Reference to an existing symbol in scope
                    if let Some(first_location) = encountered_symbols.get(&name_id) {
                        if starts_duplicate_top_level_header_declaration(token_stream) {
                            return Err(CompilerDiagnostic::duplicate_declaration(
                                name_id,
                                first_location.clone(),
                                token_stream.current_location(),
                            ));
                        }

                        start_function_body.push(current_token);
                        // Body-level symbol/import resolution belongs to AST passes. Header parsing
                        // only validates duplicate top-level declaration starts at this stage.

                        // NEW DECLARATION IN TOP-LEVEL SCOPE
                    } else if start_body_symbols.contains(&name_id)
                        && !starts_duplicate_top_level_header_declaration(token_stream)
                    {
                        start_function_body.push(current_token);
                    } else {
                        let source_file = token_stream.src_path.to_owned();
                        let mut build_context = HeaderBuildContext {
                            external_package_registry: context.external_package_registry,
                            warnings: file_warnings,
                            source_file: &source_file,
                            file_imports: &file_import_paths,
                            file_import_entries: &file_imports,
                            file_constant_order: &mut file_constant_order,
                            string_table: context.string_table,
                            file_role: context.file_role,
                        };
                        let header = create_header(
                            token_stream.src_path.append(name_id),
                            token_stream,
                            current_location,
                            &mut build_context,
                        )?;

                        match header.kind {
                            HeaderKind::StartFunction => {
                                start_function_body.push(current_token);
                                start_body_symbols.insert(name_id);
                            }
                            _ => {
                                let name_location = header.name_location.clone();
                                headers.push(header);
                                encountered_symbols.insert(name_id, name_location);
                            }
                        }
                    };
                } else {
                    if starts_duplicate_top_level_header_declaration(token_stream) {
                        return Err(CompilerDiagnostic::reserved_builtin_name(
                            name_id,
                            token_stream.current_location(),
                        ));
                    }

                    start_function_body.push(current_token);
                }
            }

            TokenKind::Import => {
                let import_index = token_stream.index.saturating_sub(1);

                let (items, next_index) = parse_import_clause_items(
                    &token_stream.tokens,
                    import_index,
                    context.string_table,
                )?;

                for item in items {
                    let normalized_path = normalize_import_dependency_path(
                        &item.path,
                        &token_stream.src_path,
                        &item.path_location,
                        context.string_table,
                    )?;

                    let local_name = item.alias.or_else(|| normalized_path.name());
                    if let Some(name) = local_name {
                        encountered_symbols.insert(name, current_location.clone());
                    }

                    if seen_imports.insert((normalized_path.to_owned(), item.alias)) {
                        file_import_paths.insert(normalized_path.to_owned());
                        file_imports.push(FileImport {
                            header_path: normalized_path,
                            alias: item.alias,
                            location: current_location.clone(),
                            path_location: item.path_location,
                            alias_location: item.alias_location,
                            from_grouped: item.from_grouped,
                        });
                    }
                }

                token_stream.index = next_index;
            }

            TokenKind::Eof => {
                start_function_body.push(current_token);
                break;
            }

            TokenKind::Hash => {
                // `#` is no longer a declaration prefix. At a statement boundary it can only
                // start `#[...]` entry-file const templates. Any other use at a boundary is
                // invalid legacy syntax.
                let hash_at_statement_boundary = token_stream
                    .tokens
                    .get(token_stream.index.saturating_sub(2))
                    .map(|previous_token| {
                        matches!(
                            previous_token.kind,
                            TokenKind::ModuleStart | TokenKind::Newline | TokenKind::End
                        )
                    })
                    .unwrap_or(true);

                if hash_at_statement_boundary {
                    match token_stream.current_token_kind() {
                        TokenKind::TemplateHead => {
                            // `#[...]` entry-file compile-time const template.
                            if context.file_role == FileRole::Normal {
                                return Err(CompilerDiagnostic::deferred_feature(
                                    context
                                        .string_table
                                        .intern("top-level const templates in non-entry files"),
                                    current_location,
                                ));
                            }

                            if context.file_role == FileRole::ModuleFacade {
                                return Err(CompilerDiagnostic::deferred_feature(
                                    context
                                        .string_table
                                        .intern("top-level const templates in module facades"),
                                    current_location,
                                ));
                            }

                            let template_token = token_stream.current_token();
                            token_stream.advance();

                            let source_file = token_stream.src_path.to_owned();
                            let mut build_context = HeaderBuildContext {
                                external_package_registry: context.external_package_registry,
                                warnings: file_warnings,
                                source_file: &source_file,
                                file_imports: &file_import_paths,
                                file_import_entries: &file_imports,
                                file_constant_order: &mut file_constant_order,
                                string_table: context.string_table,
                                file_role: context.file_role,
                            };
                            let header = create_top_level_const_template(
                                token_stream.src_path.to_owned(),
                                template_token,
                                context.const_template_offset + const_template_count,
                                token_stream,
                                &mut build_context,
                            )?;

                            const_template_count += 1;
                            // Record placement metadata: runtime_insertion_index is the count of
                            // runtime fragments seen before this const fragment in source order.
                            top_level_const_fragments.push(TopLevelConstFragment {
                                runtime_insertion_index: context.runtime_fragment_offset
                                    + runtime_fragment_count,
                                location: header.name_location.clone(),
                                header_path: header.tokens.src_path.clone(),
                            });
                            headers.push(header);
                        }

                        TokenKind::Import => {
                            return Err(CompilerDiagnostic::legacy_import_syntax(current_location));
                        }

                        TokenKind::Symbol(_) => {
                            return Err(CompilerDiagnostic::old_prefix_declaration_syntax(
                                current_location,
                            ));
                        }

                        _ => {
                            start_function_body.push(current_token);
                        }
                    }
                } else {
                    start_function_body.push(current_token);
                }
            }

            TokenKind::Must | TokenKind::TraitThis => {
                if let Some(keyword) = reserved_trait_keyword(&current_token.kind) {
                    return Err(reserved_trait_keyword_error(keyword, current_location));
                }
            }

            TokenKind::TemplateHead => {
                // Runtime top-level templates stay in the start-function body and are
                // evaluated in source order by entry start(). Increment the runtime
                // fragment count so subsequent const fragments get the correct insertion index.
                if context.file_role == FileRole::ModuleFacade {
                    return Err(CompilerDiagnostic::runtime_template_in_module_facade(
                        current_location,
                    ));
                }
                push_runtime_template_tokens_to_start_function(
                    current_token,
                    token_stream,
                    &mut start_function_body,
                    context.string_table,
                )?;
                if context.file_role == FileRole::Entry {
                    runtime_fragment_count += 1;
                }
            }

            _ => {
                start_function_body.push(current_token);
            }
        }
    }

    // Check non-entry files for top-level executable code. Since there is no semantic consumer
    // for non-entry implicit starts, any non-trivial top-level body is rejected.
    if context.file_role != FileRole::Entry {
        let has_executable_tokens = start_function_body.iter().any(|t| {
            !matches!(
                t.kind,
                TokenKind::Eof | TokenKind::Newline | TokenKind::ModuleStart
            )
        });
        if has_executable_tokens {
            return Err(CompilerDiagnostic::invalid_top_level_runtime_statement(
                start_function_body
                    .iter()
                    .find(|t| {
                        !matches!(
                            t.kind,
                            TokenKind::Eof | TokenKind::Newline | TokenKind::ModuleStart
                        )
                    })
                    .map(|t| t.location.clone())
                    .unwrap_or_default(),
            ));
        }

        return Ok(FileFrontendPrepareOutput {
            source_file: token_stream.src_path.to_owned(),
            file_id: token_stream.file_id,
            token_count,
            headers,
            top_level_const_fragments,
            const_template_count,
            runtime_fragment_count,
            warnings: std::mem::take(file_warnings),
        });
    }

    // Entry file: build the start function header for later AST body parsing.
    // `start` is never a dependency-graph participant, so this header keeps no graph edges.

    let mut start_tokens = FileTokens::new_with_file_id(
        token_stream.src_path.to_owned(),
        token_stream.file_id,
        start_function_body,
    );
    start_tokens.canonical_os_path = token_stream.canonical_os_path.clone();

    headers.push(Header {
        kind: HeaderKind::StartFunction,
        file_role: context.file_role,
        dependencies: HashSet::new(),
        name_location: SourceLocation::default(),
        tokens: start_tokens,
        source_file: token_stream.src_path.to_owned(),
        file_imports,
    });

    Ok(FileFrontendPrepareOutput {
        source_file: token_stream.src_path.to_owned(),
        file_id: token_stream.file_id,
        token_count,
        headers,
        top_level_const_fragments,
        const_template_count,
        runtime_fragment_count,
        warnings: std::mem::take(file_warnings),
    })
}

/// Detect whether a repeated top-level symbol is starting another header declaration.
/// Already in the context of parsing a variable name that exists in this scope.
///
/// WHAT: peeks at the token sequence immediately after an already-seen symbol name.
/// WHY: duplicate header declarations must fail during header parsing instead of being
///      misclassified as references inside the implicit start function.
fn starts_duplicate_top_level_header_declaration(token_stream: &FileTokens) -> bool {
    // Qualified match arms such as `Status::Ready => ...` are executable start-body
    // syntax, not a second top-level `Status :: ...` declaration. Header splitting
    // only needs to keep these tokens with the implicit start body; AST owns the
    // actual match-pattern validation.
    if token_stream.current_token_kind() == &TokenKind::DoubleColon
        && find_top_level_fat_arrow_on_line(token_stream, token_stream.index).is_some()
    {
        return false;
    }

    match token_stream.current_token_kind() {
        // `name |...|` starts a function signature.
        TokenKind::TypeParameterBracket => true,
        // `name type T ...` starts a generic function/struct/choice declaration.
        TokenKind::Type => true,
        // `name = |...|` starts a struct declaration.
        TokenKind::Assign => matches!(
            token_stream.peek_next_token(),
            Some(TokenKind::TypeParameterBracket)
        ),
        // `name :: ...` starts a choice declaration.
        TokenKind::DoubleColon => true,
        // `name as ...` starts a type alias declaration.
        TokenKind::As => true,
        // `name #= ...` or `name #Type = ...` starts a compile-time constant declaration.
        TokenKind::Hash => true,
        _ => false,
    }
}
