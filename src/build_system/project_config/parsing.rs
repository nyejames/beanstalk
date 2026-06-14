//! Tokenization, header parsing, dependency sorting, and AST construction for Stage 0 project
//! config files.
//!
//! WHAT: loads `#config.bst` and any reachable builder/core source-library files through the normal
//! frontend pipeline up to AST, then hands the folded AST off to config validation.
//! WHY: config uses normal Beanstalk syntax, so reusing tokenizer → headers → dependency sort →
//! AST keeps Stage 0 aligned with the rest of the language and lets config values benefit from
//! constant folding and type checking, including imported library constants.

use crate::build_system::create_project_modules::extract_source_code;
use crate::build_system::create_project_modules::import_scanning::extract_import_paths;
use crate::build_system::project_config::ProjectConfigParseServices;

use crate::compiler_frontend::ast::{Ast, AstBuildContext, AstBuildInput};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages, SourceLocation};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticKind, InvalidConfigReason, RuleDiagnosticKind,
};
use crate::compiler_frontend::headers::parse_file_headers::{
    FileFrontendPrepareOutput, Header, HeaderKind, HeaderParseOptions, Headers, parse_headers,
    prepare_file_from_tokens,
};
use crate::compiler_frontend::module_dependencies::resolve_module_dependencies;
use crate::compiler_frontend::paths::import_resolution::ImportPathResolutionError;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::{ImportRootPolicy, ProjectPathResolver};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{Token, TokenKind, TokenizerEntryMode};
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;
use crate::projects::settings::DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS;

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

pub(super) struct ParsedConfigFile {
    pub(super) ast: Ast,
    pub(super) errors: Vec<CompilerDiagnostic>,
    /// The source identity used when tokenizing the authored `#config.bst` file.
    ///
    /// WHY: validation must distinguish declarations authored in config from imported support
    /// declarations so only authored declarations are treated as config keys.
    pub(super) authored_config_path: PathBuf,
}

// -------------------------
//  Config Parsing Entry
// -------------------------

