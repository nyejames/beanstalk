//! Per-file header splitting.
//!
//! WHAT: walks one tokenized Beanstalk file and separates top-level declarations from the implicit
//! entry `start` body.
//! WHY: file-level control flow is different from declaration-specific parsing; keeping it separate
//! prevents the header entry point from becoming a parser monolith.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::headers::const_fragments::create_top_level_const_template;
use crate::compiler_frontend::headers::header_dispatch::create_header;
use crate::compiler_frontend::headers::imports::normalize_import_dependency_path;
use crate::compiler_frontend::headers::start_capture::push_runtime_template_tokens_to_start_function;
use crate::compiler_frontend::headers::types::{
    FileImport, FileReExport, FileRole, Header, HeaderBuildContext, HeaderKind, HeaderParseContext,
    TopLevelConstFragment,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::const_paths::parse_import_clause_items;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword, reserved_trait_keyword_error,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::projects::settings::{
    MINIMUM_LIKELY_DECLARATIONS, TOKEN_TO_DECLARATION_RATIO, TOKEN_TO_HEADER_RATIO,
};
use std::collections::HashSet;
use std::rc::Rc;

// Top-level declarations are module-visible; non-declaration statements are collected into the
// implicit start-function header for that file.
pub(super) fn parse_headers_in_file(
    token_stream: &mut FileTokens,
    context: &mut HeaderParseContext<'_>,
) -> Result<Vec<Header>, CompilerError> {
    let visible_constant_placeholders =
        discover_visible_constant_placeholders(token_stream, context.string_table)?;

    // Tracks names introduced by real top-level declarations/imports only.
    let mut headers = Vec::with_capacity(token_stream.length / TOKEN_TO_HEADER_RATIO);
    let mut encountered_symbols: HashSet<StringId> = HashSet::with_capacity(
        MINIMUM_LIKELY_DECLARATIONS + (token_stream.tokens.len() / TOKEN_TO_DECLARATION_RATIO),
    );
    // Tracks names first seen in executable start-body statements so repeat uses don't get
    // reclassified as header declarations.
    let mut start_body_symbols: HashSet<StringId> = HashSet::new();

    let mut next_statement_exported = false;
    let mut start_function_body = Vec::new();

    let mut seen_imports: HashSet<(InternedPath, Option<StringId>)> = HashSet::new();
    let mut file_import_paths: HashSet<InternedPath> = HashSet::new();
    let mut file_imports: Vec<FileImport> = Vec::new();
    let mut file_re_exports: Vec<FileReExport> = Vec::new();
    let mut file_constant_order = 0usize;

    loop {
        let current_token = token_stream.current_token();
        let current_location = token_stream.current_location();
        token_stream.advance();

        match current_token.kind.to_owned() {
            TokenKind::Symbol(name_id) => {
                let symbol_may_start_top_level_statement = next_statement_exported
                    || token_stream
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
                    if encountered_symbols.contains(&name_id) {
                        if starts_duplicate_top_level_header_declaration(
                            token_stream,
                            next_statement_exported,
                        ) {
                            crate::return_rule_error!(
                                "There is already a top-level declaration using this name. Functions, structs, and exported constants must use unique names within a file.",
                                token_stream.current_location(), {
                                    CompilationStage => "Header Parsing",
                                    ConflictType => "DuplicateTopLevelDeclaration",
                                    PrimarySuggestion => "Rename the later declaration so it does not collide with the existing top-level symbol",
                                }
                            )
                        }

                        if next_statement_exported {
                            crate::return_rule_error!(
                                "There is already a constant, function or struct using this name. You can't shadow these. Choose a unique name",
                                token_stream.current_location(), {
                                    CompilationStage => "Header Parsing",
                                    ConflictType => "DuplicateTopLevelDeclaration",
                                    PrimarySuggestion => "Rename the constant to something unique"
                                }
                            )
                        }

                        start_function_body.push(current_token);
                        // Body-level symbol/import resolution belongs to AST passes. Header parsing
                        // only validates duplicate top-level declaration starts at this stage.

                        // NEW DECLARATION IN TOP-LEVEL SCOPE
                    } else if start_body_symbols.contains(&name_id)
                        && !next_statement_exported
                        && !starts_duplicate_top_level_header_declaration(
                            token_stream,
                            next_statement_exported,
                        )
                    {
                        start_function_body.push(current_token);
                    } else {
                        let source_file = token_stream.src_path.to_owned();
                        let mut build_context = HeaderBuildContext {
                            external_package_registry: context.external_package_registry,
                            style_directives: context.style_directives,
                            warnings: context.warnings,
                            project_path_resolver: context.project_path_resolver.clone(),
                            path_format_config: context.path_format_config.clone(),
                            visible_constant_placeholders: Rc::clone(
                                &visible_constant_placeholders,
                            ),
                            source_file: &source_file,
                            file_imports: &file_import_paths,
                            file_import_entries: &file_imports,
                            file_re_export_entries: &file_re_exports,
                            file_constant_order: &mut file_constant_order,
                            string_table: context.string_table,
                        };
                        let header = create_header(
                            token_stream.src_path.append(name_id),
                            next_statement_exported,
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
                                headers.push(header);
                                encountered_symbols.insert(name_id);
                            }
                        }
                        next_statement_exported = false;
                    };
                } else {
                    start_function_body.push(current_token);
                    if next_statement_exported {
                        next_statement_exported = false;
                        context.warnings.push(CompilerWarning::new(
                            "You can't export a reference to a host function, only new declarations.",
                            token_stream.current_location(),
                            WarningKind::PointlessExport,
                        ))
                    }
                }
            }

            TokenKind::Import => {
                let import_index = token_stream.index.saturating_sub(1);
                let (items, next_index) = parse_import_clause_items(
                    &token_stream.tokens,
                    import_index,
                    context.string_table,
                )?;

                if next_statement_exported {
                    // `#import @...` is facade-only re-export syntax.
                    if context.file_role != FileRole::ModuleFacade {
                        crate::return_rule_error!(
                            "`#import` can only be used in `#mod.bst` to re-export symbols from a library facade.",
                            current_location, {
                                CompilationStage => "Header Parsing",
                                PrimarySuggestion => "Move this `#import` into a `#mod.bst` file, or use `import` for local bindings",
                            }
                        );
                    }

                    for item in items {
                        let normalized_path = normalize_import_dependency_path(
                            &item.path,
                            &token_stream.src_path,
                            context.string_table,
                        )?;

                        file_re_exports.push(FileReExport {
                            header_path: normalized_path,
                            alias: item.alias,
                            location: current_location.clone(),
                            path_location: item.path_location,
                            alias_location: item.alias_location,
                        });
                    }

                    next_statement_exported = false;
                } else {
                    for item in items {
                        let normalized_path = normalize_import_dependency_path(
                            &item.path,
                            &token_stream.src_path,
                            context.string_table,
                        )?;

                        let local_name = item.alias.or_else(|| normalized_path.name());
                        if let Some(name) = local_name {
                            encountered_symbols.insert(name);
                        }

                        if seen_imports.insert((normalized_path.to_owned(), item.alias)) {
                            file_import_paths.insert(normalized_path.to_owned());
                            file_imports.push(FileImport {
                                header_path: normalized_path,
                                alias: item.alias,
                                location: current_location.clone(),
                                path_location: item.path_location,
                                alias_location: item.alias_location,
                            });
                        }
                    }
                }

                token_stream.index = next_index;
            }

            TokenKind::Eof => {
                start_function_body.push(current_token);
                break;
            }

            TokenKind::Hash => {
                next_statement_exported = true;
            }

            TokenKind::Must | TokenKind::TraitThis => {
                if let Some(keyword) = reserved_trait_keyword(&current_token.kind) {
                    return Err(reserved_trait_keyword_error(
                        keyword,
                        current_location,
                        "Header Parsing",
                        "Use a normal identifier or type name until traits are implemented",
                    ));
                }
            }

            TokenKind::TemplateHead => {
                if next_statement_exported {
                    if context.file_role == FileRole::Normal {
                        crate::return_rule_error!(
                            "Top-level const templates are currently only supported in the module entry file.",
                            current_location, {
                                CompilationStage => "Header Parsing",
                                PrimarySuggestion => "Move this '#[...]' template to the entry file or remove the export marker",
                            }
                        );
                    }
                    let source_file = token_stream.src_path.to_owned();
                    let mut build_context = HeaderBuildContext {
                        external_package_registry: context.external_package_registry,
                        style_directives: context.style_directives,
                        warnings: context.warnings,
                        project_path_resolver: context.project_path_resolver.clone(),
                        path_format_config: context.path_format_config.clone(),
                        visible_constant_placeholders: Rc::clone(&visible_constant_placeholders),
                        source_file: &source_file,
                        file_imports: &file_import_paths,
                        file_import_entries: &file_imports,
                        file_re_export_entries: &file_re_exports,
                        file_constant_order: &mut file_constant_order,
                        string_table: context.string_table,
                    };
                    let header = create_top_level_const_template(
                        token_stream.src_path.to_owned(),
                        current_token,
                        *context.const_template_number,
                        token_stream,
                        &mut build_context,
                    )?;

                    *context.const_template_number += 1;
                    // Record placement metadata: runtime_insertion_index is the count of
                    // runtime fragments seen before this const fragment in source order.
                    context
                        .top_level_const_fragments
                        .push(TopLevelConstFragment {
                            runtime_insertion_index: *context.runtime_fragment_count,
                            location: header.name_location.clone(),
                            header_path: header.tokens.src_path.clone(),
                        });
                    headers.push(header);
                    next_statement_exported = false;
                } else {
                    // Runtime top-level templates stay in the start-function body and are
                    // evaluated in source order by entry start(). Increment the runtime
                    // fragment count so subsequent const fragments get the correct insertion index.
                    if context.file_role == FileRole::ModuleFacade {
                        crate::return_rule_error!(
                            "Library facade files (#mod.bst) cannot contain runtime top-level templates.",
                            current_location, {
                                CompilationStage => "Header Parsing",
                                PrimarySuggestion => "Remove the template or move it to a normal source file.",
                            }
                        );
                    }
                    push_runtime_template_tokens_to_start_function(
                        current_token,
                        token_stream,
                        &mut start_function_body,
                    )?;
                    if context.file_role == FileRole::Entry {
                        *context.runtime_fragment_count += 1;
                    }
                }
            }

            _ => {
                start_function_body.push(current_token);
            }
        }
    }

    // Ensure re-exports collected during parsing survive to build_module_symbols even when
    // no header was created after the last `#import` clause (common in #mod.bst facades).
    // WHY: non-entry files do not produce a start-function header, so file_re_exports would
    // otherwise be lost when parse_headers_in_file returns.
    if !file_re_exports.is_empty() {
        for header in &mut headers {
            header.file_re_exports = file_re_exports.clone();
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
            let msg = if context.file_role == FileRole::ModuleFacade {
                "Library facade files (#mod.bst) cannot contain top-level executable statements."
            } else {
                "Non-entry files cannot contain top-level executable statements. Move this code into a named function or into the entry file."
            };
            crate::return_rule_error!(
                msg,
                start_function_body
                    .iter()
                    .find(|t| !matches!(t.kind, TokenKind::Eof | TokenKind::Newline | TokenKind::ModuleStart))
                    .map(|t| t.location.clone())
                    .unwrap_or_default(), {
                    CompilationStage => "Header Parsing",
                    PrimarySuggestion => "Wrap this code in a named function declaration",
                }
            );
        }
        return Ok(headers);
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
        exported: false,
        dependencies: HashSet::new(),
        name_location: SourceLocation::default(),
        tokens: start_tokens,
        source_file: token_stream.src_path.to_owned(),
        file_imports,
        file_re_exports: Vec::new(),
    });

    Ok(headers)
}

