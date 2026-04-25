//! File-header parsing for the frontend pre-AST stage.
//!
//! WHAT: splits tokenized source files into function/struct/choice/constant/start-function headers
//! plus const-fragment placement metadata for the entry file. Also collects the header-owned
//! `ModuleSymbols` package: all order-independent declaration metadata and builtin symbol data
//! needed by dependency sorting and AST construction.
//! WHY: later AST passes need declaration-shaped inputs before body parsing, while still preserving
//! file-local visibility, constant ordering, and entry-file template ordering. Owning the full
//! top-level symbol collection here removes the need for a separate manifest-building stage.
//!
//! ## Stage contract
//!
//! Headers parse the declaration shell of every top-level kind:
//! - **Functions**: full `FunctionSignature` (parameter types + return types). Strict edges come
//!   from signature type references only. Body tokens are captured but not parsed.
//! - **Structs**: full field list (`parse_struct_shell`). Strict edges come from field type refs.
//! - **Choices**: nominal path + variant list (alpha: unit-only). In alpha, variant payload edges
//!   do not exist because payload variants are deferred.
//! - **Constants**: `DeclarationSyntax` (type annotation + initializer tokens). Strict edges from
//!   the declared type annotation only; initializer expression symbols are NOT strict edges.
//! - **StartFunction**: body tokens captured for AST emission; NOT part of dependency sorting and
//!   stores no graph edges.
//!
//! Top-level runtime templates are evaluated in entry `start()` in source order.
//! Entry `start()` returns `Vec<String>`.
//! Only const top-level fragments carry placement metadata; they do not pass through HIR.
//! Start functions are build-system-only; they are not importable or callable from modules.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext, TopLevelDeclarationIndex};
use crate::compiler_frontend::builtins::error_type::{
    is_reserved_builtin_symbol, register_builtin_error_types,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::declaration_syntax::choice::parse_choice_shell as parse_choice_header_payload;
use crate::compiler_frontend::declaration_syntax::declaration_shell::{
    DeclarationSyntax, parse_declaration_syntax,
};
use crate::compiler_frontend::declaration_syntax::r#struct::parse_struct_shell;
use crate::compiler_frontend::declaration_syntax::type_syntax::for_each_named_type_in_data_type;
use crate::compiler_frontend::headers::module_symbols::{ModuleSymbols, register_declared_symbol};
// Temporary re-exports for Commit 1 compatibility. Removed in Commit 2.
pub use crate::compiler_frontend::headers::types::{
    FileImport, Header, HeaderKind, HeaderParseOptions, Headers, TopLevelConstFragment,
};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::const_paths::parse_import_clause_tokens;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::reserved_trait_syntax::{
    ReservedTraitKeyword, reserved_trait_declaration_error, reserved_trait_keyword,
    reserved_trait_keyword_error,
};
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::identifier_policy::{
    IdentifierNamingKind, ensure_not_keyword_shadow_identifier, naming_warning_for_identifier,
};

use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::token_scan::consume_balanced_template_region;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::projects::settings::{
    IMPLICIT_START_FUNC_NAME, MINIMUM_LIKELY_DECLARATIONS, TOKEN_TO_DECLARATION_RATIO,
    TOKEN_TO_HEADER_RATIO, TOP_LEVEL_CONST_TEMPLATE_NAME,
};
use crate::return_rule_error;
use std::collections::HashSet;

use std::path::Path;
use std::rc::Rc;

// Shared file-level state that stays live while one source file is being split into headers.
struct HeaderParseContext<'a> {
    host_function_registry: &'a HostRegistry,
    style_directives: &'a StyleDirectiveRegistry,
    warnings: &'a mut Vec<CompilerWarning>,
    is_entry_file: bool,
    project_path_resolver: Option<ProjectPathResolver>,
    path_format_config: PathStringFormatConfig,
    string_table: &'a mut StringTable,
    const_template_number: &'a mut usize,
    /// Count of runtime (non-exported) top-level templates seen so far in the entry file.
    /// Used as the runtime_insertion_index for the next const fragment.
    runtime_fragment_count: &'a mut usize,
    top_level_const_fragments: &'a mut Vec<TopLevelConstFragment>,
}