/// Parse `#config.bst` through tokenizer → headers → dependency sort → AST.
///
/// WHY: value validation happens later, but the pipeline must surface all structural errors before
/// Stage 0 tries to apply any settings.
pub(super) fn parse_config_file(
    config_path: &Path,
    services: &ProjectConfigParseServices<'_>,
    string_table: &mut StringTable,
) -> Result<ParsedConfigFile, CompilerMessages> {
    let mut errors = Vec::new();

    let canonical_config = std::fs::canonicalize(config_path).map_err(|error| {
        CompilerMessages::from_error(
            CompilerError::file_error(
                config_path,
                format!("Failed to canonicalize config path: {error}"),
                string_table,
            ),
            string_table.clone(),
        )
    })?;

    // -------------------------
    //  Config Path Resolver
    // -------------------------
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let canonical_dir =
        std::fs::canonicalize(config_dir).unwrap_or_else(|_| config_dir.to_path_buf());

    let project_path_resolver = match ProjectPathResolver::new(
        canonical_dir.clone(),
        canonical_dir,
        &services.libraries.source_libraries,
        &services.libraries.source_file_kinds,
    ) {
        Ok(resolver) => resolver
            .with_import_root_policy(ImportRootPolicy::SourceLibrariesAndExternalPackagesOnly),
        Err(error) => {
            return Err(CompilerMessages::from_error(error, string_table.clone()));
        }
    };

    // -------------------------
    //  Build Config Source Set
    // -------------------------
    let source_set = build_config_source_set(
        &canonical_config,
        services,
        &project_path_resolver,
        &mut errors,
        string_table,
    )?;

    // -------------------------
    //  Tokenize and Prepare All Files
    // -------------------------
    let mut prepared_outputs = Vec::with_capacity(source_set.len());

    for file_path in &source_set {
        // Preserve the original non-canonicalized path for the authored config file's source
        // location scope so diagnostics match the path the caller provided.
        let logical_scope_path;
        let scope_path = if file_path == &canonical_config {
            config_path
        } else {
            logical_scope_path = project_path_resolver
                .logical_path_for_canonical_file(file_path, string_table)
                .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;
            logical_scope_path.as_path()
        };

        let output = match prepare_one_config_file(
            file_path,
            scope_path,
            &canonical_config,
            services,
            &mut errors,
            string_table,
        )? {
            Some(output) => output,
            None => continue,
        };

        prepared_outputs.push(output);
    }

    if !errors.is_empty() {
        return Err(CompilerMessages::from_diagnostics(
            errors,
            string_table.clone(),
        ));
    }

    // -------------------------
    //  Header Aggregation
    // -------------------------
    let bag_result = parse_headers(
        prepared_outputs,
        &services.libraries.external_packages,
        &ExternalImportResolutionTable::default(),
        Some(&project_path_resolver),
        string_table,
    );

    let parsed_headers = match bag_result {
        Ok(headers) => headers,
        Err(bag) => {
            for diagnostic in bag.diagnostics() {
                if is_authored_config_duplicate(diagnostic, config_path, string_table) {
                    errors.push(config_diagnostic(
                        None,
                        InvalidConfigReason::DuplicateKey,
                        diagnostic.primary_location.clone(),
                    ));
                } else {
                    errors.push(diagnostic.clone());
                }
            }
            return Err(CompilerMessages::from_diagnostics(
                errors,
                string_table.clone(),
            ));
        }
    };

    // -------------------------
    //  Dependency Sorting
    // -------------------------
    let headers_for_sort = Headers {
        headers: parsed_headers.headers,
        top_level_const_fragments: parsed_headers.top_level_const_fragments,
        entry_runtime_fragment_count: parsed_headers.entry_runtime_fragment_count,
        module_symbols: parsed_headers.module_symbols,
        import_environment: parsed_headers.import_environment,
    };

    let sorted = match resolve_module_dependencies(headers_for_sort, string_table) {
        Ok(sorted) => sorted,
        Err(bag) => {
            errors.extend(bag.into_diagnostics());
            return Err(CompilerMessages::from_diagnostics(
                errors,
                string_table.clone(),
            ));
        }
    };

    // -------------------------
    //  AST Construction
    // -------------------------
    let interned_path = InternedPath::from_path_buf(config_path, string_table);

    let ast = Ast::new(
        AstBuildInput {
            headers: sorted.headers,
            module_symbols: sorted.module_symbols,
            import_environment: sorted.import_environment,
            top_level_const_fragments: sorted.top_level_const_fragments,
        },
        AstBuildContext {
            external_package_registry: &services.libraries.external_packages,
            style_directives: services.style_directives,
            string_table,
            entry_dir: interned_path,
            build_profile: crate::compiler_frontend::FrontendBuildProfile::Dev,
            project_path_resolver: Some(project_path_resolver),
            path_format_config: PathStringFormatConfig::default(),
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        },
    )?;

    Ok(ParsedConfigFile {
        ast,
        errors,
        authored_config_path: config_path.to_path_buf(),
    })
}

// -------------------------
//  Config Source Set
// -------------------------

/// Build the set of source files that config parsing must compile.
///
/// WHAT: starts from the authored `#config.bst` and BFS-follows imports into builder/core
/// source-library files only. External package imports are tracked but do not add files.
/// WHY: config expressions may reference imported library constants, so those files must be
/// parsed and folded, but project-local files and relative imports are rejected by policy.
fn build_config_source_set(
    canonical_config: &Path,
    services: &ProjectConfigParseServices<'_>,
    project_path_resolver: &ProjectPathResolver,
    errors: &mut Vec<CompilerDiagnostic>,
    string_table: &mut StringTable,
) -> Result<Vec<PathBuf>, CompilerMessages> {
    let mut visited: BTreeSet<PathBuf> = BTreeSet::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    let mut source_set: Vec<PathBuf> = Vec::new();

    queue.push_back(canonical_config.to_path_buf());

    while let Some(file_path) = queue.pop_front() {
        let canonical_file = match std::fs::canonicalize(&file_path) {
            Ok(path) => path,
            Err(error) => {
                return Err(CompilerMessages::from_error(
                    CompilerError::file_error(
                        &file_path,
                        format!("Failed to canonicalize config source path: {error}"),
                        string_table,
                    ),
                    string_table.clone(),
                ));
            }
        };

        if !visited.insert(canonical_file.clone()) {
            continue;
        }

        source_set.push(canonical_file.clone());

        let import_paths =
            match extract_import_paths(&canonical_file, services.style_directives, string_table) {
                Ok(paths) => paths,
                Err(_error) => {
                    // Tokenization/import extraction errors for this file will be reported during
                    // `prepare_one_config_file` instead, so skip duplicates here.
                    continue;
                }
            };

        for import_path in &import_paths {
            // Virtual package imports (e.g. @core/math) are allowed and need no file discovery.
            if services
                .libraries
                .external_packages
                .is_virtual_package_import(import_path, string_table)
            {
                continue;
            }

            let resolved = match project_path_resolver
                .resolve_import_to_source_file_with_facade_fallback(
                    import_path,
                    &canonical_file,
                    string_table,
                ) {
                Ok(resolved) => resolved.path,
                Err(ImportPathResolutionError::Diagnostic(diagnostic)) => {
                    errors.push(diagnostic);
                    continue;
                }
                Err(ImportPathResolutionError::Infrastructure(error)) => {
                    return Err(CompilerMessages::from_error(error, string_table.clone()));
                }
            };

            if !visited.contains(&resolved) {
                queue.push_back(resolved);
            }
        }
    }

    Ok(source_set)
}