/// Detect whether a repeated top-level symbol is starting another header declaration.
/// Already in the context of parsing a variable name that exists in this scope.
///
/// WHAT: peeks at the token sequence immediately after an already-seen symbol name.
/// WHY: duplicate header declarations must fail during header parsing instead of being
///      misclassified as references inside the implicit start function.
fn starts_duplicate_top_level_header_declaration(
    token_stream: &FileTokens,
    next_statement_exported: bool,
) -> bool {
    if next_statement_exported {
        return matches!(
            token_stream.current_token_kind(),
            // Exported functions still parse like normal `name |...|` declarations.
            TokenKind::TypeParameterBracket
                // Exported choice declarations parse as `#Name :: ...`.
                | TokenKind::DoubleColon
                // Exported type aliases parse as `#Name as ...`.
                | TokenKind::As
        );
    }

    match token_stream.current_token_kind() {
        // `name |...|` starts a function signature.
        TokenKind::TypeParameterBracket => true,
        // `name = |...|` starts a struct declaration.
        TokenKind::Assign => matches!(
            token_stream.peek_next_token(),
            Some(TokenKind::TypeParameterBracket)
        ),
        // `name :: ...` starts a choice declaration.
        TokenKind::DoubleColon => true,
        // `name as ...` starts a type alias declaration.
        TokenKind::As => true,
        _ => false,
    }
}