// Shared per-header builder inputs that stay stable while one declaration is classified.
struct HeaderBuildContext<'a> {
    host_function_registry: &'a HostRegistry,
    style_directives: &'a StyleDirectiveRegistry,
    warnings: &'a mut Vec<CompilerWarning>,
    project_path_resolver: Option<ProjectPathResolver>,
    path_format_config: PathStringFormatConfig,
    visible_constant_placeholders: Rc<TopLevelDeclarationIndex>,
    source_file: &'a InternedPath,
    file_imports: &'a HashSet<InternedPath>,
    file_import_entries: &'a [FileImport],
    file_constant_order: &'a mut usize,
    string_table: &'a mut StringTable,
}

pub fn parse_headers(
    tokenized_files: Vec<FileTokens>,
    host_registry: &HostRegistry,
    warnings: &mut Vec<CompilerWarning>,
    entry_file_path: &Path,
    options: HeaderParseOptions,
    string_table: &mut StringTable,
) -> Result<Headers, Vec<CompilerError>> {
    let HeaderParseOptions {
        entry_file_id,
        project_path_resolver,
        path_format_config,
        style_directives,
    } = options;

    let mut headers: Vec<Header> = Vec::new();
    let mut errors: Vec<CompilerError> = Vec::new();
    let mut const_template_count = 0;
    let mut top_level_const_fragments = Vec::new();
    // Tracks runtime fragments seen so far in the entry file, for const fragment insertion indices.
    let mut runtime_fragment_count = 0usize;

    for mut file in tokenized_files {
        let is_entry_file = match (entry_file_id, file.file_id) {
            (Some(expected_id), Some(current_id)) => expected_id == current_id,
            _ => file.src_path.to_path_buf(string_table) == entry_file_path,
        };

        let mut parse_context = HeaderParseContext {
            host_function_registry: host_registry,
            style_directives: &style_directives,
            warnings,
            is_entry_file,
            project_path_resolver: project_path_resolver.clone(),
            path_format_config: path_format_config.clone(),
            string_table,
            const_template_number: &mut const_template_count,
            runtime_fragment_count: &mut runtime_fragment_count,
            top_level_const_fragments: &mut top_level_const_fragments,
        };

        let headers_from_file = parse_headers_in_file(&mut file, &mut parse_context);

        match headers_from_file {
            Ok(file_headers) => {
                headers.extend(file_headers);
            }
            Err(e) => {
                errors.push(e);
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let module_symbols =
        build_module_symbols(&headers, string_table).map_err(|mut symbol_errors| {
            errors.append(&mut symbol_errors);
            errors
        })?;

    Ok(Headers {
        headers,
        top_level_const_fragments,
        entry_runtime_fragment_count: runtime_fragment_count,
        module_symbols,
    })
}

/// Collect all order-independent top-level symbol metadata from parsed (unsorted) headers.
///
/// WHAT: validates symbol names, builds import/export/source maps, registers builtins.
/// WHY: all this work depends only on the per-header data available immediately after parsing;
/// it does not require dependency order. `declarations` is intentionally left empty here
/// and filled by `resolve_module_dependencies` once headers are sorted.
fn build_module_symbols(
    headers: &[Header],
    string_table: &mut StringTable,
) -> Result<ModuleSymbols, Vec<CompilerError>> {
    let mut module_symbols = ModuleSymbols::empty();
    let mut errors: Vec<CompilerError> = Vec::new();

    for header in headers {
        if let Some(symbol_name) = header.tokens.src_path.name() {
            let symbol_name_text = string_table.resolve(symbol_name).to_owned();

            if let Err(error) = ensure_not_keyword_shadow_identifier(
                &symbol_name_text,
                header.name_location.to_owned(),
                "Module Declaration Collection",
            ) {
                errors.push(error);
                continue;
            }

            if is_reserved_builtin_symbol(&symbol_name_text) {
                errors.push(CompilerError::new_rule_error(
                    format!("'{symbol_name_text}' is reserved as a builtin language type."),
                    header.name_location.to_owned(),
                ));
                continue;
            }
        }

        module_symbols
            .module_file_paths
            .insert(header.source_file.to_owned());
        module_symbols.canonical_source_by_symbol_path.insert(
            header.tokens.src_path.to_owned(),
            header.canonical_source_file(string_table),
        );
        module_symbols
            .file_imports_by_source
            .entry(header.source_file.to_owned())
            .or_insert_with(|| header.file_imports.to_owned());

        match &header.kind {
            HeaderKind::Function { .. } => {
                register_declared_symbol(
                    &mut module_symbols,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported),
                );
            }
            HeaderKind::Struct { .. } => {
                register_declared_symbol(
                    &mut module_symbols,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported),
                );
            }
            HeaderKind::Choice { .. } => {
                register_declared_symbol(
                    &mut module_symbols,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported),
                );
            }
            HeaderKind::StartFunction => {
                let start_name = header
                    .source_file
                    .join_str(IMPLICIT_START_FUNC_NAME, string_table);
                register_declared_symbol(
                    &mut module_symbols,
                    &start_name,
                    &header.source_file,
                    None,
                );
            }
            HeaderKind::Constant { .. } => {
                register_declared_symbol(
                    &mut module_symbols,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported),
                );
            }
            _ => {}
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Register builtin error types: visible paths, struct fields, AST nodes, and declarations.
    // WHY: builtins are merged once here so AST passes see them without a separate absorption step.
    let builtin_manifest = register_builtin_error_types(string_table);
    module_symbols
        .builtin_visible_symbol_paths
        .extend(builtin_manifest.visible_symbol_paths.iter().cloned());
    module_symbols.builtin_declarations = builtin_manifest.declarations;
    module_symbols
        .resolved_struct_fields_by_path
        .extend(builtin_manifest.resolved_struct_fields_by_path);
    module_symbols
        .struct_source_by_path
        .extend(builtin_manifest.struct_source_by_path);
    module_symbols
        .builtin_struct_ast_nodes
        .extend(builtin_manifest.ast_struct_nodes);
    module_symbols.seed_declaration_stubs(headers, string_table);

    Ok(module_symbols)
}

// Top-level declarations are module-visible; non-declaration statements are collected into the
// implicit start-function header for that file.
fn parse_headers_in_file(
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

    let mut file_import_paths: HashSet<InternedPath> = HashSet::new();
    let mut file_imports: Vec<FileImport> = Vec::new();
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
                if context
                    .host_function_registry
                    .get_function(context.string_table.resolve(name_id))
                    .is_none()
                {
                    // Reference to an existing symbol in scope
                    if encountered_symbols.contains(&name_id) {
                        if starts_duplicate_top_level_header_declaration(
                            token_stream,
                            next_statement_exported,
                        ) {
                            return_rule_error!(
                                "There is already a top-level declaration using this name. Functions, structs, and exported constants must use unique names within a file.",
                                token_stream.current_location(), {
                                    CompilationStage => "Header Parsing",
                                    ConflictType => "DuplicateTopLevelDeclaration",
                                    PrimarySuggestion => "Rename the later declaration so it does not collide with the existing top-level symbol",
                                }
                            )
                        }

                        if next_statement_exported {
                            return_rule_error!(
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
                            host_function_registry: context.host_function_registry,
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
                let (paths, next_index) =
                    parse_import_clause_tokens(&token_stream.tokens, import_index)?;

                for path in paths {
                    let normalized_path = normalize_import_dependency_path(
                        &path,
                        &token_stream.src_path,
                        context.string_table,
                    )?;

                    if let Some(name) = normalized_path.name() {
                        encountered_symbols.insert(name);
                    }

                    if file_import_paths.insert(normalized_path.to_owned()) {
                        file_imports.push(FileImport {
                            header_path: normalized_path,
                            location: current_location.clone(),
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
                    if !context.is_entry_file {
                        return_rule_error!(
                            "Top-level const templates are currently only supported in the module entry file.",
                            current_location, {
                                CompilationStage => "Header Parsing",
                                PrimarySuggestion => "Move this '#[...]' template to the entry file or remove the export marker",
                            }
                        );
                    }
                    let source_file = token_stream.src_path.to_owned();
                    let mut build_context = HeaderBuildContext {
                        host_function_registry: context.host_function_registry,
                        style_directives: context.style_directives,
                        warnings: context.warnings,
                        project_path_resolver: context.project_path_resolver.clone(),
                        path_format_config: context.path_format_config.clone(),
                        visible_constant_placeholders: Rc::clone(&visible_constant_placeholders),
                        source_file: &source_file,
                        file_imports: &file_import_paths,
                        file_import_entries: &file_imports,
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
                    push_runtime_template_tokens_to_start_function(
                        current_token,
                        token_stream,
                        &mut start_function_body,
                    )?;
                    if context.is_entry_file {
                        *context.runtime_fragment_count += 1;
                    }
                }
            }

            _ => {
                start_function_body.push(current_token);
            }
        }
    }

    // Check non-entry files for top-level executable code. Since there is no semantic consumer
    // for non-entry implicit starts, any non-trivial top-level body is rejected.
    if !context.is_entry_file {
        let has_executable_tokens = start_function_body.iter().any(|t| {
            !matches!(
                t.kind,
                TokenKind::Eof | TokenKind::Newline | TokenKind::ModuleStart
            )
        });
        if has_executable_tokens {
            return_rule_error!(
                "Non-entry files cannot contain top-level executable statements. Move this code into a named function or into the entry file.",
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
        _ => false,
    }
}

fn normalize_import_dependency_path(
    import_path: &InternedPath,
    source_file: &InternedPath,
    string_table: &mut StringTable,
) -> Result<InternedPath, CompilerError> {
    let mut import_components = import_path.as_components().iter().copied();
    let Some(first) = import_components.next() else {
        return Ok(import_path.to_owned());
    };

    let first_segment = string_table.resolve(first);
    if first_segment != "." && first_segment != ".." {
        return Ok(import_path.to_owned());
    }

    let mut resolved_components = source_file.as_components().to_vec();
    resolved_components.pop();

    for component in import_path.as_components() {
        match string_table.resolve(*component) {
            "." => {}
            ".." => {
                resolved_components.pop();
            }
            _ => resolved_components.push(*component),
        }
    }

    Ok(InternedPath::from_components(resolved_components))
}

// WHAT: classifies one top-level declaration by its leading token and builds the concrete header
// payload (kind + body token slice + dependency set) that later AST passes consume.
//
// WHY: every declaration kind (function, struct, choice/union, constant) has a different leading
// token pattern. This function dispatches on that token and delegates to kind-specific helpers
// where they exist, or captures body tokens directly for simpler cases.
//
// Dispatch summary:
//   `|`  (TypeParameterBracket)  → function signature + body token capture
//   `=`  (Assign)                → struct `= |fields|` or exported constant `= <expr>`
//   `::`  (DoubleColon)          → choice/union variant list
//   type tokens / `~`            → exported constant with implicit `=` already consumed
//   `must` / `This`              → reserved trait syntax, error
//   anything else                → no header created (e.g. start-template body lines)
fn create_header(
    full_name: InternedPath,
    exported: bool,
    token_stream: &mut FileTokens,
    name_location: SourceLocation,
    context: &mut HeaderBuildContext<'_>,
) -> Result<Header, CompilerError> {
    let Some(declaration_name) = full_name.name() else {
        return Err(CompilerError::compiler_error(
            "Header declaration path is missing its declaration name.",
        ));
    };
    let declaration_name_text = context.string_table.resolve(declaration_name);

    // Only imported symbols become inter-header dependency edges here.
    let mut dependencies: HashSet<InternedPath> = HashSet::new();
    let mut kind: HeaderKind = HeaderKind::StartFunction;
    let mut body = Vec::new();
    let current_token = token_stream.current_token_kind().to_owned();

    match current_token {
        // Function declaration: `name |params| -> return_type : body ;`
        TokenKind::TypeParameterBracket => {
            ensure_not_keyword_shadow_identifier(
                declaration_name_text,
                name_location.to_owned(),
                "Header Parsing",
            )?;
            emit_header_naming_warning(
                context.warnings,
                declaration_name_text,
                name_location.to_owned(),
                IdentifierNamingKind::ValueLike,
            );

            let signature_context = ScopeContext::new(
                ContextKind::ConstantHeader,
                full_name.to_owned(),
                Rc::clone(&context.visible_constant_placeholders),
                context.host_function_registry.to_owned(),
                vec![],
            )
            .with_project_path_resolver(context.project_path_resolver.clone())
            .with_source_file_scope(context.source_file.to_owned())
            .with_path_format_config(context.path_format_config.clone());

            let signature = FunctionSignature::new(
                token_stream,
                context.warnings,
                context.string_table,
                &full_name,
                &signature_context,
            )?;

            // Strict edges: parameter + return type references only.
            for param in &signature.parameters {
                for_each_named_type_in_data_type(&param.value.data_type, &mut |type_name| {
                    collect_named_type_dependency_edge(
                        type_name,
                        context.file_imports,
                        context.source_file,
                        context.string_table,
                        &mut dependencies,
                    );
                });
            }

            for ret in &signature.returns {
                for_each_named_type_in_data_type(ret.value.data_type(), &mut |type_name| {
                    collect_named_type_dependency_edge(
                        type_name,
                        context.file_imports,
                        context.source_file,
                        context.string_table,
                        &mut dependencies,
                    );
                });
            }

            capture_function_body_tokens(token_stream, &mut body)?;

            kind = HeaderKind::Function { signature };
        }

        // `must` keyword: reserved for future trait implementation syntax.
        TokenKind::Must => {
            return Err(reserved_trait_declaration_error(
                token_stream.current_location(),
            ));
        }

        // `This` keyword: reserved for future trait `This` self-type syntax.
        TokenKind::TraitThis => {
            return Err(reserved_trait_keyword_error(
                ReservedTraitKeyword::This,
                token_stream.current_location(),
                "Header Parsing",
                "Use a normal identifier or type name until traits are implemented",
            ));
        }

        // `=` (Assign): either `name = |fields|` (struct) or `#name = <expr>` (exported constant).
        // Peek ahead: if the next token is `|`, this is a struct definition; otherwise a constant.
        TokenKind::Assign => {
            if let Some(TokenKind::TypeParameterBracket) = token_stream.peek_next_token() {
                ensure_not_keyword_shadow_identifier(
                    declaration_name_text,
                    name_location.to_owned(),
                    "Header Parsing",
                )?;
                emit_header_naming_warning(
                    context.warnings,
                    declaration_name_text,
                    name_location.to_owned(),
                    IdentifierNamingKind::TypeLike,
                );

                token_stream.advance();

                // Parse field shell directly — avoids reparsing in the AST type-resolution pass.
                // WHY: the header stage owns top-level shell parsing; AST owns body/executable parsing.
                let struct_context = ScopeContext::new(
                    ContextKind::ConstantHeader,
                    full_name.to_owned(),
                    Rc::clone(&context.visible_constant_placeholders),
                    context.host_function_registry.to_owned(),
                    vec![],
                )
                .with_style_directives(context.style_directives)
                .with_project_path_resolver(context.project_path_resolver.clone())
                .with_source_file_scope(context.source_file.to_owned())
                .with_path_format_config(context.path_format_config.clone());

                // Field IDs are built as `token_stream.src_path.append(field_name)` inside
                // parse_signature_members. Set src_path to the struct's own path so field IDs
                // are `struct_path/field_name` — matching what resolve_struct_field_types expects.
                // WHY: token_stream.src_path is the file path at this point; fields need to be
                // children of the struct path, not siblings of the struct in the file namespace.
                let saved_src_path = token_stream.src_path.to_owned();
                token_stream.src_path = full_name.to_owned();
                let fields =
                    parse_struct_shell(token_stream, &struct_context, context.string_table);
                token_stream.src_path = saved_src_path;
                let fields = fields?;

                // Collect strict type edges from field types only (no default-expression edges).
                // WHY: struct field type refs are the only struct edges that constrain sort order.
                for field in &fields {
                    for_each_named_type_in_data_type(&field.value.data_type, &mut |type_name| {
                        collect_named_type_dependency_edge(
                            type_name,
                            context.file_imports,
                            context.source_file,
                            context.string_table,
                            &mut dependencies,
                        );
                    });
                }

                kind = HeaderKind::Struct { fields };
            } else if exported {
                ensure_not_keyword_shadow_identifier(
                    declaration_name_text,
                    name_location.to_owned(),
                    "Header Parsing",
                )?;
                emit_header_naming_warning(
                    context.warnings,
                    declaration_name_text,
                    name_location.to_owned(),
                    IdentifierNamingKind::TopLevelConstant,
                );

                let constant_header = create_constant_header_payload(
                    &full_name,
                    token_stream,
                    context,
                    &mut dependencies,
                )?;

                kind = HeaderKind::Constant {
                    declaration: constant_header,
                };
            }
        }

        // Type-starting tokens: `#name ~Type`, `#name Int`, `#name {collection}`, etc.
        // These only produce a header if the declaration is exported (`#`). Non-exported
        // declarations starting with a type are top-level template or body lines, not headers.
        TokenKind::Mutable
        | TokenKind::DatatypeInt
        | TokenKind::DatatypeFloat
        | TokenKind::DatatypeBool
        | TokenKind::DatatypeString
        | TokenKind::DatatypeChar
        | TokenKind::OpenCurly
        | TokenKind::Symbol(_)
            if exported =>
        {
            ensure_not_keyword_shadow_identifier(
                declaration_name_text,
                name_location.to_owned(),
                "Header Parsing",
            )?;
            emit_header_naming_warning(
                context.warnings,
                declaration_name_text,
                name_location.to_owned(),
                IdentifierNamingKind::TopLevelConstant,
            );

            let constant_header = create_constant_header_payload(
                &full_name,
                token_stream,
                context,
                &mut dependencies,
            )?;

            kind = HeaderKind::Constant {
                declaration: constant_header,
            };
        }

        // `::` (DoubleColon): choice/union declaration `name :: VariantA | VariantB | ...`
        TokenKind::DoubleColon => {
            ensure_not_keyword_shadow_identifier(
                declaration_name_text,
                name_location.to_owned(),
                "Header Parsing",
            )?;
            emit_header_naming_warning(
                context.warnings,
                declaration_name_text,
                name_location.to_owned(),
                IdentifierNamingKind::TypeLike,
            );

            let choice_header =
                parse_choice_header_payload(token_stream, context.string_table, context.warnings)?;

            kind = HeaderKind::Choice {
                variants: choice_header,
            };
        }

        _ => {}
    }

    let mut header_tokens = FileTokens::new_with_file_id(full_name, token_stream.file_id, body);
    header_tokens.canonical_os_path = token_stream.canonical_os_path.clone();

    Ok(Header {
        kind,
        exported,
        dependencies,
        name_location,
        tokens: header_tokens,
        source_file: context.source_file.to_owned(),
        file_imports: context.file_import_entries.to_vec(),
    })
}

fn emit_header_naming_warning(
    warnings: &mut Vec<CompilerWarning>,
    identifier: &str,
    location: SourceLocation,
    naming_kind: IdentifierNamingKind,
) {
    if let Some(warning) = naming_warning_for_identifier(identifier, location, naming_kind) {
        warnings.push(warning);
    }
}

// WHAT: collects all tokens that make up a function body (`:` … `;`) into `body`,
// tracking scope depth to handle nested scopes (inner `if`/`loop`/etc.) correctly.
//
// WHY: extracted from `create_header` to reduce its length and make the scope-balancing
// contract explicit. The token stream must already be positioned on the first body token
// (i.e. `FunctionSignature::new` has already consumed the signature).
// Strict dependency edges are derived from the signature only; body tokens are captured but
// not scanned for imports — that is AST's responsibility at body-lowering time.
fn capture_function_body_tokens(
    token_stream: &mut FileTokens,
    body: &mut Vec<Token>,
) -> Result<(), CompilerError> {
    let mut scopes_opened = 1;
    let mut scopes_closed = 0;

    // `FunctionSignature::new` stops on the first body token, so the first loop
    // iteration must inspect the current token before advancing.
    while scopes_opened > scopes_closed {
        match token_stream.current_token_kind() {
            TokenKind::End => {
                scopes_closed += 1;
                if scopes_opened > scopes_closed {
                    body.push(token_stream.current_token());
                }
            }

            // Colons used in templates parse into a different token (StartTemplateBody),
            // so there is no risk of templates creating a colon imbalance here.
            // All other language constructs follow the invariant: every `:` is closed by `;`.
            TokenKind::Colon => {
                scopes_opened += 1;
                body.push(token_stream.current_token());
            }

            // `::` is an expression/operator token (e.g. `Choice::Variant`) and must not
            // affect function-scope depth balancing.
            TokenKind::DoubleColon => {
                body.push(token_stream.current_token());
            }

            TokenKind::Eof => {
                return_rule_error!(
                    "Unexpected end of file while parsing function body. Missing ';' to close this scope.",
                    token_stream.current_location(),
                    {
                        PrimarySuggestion => "Close the function body with ';'",
                        SuggestedInsertion => ";",
                    }
                )
            }

            _ => {
                body.push(token_stream.current_token());
            }
        }

        token_stream.advance();
    }

    Ok(())
}
fn create_constant_header_payload(
    full_name: &InternedPath,
    token_stream: &mut FileTokens,
    context: &mut HeaderBuildContext<'_>,
    dependencies: &mut HashSet<InternedPath>,
) -> Result<DeclarationSyntax, CompilerError> {
    let Some(declaration_name) = full_name.name() else {
        return Err(CompilerError::compiler_error(
            "Constant header path is missing its declaration name.",
        ));
    };
    let declaration_syntax =
        parse_declaration_syntax(token_stream, declaration_name, context.string_table)?;

    // Strict edges: declared type annotation only.
    // WHY: initializer-expression symbols are soft ordering hints, not strict structural deps.
    collect_constant_type_dependencies(&declaration_syntax, context, dependencies);

    *context.file_constant_order += 1;

    Ok(declaration_syntax)
}

fn discover_visible_constant_placeholders(
    token_stream: &FileTokens,
    string_table: &mut StringTable,
) -> Result<Rc<TopLevelDeclarationIndex>, CompilerError> {
    let mut placeholders = Vec::new();
    let mut seen_paths = HashSet::new();
    let mut next_statement_exported = false;
    let mut scope_depth = 0usize;
    let tokens = &token_stream.tokens;

    let mut index = 0usize;
    while index < tokens.len() {
        if scope_depth == 0 && matches!(tokens[index].kind, TokenKind::Import) {
            let (paths, next_index) = parse_import_clause_tokens(tokens, index)?;
            for path in paths {
                let normalized =
                    normalize_import_dependency_path(&path, &token_stream.src_path, string_table)?;
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

    Ok(Rc::new(TopLevelDeclarationIndex::new(placeholders)))
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
        value: Expression::no_value(location, DataType::Inferred, Ownership::ImmutableOwned),
    }
}

/// Collect strict dependency edges from a constant's declared type annotation.
///
/// WHY: only the declared type creates a structural ordering constraint; initializer-expression
/// symbol references are soft hints that are intentionally excluded from strict graph edges.
fn collect_constant_type_dependencies(
    declaration_syntax: &DeclarationSyntax,
    context: &HeaderBuildContext<'_>,
    dependencies: &mut HashSet<InternedPath>,
) {
    for_each_named_type_in_data_type(&declaration_syntax.type_annotation, &mut |type_name| {
        collect_named_type_dependency_edge(
            type_name,
            context.file_imports,
            context.source_file,
            context.string_table,
            dependencies,
        );
    });
}

fn collect_named_type_dependency_edge(
    type_name: StringId,
    file_imports: &HashSet<InternedPath>,
    source_file: &InternedPath,
    string_table: &StringTable,
    dependencies: &mut HashSet<InternedPath>,
) {
    if is_reserved_builtin_symbol(string_table.resolve(type_name)) {
        return;
    }

    let edge = file_imports
        .iter()
        .find(|import_path| import_path.name() == Some(type_name))
        .cloned()
        .unwrap_or_else(|| source_file.append(type_name));
    dependencies.insert(edge);
}

fn create_top_level_const_template(
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

    consume_balanced_template_region(
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

    Ok(Header {
        kind: HeaderKind::ConstTemplate,
        exported: true,
        dependencies,
        name_location,
        tokens: template_tokens,
        source_file: context.source_file.to_owned(),
        file_imports: context.file_import_entries.to_vec(),
    })
}

fn push_runtime_template_tokens_to_start_function(
    opening_template_token: Token,
    token_stream: &mut FileTokens,
    start_function_body: &mut Vec<Token>,
) -> Result<(), CompilerError> {
    start_function_body.push(opening_template_token);

    consume_balanced_template_region(
        token_stream,
        |token, _token_kind| {
            start_function_body.push(token);
        },
        |location| {
            CompilerError::new_rule_error(
                "Unexpected end of file while parsing top-level runtime template. Missing ']' to close the template.",
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
    })
}

#[cfg(test)]
#[path = "tests/parse_file_headers_tests.rs"]
mod parse_file_headers_tests;