// -------------------------
//  Per-File Preparation
// -------------------------

/// Tokenize and header-parse one file that belongs to the config source set.
///
/// WHAT: runs the same per-file preparation as normal module compilation, but applies
/// config-specific token validation only to the authored config file.
fn prepare_one_config_file(
    file_path: &Path,
    scope_path: &Path,
    canonical_config: &Path,
    services: &ProjectConfigParseServices<'_>,
    errors: &mut Vec<CompilerDiagnostic>,
    string_table: &mut StringTable,
) -> Result<Option<FileFrontendPrepareOutput>, CompilerMessages> {
    let source = extract_source_code(file_path, string_table)
        .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;
    let interned_path = InternedPath::from_path_buf(scope_path, string_table);

    let mut token_stream = match tokenize(
        &source,
        &interned_path,
        TokenizerEntryMode::SourceFile,
        services.style_directives,
        string_table,
        None,
    ) {
        Ok(tokens) => tokens,
        Err(error) => {
            errors.push(error);
            return Ok(None);
        }
    };
    token_stream.canonical_os_path = Some(file_path.to_path_buf());

    // Only validate hash assignments for the authored config file.
    let is_authored_config = file_path == canonical_config;
    if is_authored_config {
        errors.extend(validate_config_hash_assignments(&token_stream.tokens));
    }

    // Only the authored config file should be treated as the entry file.
    // Imported library files must be non-entry so top-level runtime statements are rejected.
    let entry_file_path = if is_authored_config {
        scope_path
    } else {
        canonical_config
    };

    let output = match prepare_file_from_tokens(
        token_stream,
        entry_file_path,
        &HeaderParseOptions::default(),
        &services.libraries.external_packages,
        string_table,
        0,
        0,
    ) {
        Ok(output) => output,
        Err(error) => {
            errors.extend(error.warnings);
            if is_authored_config && is_duplicate_config_header_error(&error.diagnostic) {
                errors.push(config_diagnostic(
                    None,
                    InvalidConfigReason::DuplicateKey,
                    error.diagnostic.primary_location.clone(),
                ));
            } else {
                errors.push(*error.diagnostic);
            }
            return Ok(None);
        }
    };

    // Only validate config structural restrictions for the authored config file.
    // Imported library files may contain functions, types, and other support surfaces.
    if is_authored_config {
        errors.extend(validate_authored_config_surface(&output.headers));
    }

    Ok(Some(output))
}

// -------------------------
//  Structural Validation
// -------------------------