fn discover_visible_constant_placeholders(
    token_stream: &FileTokens,
    string_table: &mut StringTable,
) -> Result<Rc<crate::compiler_frontend::ast::TopLevelDeclarationIndex>, CompilerError> {
    let mut placeholders = Vec::new();
    let mut seen_paths = HashSet::new();
    let mut next_statement_exported = false;
    let mut scope_depth = 0usize;
    let tokens = &token_stream.tokens;

    let mut index = 0usize;
    while index < tokens.len() {
        if scope_depth == 0 && matches!(tokens[index].kind, TokenKind::Import) {
            let (items, next_index) = parse_import_clause_items(tokens, index, string_table)?;
            for item in items {
                let normalized = normalize_import_dependency_path(
                    &item.path,
                    &token_stream.src_path,
                    string_table,
                )?;
                if normalized.name().is_some() {
                    let placeholder = header_constant_placeholder_declaration(
                        normalized,
                        tokens[index].location.clone(),
                    );
                    if seen_paths.insert(placeholder.id.clone()) {
                        placeholders.push(placeholder);
                    }
                }
            }
            index = next_index;
            continue;
        }

        if scope_depth == 0 && matches!(tokens[index].kind, TokenKind::Hash) {
            next_statement_exported = true;
            index += 1;
            continue;
        }

        if scope_depth == 0
            && next_statement_exported
            && let TokenKind::Symbol(name_id) = tokens[index].kind
            && exported_symbol_starts_constant(tokens, index + 1)
        {
            let placeholder = header_constant_placeholder_declaration(
                token_stream.src_path.append(name_id),
                tokens[index].location.clone(),
            );
            if seen_paths.insert(placeholder.id.clone()) {
                placeholders.push(placeholder);
            }
        }

        match tokens[index].kind {
            TokenKind::Colon => {
                scope_depth += 1;
                next_statement_exported = false;
            }
            TokenKind::End => {
                scope_depth = scope_depth.saturating_sub(1);
                next_statement_exported = false;
            }
            TokenKind::Newline | TokenKind::ModuleStart => {}
            _ => {
                if scope_depth == 0 {
                    next_statement_exported = false;
                }
            }
        }

        index += 1;
    }

    Ok(Rc::new(
        crate::compiler_frontend::ast::TopLevelDeclarationIndex::new(placeholders),
    ))
}

fn exported_symbol_starts_constant(tokens: &[Token], next_index: usize) -> bool {
    match tokens.get(next_index).map(|token| &token.kind) {
        Some(TokenKind::TypeParameterBracket) | Some(TokenKind::DoubleColon) => false,
        Some(TokenKind::Assign) => !matches!(
            tokens.get(next_index + 1).map(|token| &token.kind),
            Some(TokenKind::TypeParameterBracket)
        ),
        Some(TokenKind::Mutable)
        | Some(TokenKind::DatatypeInt)
        | Some(TokenKind::DatatypeFloat)
        | Some(TokenKind::DatatypeBool)
        | Some(TokenKind::DatatypeString)
        | Some(TokenKind::DatatypeChar)
        | Some(TokenKind::OpenCurly)
        | Some(TokenKind::Symbol(_)) => true,
        _ => false,
    }
}

fn header_constant_placeholder_declaration(
    id: InternedPath,
    location: SourceLocation,
) -> Declaration {
    Declaration {
        id,
        value: Expression::no_value(location, DataType::Inferred, ValueMode::ImmutableOwned),
    }
}