/// Reject unsupported surfaces in the authored `#config.bst` file after header parsing has
/// normalized declaration shapes.
///
/// WHY: Stage 0 config uses frontend parsing for expression semantics, but config is not a normal
/// module. It is compile-time-only, so runtime declarations such as functions and standalone
/// templates are rejected before AST. Type aliases, structs, and choices are allowed as support
/// declarations because they can be referenced by compile-time constant expressions.
/// Imports are allowed when they pass the config import-root policy, so they are not rejected
/// here. Start-body validation happens later through `validation.rs` and AST const facts.
fn validate_authored_config_surface(headers: &[Header]) -> Vec<CompilerDiagnostic> {
    let mut errors = Vec::new();

    for header in headers {
        let reason = match &header.kind {
            HeaderKind::Function { .. } => Some(InvalidConfigReason::FunctionUnsupported),
            HeaderKind::ConstTemplate { .. } => {
                Some(InvalidConfigReason::StandaloneTemplateUnsupported)
            }
            HeaderKind::Constant { .. }
            | HeaderKind::StartFunction
            | HeaderKind::Struct { .. }
            | HeaderKind::Choice { .. }
            | HeaderKind::TypeAlias { .. }
            | HeaderKind::Trait { .. }
            | HeaderKind::TraitConformance { .. }
            | HeaderKind::TraitIncompatibility { .. } => None,
        };

        if let Some(reason) = reason {
            errors.push(config_diagnostic(
                header.tokens.src_path.name(),
                reason,
                header.name_location.clone(),
            ));
        }
    }

    errors
}

// -------------------------
//  Shorthand Validation
// -------------------------

fn is_duplicate_config_header_error(diagnostic: &CompilerDiagnostic) -> bool {
    matches!(
        diagnostic.kind,
        DiagnosticKind::Rule(RuleDiagnosticKind::DuplicateDeclaration)
    )
}

fn is_authored_config_duplicate(
    diagnostic: &CompilerDiagnostic,
    authored_config_path: &Path,
    string_table: &StringTable,
) -> bool {
    if !is_duplicate_config_header_error(diagnostic) {
        return false;
    }

    let diagnostic_path = diagnostic.primary_location.scope.to_path_buf(string_table);
    paths_match(&diagnostic_path, authored_config_path)
}

/// Compare source paths exactly first, then by canonical filesystem identity.
///
/// WHY: authored config diagnostics preserve the caller-provided path, while other
/// config source-set paths may already be canonicalized or library-logical.
fn paths_match(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }

    let left_canonical = std::fs::canonicalize(left);
    let right_canonical = std::fs::canonicalize(right);
    matches!(
        (left_canonical, right_canonical),
        (Ok(left_path), Ok(right_path)) if left_path == right_path
    )
}

/// Validate that all config declarations use standard constant syntax (`key #= value`).
///
/// WHY: the old shorthand `#key value` was removed. Collecting all violations at once lets users
/// fix them in a single iteration rather than one error at a time.
fn validate_config_hash_assignments(tokens: &[Token]) -> Vec<CompilerDiagnostic> {
    let mut errors = Vec::new();
    let mut index = 0usize;

    while index < tokens.len() {
        if !matches!(tokens[index].kind, TokenKind::Hash) {
            index += 1;
            continue;
        }

        index += 1;
        skip_newlines(tokens, &mut index);

        let Some(name_token) = tokens.get(index) else {
            break;
        };
        let TokenKind::Symbol(name_id) = name_token.kind else {
            continue;
        };

        index += 1;
        skip_newlines(tokens, &mut index);

        let Some(next_token) = tokens.get(index) else {
            break;
        };

        // Current binding-mode syntax is left to the normal frontend parser. This
        // check only reports the older removed shorthand where a scalar value
        // followed `#name` without a declaration operator.
        if matches!(
            next_token.kind,
            TokenKind::Assign | TokenKind::DoubleColon | TokenKind::TypeParameterBracket
        ) {
            continue;
        }

        // Scalar-like tokens immediately after `#name` indicate the removed shorthand form.
        if matches!(
            next_token.kind,
            TokenKind::StringSliceLiteral(_)
                | TokenKind::RawStringLiteral(_)
                | TokenKind::Symbol(_)
                | TokenKind::NumericLiteral(_)
                | TokenKind::BoolLiteral(_)
                | TokenKind::Path(_)
                | TokenKind::OpenCurly
        ) {
            errors.push(config_diagnostic(
                Some(name_id),
                InvalidConfigReason::ShorthandDeclaration,
                next_token.location.clone(),
            ));
        }
    }

    errors
}

fn config_diagnostic(
    key: Option<StringId>,
    reason: InvalidConfigReason,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::invalid_config_reason(key, reason, location)
}

fn skip_newlines(tokens: &[Token], index: &mut usize) {
    while let Some(token) = tokens.get(*index) {
        if !matches!(token.kind, TokenKind::Newline) {
            break;
        }
        *index += 1;
    }
}
