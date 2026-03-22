// Core build functionality shared across all project types
//
// Contains the common compilation pipeline steps that are used by all project builders.
// This now only compiles the HIR and runs the borrow checker.
// This is because both a Wasm and JS backend must be supported, so it is agnostic about what happens after that.

use crate::build_system::build::{InputFile, Module};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages, ErrorMetaDataKey, ErrorType};
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind, parse_headers};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::{StyleDirectiveRegistry, StyleDirectiveSpec};
use crate::compiler_frontend::tokenizer::paths::collect_import_paths_from_tokens;
use crate::compiler_frontend::tokenizer::tokenizer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, Token, TokenKind, TokenizeMode};
use crate::compiler_frontend::{CompilerFrontend, Flag, FrontendBuildProfile};
use crate::projects::path_resolution::{ProjectPathResolver, resolve_project_entry_root};
use crate::projects::settings;
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};
use crate::{borrow_log, return_err_as_messages, return_file_error, timer_log};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

struct DiscoveredModule {
    entry_point: PathBuf,
    input_files: Vec<InputFile>,
}

struct ParsedImportPaths {
    paths: Vec<InternedPath>,
    string_table: StringTable,
}

/// Find and compile all modules in the project.
/// This function is agnostic for all projects,
/// every builder will use it. It defines the structure of all Beanstalk projects
pub fn compile_project_frontend(
    config: &mut Config,
    flags: &[Flag],
    frontend_style_directives: &[StyleDirectiveSpec],
) -> Result<Vec<Module>, CompilerMessages> {
    let style_directives = StyleDirectiveRegistry::merged(frontend_style_directives);

    let build_profile = if flags.contains(&Flag::Release) {
        FrontendBuildProfile::Release
    } else {
        FrontendBuildProfile::Dev
    };

    // -----------------------------
    //    SINGLE FILE COMPILATION
    // -----------------------------
    // If the entry is a file (not a directory),
    // compile and output that single file.
    if let Some(extension) = config.entry_dir.extension() {
        match extension.to_str().unwrap_or_default() {
            BEANSTALK_FILE_EXTENSION => {
                let entry_path = match fs::canonicalize(&config.entry_dir) {
                    Ok(path) => path,
                    Err(error) => {
                        let file_error = CompilerError::file_error(
                            &config.entry_dir,
                            format!("Failed to resolve entry file path: {error}"),
                        );
                        return_err_as_messages!(file_error);
                    }
                };

                let source_root = match entry_path.parent() {
                    Some(parent) => parent.to_path_buf(),
                    None => PathBuf::from("."),
                };
                let project_path_resolver = match ProjectPathResolver::new(
                    source_root.clone(),
                    source_root.clone(),
                    &config.root_folders,
                ) {
                    Ok(resolver) => resolver,
                    Err(error) => return_err_as_messages!(error),
                };
                let reachable_files = match discover_reachable_files(
                    &entry_path,
                    &project_path_resolver,
                    &style_directives,
                ) {
                    Ok(files) => files,
                    Err(error) => return_err_as_messages!(error),
                };

                let mut input_files = Vec::with_capacity(reachable_files.len());
                for source_path in reachable_files {
                    let source_code = match extract_source_code(&source_path) {
                        Ok(code) => code,
                        Err(error) => return_err_as_messages!(error),
                    };

                    input_files.push(InputFile {
                        source_code,
                        source_path,
                    });
                }

                let module = compile_module(
                    input_files,
                    config,
                    &entry_path,
                    build_profile,
                    Some(project_path_resolver.clone()),
                    &style_directives,
                )?;
                return Ok(vec![module]);
            }

            _ => {
                let err = CompilerError::file_error(
                    &config.entry_dir,
                    format!(
                        "Unsupported file extension for compilation. Beanstalk files use .{BEANSTALK_FILE_EXTENSION}"
                    ),
                );
                return_err_as_messages!(err);
            }
        }
    }

    // Guard clause to make sure the entry is a directory.
    // Could be a file without an extension, which would be weird.
    if !config.entry_dir.is_dir() {
        let err = CompilerError::file_error(
            &config.entry_dir,
            format!(
                "Found a file without an extension set. Beanstalk files use .{BEANSTALK_FILE_EXTENSION}"
            ),
        );
        return_err_as_messages!(err);
    }

    // -------------------------------------
    //  DISCOVER ALL MODULES IN THE PROJECT
    // -------------------------------------
    // Root module entries are #*.bst files (excluding #config.bst).
    // Each entry compiles as its own frontend module with reachable-only inputs.
    let project_root = match fs::canonicalize(&config.entry_dir) {
        Ok(path) => path,
        Err(error) => {
            let file_error = CompilerError::file_error(
                &config.entry_dir,
                format!("Failed to canonicalize project root: {error}"),
            );
            return_err_as_messages!(file_error);
        }
    };
    let entry_root = resolve_project_entry_root(config);
    if !entry_root.exists() {
        let file_error = CompilerError::file_error(
            &entry_root,
            format!(
                "Configured entry root '{}' does not exist",
                entry_root.display()
            ),
        );
        return_err_as_messages!(file_error);
    }
    let entry_root = match fs::canonicalize(&entry_root) {
        Ok(path) => path,
        Err(error) => {
            let file_error = CompilerError::file_error(
                &entry_root,
                format!("Failed to canonicalize configured entry root: {error}"),
            );
            return_err_as_messages!(file_error);
        }
    };
    let project_path_resolver =
        match ProjectPathResolver::new(project_root, entry_root, &config.root_folders) {
            Ok(resolver) => resolver,
            Err(error) => return_err_as_messages!(error),
        };

    let discovered_modules =
        match discover_all_modules_in_project(config, &project_path_resolver, &style_directives) {
            Ok(modules) => modules,
            Err(error) => return_err_as_messages!(error),
        };

    let mut compiled_modules = Vec::with_capacity(discovered_modules.len());
    for discovered in discovered_modules {
        let module = compile_module(
            discovered.input_files,
            config,
            &discovered.entry_point,
            build_profile,
            Some(project_path_resolver.clone()),
            &style_directives,
        )?;
        compiled_modules.push(module);
    }

    Ok(compiled_modules)
}

/// Perform the core compilation pipeline shared by all project types
pub fn compile_module(
    module: Vec<InputFile>,
    config: &Config,
    entry_file_path: &Path,
    build_profile: FrontendBuildProfile,
    project_path_resolver: Option<ProjectPathResolver>,
    style_directives: &StyleDirectiveRegistry,
) -> Result<Module, CompilerMessages> {
    // Module capacity heuristic
    // Just a guess of how many strings we might need to intern per file
    const FILE_MIN_UNIQUE_SYMBOLS_CAPACITY: usize = 16;

    // Create a new string table for interning strings
    let string_table = StringTable::with_capacity(module.len() * FILE_MIN_UNIQUE_SYMBOLS_CAPACITY);

    // Create the compiler_frontend instance
    let mut compiler = CompilerFrontend::new(
        config,
        string_table,
        style_directives.to_owned(),
        project_path_resolver,
    );

    let _time = Instant::now();

    // ----------------------------------
    //         Token generation
    // ----------------------------------
    let tokenizer_result: Vec<Result<FileTokens, CompilerError>> = module
        .iter()
        .map(|module| {
            compiler.source_to_tokens(
                &module.source_code,
                &module.source_path,
                TokenizeMode::Normal,
            )
        })
        .collect();

    // Check for any errors first
    let mut project_tokens = Vec::with_capacity(tokenizer_result.len());
    let mut compiler_messages = CompilerMessages::new();
    for file in tokenizer_result {
        match file {
            Ok(tokens) => {
                project_tokens.push(tokens);
            }
            Err(e) => {
                compiler_messages.errors.push(e);
            }
        }
    }

    if !compiler_messages.errors.is_empty() {
        return Err(compiler_messages);
    }

    timer_log!(_time, "Tokenized in: ");

    // ----------------------------------
    //           Parse Headers
    // ----------------------------------
    // This will parse all the top level declarations across the token_stream
    // This is to split up the AST generation into discreet blocks and make all the public declarations known during AST generation.
    // All imports are figured out at this stage, so each header can be ordered depending on their dependencies.
    let _time = Instant::now();

    let module_headers = match compiler.tokens_to_headers(
        project_tokens,
        &mut compiler_messages.warnings,
        entry_file_path,
    ) {
        Ok(headers) => headers,
        Err(e) => {
            compiler_messages.errors.extend(e);
            return Err(compiler_messages);
        }
    };

    timer_log!(_time, "Headers Parsed in: ");

    // ----------------------------------
    //       Dependency resolution
    // ----------------------------------
    let _time = Instant::now();
    let sorted_modules = match compiler.sort_headers(module_headers.headers) {
        Ok(modules) => modules,
        Err(error) => {
            compiler_messages.errors.extend(error);
            return Err(compiler_messages);
        }
    };

    timer_log!(_time, "Dependency graph created in: ");

    // ----------------------------------
    //          AST generation
    // ----------------------------------
    let _time = Instant::now();
    // Combine all headers into one AST for this module.
    let module_ast = match compiler.headers_to_ast(
        sorted_modules,
        module_headers.top_level_template_items,
        entry_file_path,
        build_profile,
    ) {
        Ok(parser_output) => {
            compiler_messages
                .warnings
                .extend(parser_output.warnings.clone());
            parser_output
        }
        Err(e) => {
            compiler_messages.errors.extend(e.errors);
            return Err(compiler_messages);
        }
    };

    timer_log!(_time, "AST created in: ");

    // ----------------------------------
    //          HIR generation
    // ----------------------------------
    let _time = Instant::now();

    let hir_module = match compiler.generate_hir(module_ast) {
        Ok(nodes) => nodes,
        Err(e) => {
            compiler_messages.errors.extend(e.errors);
            compiler_messages.warnings.extend(e.warnings);
            return Err(compiler_messages);
        }
    };

    timer_log!(_time, "HIR generated in: ");

    // ----------------------------------
    //          BORROW CHECKING
    // ----------------------------------
    let _time = Instant::now();

    let borrow_analysis = match compiler.check_borrows(&hir_module) {
        Ok(outcome) => outcome,
        Err(e) => {
            compiler_messages.errors.extend(e.errors);
            compiler_messages.warnings.extend(e.warnings);
            return Err(compiler_messages);
        }
    };

    timer_log!(_time, "Borrow checking completed in: ");

    // Debug output for the borrow checker (macro-gated by `show_borrow_checker`)
    borrow_log!("=== BORROW CHECKER OUTPUT ===");
    borrow_log!(format!(
        "Borrow checking completed successfully (states={} functions={} blocks={} conflicts_checked={} stmt_facts={} term_facts={} value_facts={})",
        borrow_analysis.analysis.total_state_snapshots(),
        borrow_analysis.stats.functions_analyzed,
        borrow_analysis.stats.blocks_analyzed,
        borrow_analysis.stats.conflicts_checked,
        borrow_analysis.analysis.statement_facts.len(),
        borrow_analysis.analysis.terminator_facts.len(),
        borrow_analysis.analysis.value_facts.len()
    ));
    borrow_log!("=== END BORROW CHECKER OUTPUT ===");

    Ok(Module {
        entry_point: entry_file_path.to_path_buf(),
        hir: hir_module,
        borrow_analysis,
        warnings: compiler_messages.warnings,
        string_table: compiler.string_table,
    })
}

fn discover_all_modules_in_project(
    config: &Config,
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
) -> Result<Vec<DiscoveredModule>, CompilerError> {
    let source_root = resolve_project_entry_root(config);
    if !source_root.exists() {
        return_file_error!(
            &source_root,
            format!(
                "Configured entry root '{}' does not exist",
                source_root.display()
            ),
            {
                CompilationStage => String::from("Project Structure"),
                PrimarySuggestion => String::from("Set '#entry_root' in #config.bst to an existing directory"),
            }
        );
    }

    project_path_resolver.validate_entry_root_collisions()?;

    let entry_points = discover_root_entry_files(project_path_resolver.entry_root())?;
    if entry_points.is_empty() {
        return_file_error!(
            project_path_resolver.entry_root(),
            "No root module entries were found. Expected at least one '#*.bst' file under the configured entry root.",
            {
                CompilationStage => String::from("Project Structure"),
                PrimarySuggestion => String::from("Add at least one entry file like '#page.bst' under the configured entry root"),
            }
        );
    }

    let mut modules = Vec::with_capacity(entry_points.len());
    for entry_point in entry_points {
        let reachable_files =
            discover_reachable_files(&entry_point, project_path_resolver, style_directives)?;

        let mut input_files = Vec::with_capacity(reachable_files.len());
        for source_path in reachable_files {
            input_files.push(InputFile {
                source_code: extract_source_code(&source_path)?,
                source_path,
            });
        }

        modules.push(DiscoveredModule {
            entry_point,
            input_files,
        });
    }

    Ok(modules)
}

fn discover_root_entry_files(source_root: &Path) -> Result<Vec<PathBuf>, CompilerError> {
    let mut discovered = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back(source_root.to_path_buf());

    while let Some(dir) = queue.pop_front() {
        let entries = fs::read_dir(&dir).map_err(|error| {
            CompilerError::file_error(
                &dir,
                format!("Failed to read directory while discovering modules: {error}"),
            )
        })?;

        for entry in entries {
            let entry = entry.map_err(|error| {
                CompilerError::file_error(
                    &dir,
                    format!("Failed to read directory entry while discovering modules: {error}"),
                )
            })?;
            let path = entry.path();

            if path.is_dir() {
                queue.push_back(path);
                continue;
            }

            if path.extension().and_then(|extension| extension.to_str())
                != Some(BEANSTALK_FILE_EXTENSION)
            {
                continue;
            }

            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            if !file_name.starts_with('#') || file_name == settings::CONFIG_FILE_NAME {
                continue;
            }

            discovered.push(fs::canonicalize(&path).map_err(|error| {
                CompilerError::file_error(
                    &path,
                    format!("Failed to canonicalize module entry path: {error}"),
                )
            })?);
        }
    }

    discovered.sort();
    Ok(discovered)
}

fn discover_reachable_files(
    entry_point: &Path,
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
) -> Result<Vec<PathBuf>, CompilerError> {
    let mut reachable = BTreeSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(entry_point.to_path_buf());

    while let Some(next_file) = queue.pop_front() {
        let canonical_file = fs::canonicalize(&next_file).map_err(|error| {
            CompilerError::file_error(
                &next_file,
                format!("Failed to canonicalize module file path: {error}"),
            )
        })?;

        if !reachable.insert(canonical_file.clone()) {
            continue;
        }

        let import_paths = extract_import_paths(&canonical_file, style_directives)?;
        for import_path in &import_paths.paths {
            let resolved = project_path_resolver.resolve_import_to_file(
                import_path,
                &canonical_file,
                &import_paths.string_table,
            )?;
            if !reachable.contains(&resolved) {
                queue.push_back(resolved);
            }
        }
    }

    Ok(reachable.into_iter().collect())
}

fn extract_import_paths(
    file_path: &Path,
    style_directives: &StyleDirectiveRegistry,
) -> Result<ParsedImportPaths, CompilerError> {
    let source = extract_source_code(file_path)?;
    let mut string_table = StringTable::new();
    let interned_path = InternedPath::from_path_buf(file_path, &mut string_table);
    let tokens = tokenize(
        &source,
        &interned_path,
        TokenizeMode::Normal,
        style_directives,
        &mut string_table,
    )
    .map_err(|error| error.with_file_path(file_path.to_path_buf()))?;

    let imports = collect_import_paths_from_tokens(&tokens.tokens, &string_table)
        .map_err(|error| error.with_file_path(file_path.to_path_buf()))?;

    Ok(ParsedImportPaths {
        paths: imports,
        string_table,
    })
}
/// WHAT: loads and validates project config from #config.bst before compilation begins.
/// WHY: config must be validated early so backends can reject invalid settings before any work.
///
/// This function is part of Stage 0 (project structure discovery) and executes before
/// frontend compilation. It checks if a config file exists and delegates to
/// parse_project_config_file if present. Missing config files are allowed (returns Ok).
pub fn load_project_config(config: &mut Config) -> Result<(), CompilerMessages> {
    let config_path = config.entry_dir.join(settings::CONFIG_FILE_NAME);
    
    if !config_path.exists() {
        return Ok(()); // Config file is optional
    }
    
    parse_project_config_file(config, &config_path)
}

/// WHAT: parses #config.bst and extracts top-level constant declarations into the Config struct.
/// WHY: config follows regular Beanstalk syntax; Stage 0 extracts constant headers and validates them.
///
/// Error handling: tokenization and header parsing errors are collected and returned together.
/// Value-level validation (apply_config_constants_from_headers) uses first-error semantics.
fn parse_project_config_file(config: &mut Config, config_path: &Path) -> Result<(), CompilerMessages> {
    let mut errors = Vec::new();
    
    let source = extract_source_code(config_path).map_err(compiler_messages_from_error)?;
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let interned_path = InternedPath::from_path_buf(config_path, &mut string_table);
    
    // Tokenization errors are fatal
    let token_stream = match tokenize(
        &source,
        &interned_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    ) {
        Ok(tokens) => tokens,
        Err(error) => {
            errors.push(error.with_file_path(config_path.to_path_buf()));
            return Err(CompilerMessages { errors, warnings: Vec::new() });
        }
    };

    // Check for deprecated '#key value' syntax (should be '#key = value')
    let legacy_errors = validate_config_hash_assignments(&token_stream.tokens, &string_table);
    errors.extend(legacy_errors);

    let host_registry = HostRegistry::new(&mut string_table);
    let mut warnings = Vec::new();
    
    // Header parsing - for config files, we handle duplicate detection separately
    let parsed_headers = match parse_headers(
        vec![token_stream],
        &host_registry,
        &mut warnings,
        config_path,
        &mut string_table,
    ) {
        Ok(headers) => headers,
        Err(header_errors) => {
            // Preserve all header parsing errors
            // Convert duplicate constant errors from ErrorType::Rule to ErrorType::Config
            // for consistency with config file error reporting
            for error in header_errors {
                if error.msg.contains("already a constant") || error.msg.contains("shadow") {
                    // Convert Rule error to Config error for config files
                    // Use a clearer message for config context while preserving location and metadata
                    let mut config_error = error.clone();
                    config_error.error_type = ErrorType::Config;
                    config_error.msg = "Duplicate config key found. Each config key must be unique.".to_string();
                    errors.push(config_error);
                } else {
                    // Preserve all other header errors as-is
                    errors.push(error);
                }
            }
            return Err(CompilerMessages { errors, warnings: Vec::new() });
        }
    };

    // Detect duplicate config keys with Config error type
    // This handles cases where parse_headers succeeded but there are still duplicates
    if let Some(duplicate_errors) = detect_duplicate_config_keys(&parsed_headers.headers, &string_table) {
        errors.extend(duplicate_errors);
    }

    // Apply config values - collect all value-level errors
    if let Err(config_errors) = apply_config_constants_from_headers(config, &parsed_headers.headers, &string_table, config_path) {
        errors.extend(config_errors);
    }

    if !errors.is_empty() {
        return Err(CompilerMessages { errors, warnings: Vec::new() });
    }

    Ok(())
}

/// WHAT: validates that config uses standard constant syntax, not legacy shorthand.
/// WHY: collect all legacy syntax violations so users can fix them in one iteration.
fn validate_config_hash_assignments(
    tokens: &[Token],
    string_table: &StringTable,
) -> Vec<CompilerError> {
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

        // Regular declarations can follow `#name` with `=`, `|`, `::`, etc.
        if matches!(
            next_token.kind,
            TokenKind::Assign | TokenKind::DoubleColon | TokenKind::TypeParameterBracket
        ) {
            continue;
        }

        // Scalar-like tokens after `#name` are the old config syntax (`#key value`).
        if matches!(
            next_token.kind,
            TokenKind::StringSliceLiteral(_)
                | TokenKind::RawStringLiteral(_)
                | TokenKind::Symbol(_)
                | TokenKind::IntLiteral(_)
                | TokenKind::FloatLiteral(_)
                | TokenKind::BoolLiteral(_)
                | TokenKind::Path(_)
                | TokenKind::OpenCurly
        ) {
            let name = string_table.resolve(name_id);
            let mut error = CompilerError::new(
                format!(
                    "Invalid config declaration '#{name} ...'. Use standard constant syntax: '#{name} = value'."
                ),
                next_token.location.to_error_location(string_table),
                ErrorType::Config,
            );
            error.metadata.insert(
                crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
                format!("Add '=' between '#{name}' and the value")
            );
            errors.push(error);
        }
    }

    errors
}

/// WHAT: extracts config key-value pairs from parsed headers and stores them in the Config struct.
/// WHY: config constants must be applied to the Config struct with precise location tracking
///      for accurate error reporting; deprecated keys are detected and rejected here.
///      All errors are collected to enable multi-error reporting.
fn apply_config_constants_from_headers(
    config: &mut Config,
    headers: &[Header],
    string_table: &StringTable,
    config_path: &Path,
) -> Result<(), Vec<CompilerError>> {
    let mut errors = Vec::new();

    for header in headers {
        let HeaderKind::Constant { metadata } = &header.kind else {
            continue;
        };

        // Extract the config key name from the header
        let Some(key_id) = header.tokens.src_path.name() else {
            errors.push(CompilerError::compiler_error(
                "Config constant header is missing a symbol name.",
            ));
            continue;
        };
        let key = string_table.resolve(key_id).to_string();

        // Location tracking: store the source location for this config key for precise error reporting
        let location = header.name_location.to_error_location(string_table);
        config.setting_locations.insert(key.clone(), location);

        let mut initializer_tokens = metadata.declaration_syntax.initializer_tokens.clone();
        initializer_tokens.push(Token::new(TokenKind::Eof, header.name_location.to_owned()));
        let mut value_index = 0usize;
        skip_newlines(&initializer_tokens, &mut value_index);

        // Deprecated key handling: collect errors for old config keys with helpful migration messages
        if key == "libraries" {
            let mut error = CompilerError::new(
                "Config key '#libraries' has been replaced. Use '#root_folders' instead.",
                header.name_location.to_error_location(string_table),
                ErrorType::Config,
            );
            error.metadata.insert(
                crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
                "Rename '#libraries' to '#root_folders' in your config file".to_string()
            );
            errors.push(error);
            continue;
        }

        // Special handling for '#root_folders' which accepts multiple values
        if key == "root_folders" {
            match parse_root_folders_value(
                &initializer_tokens,
                &mut value_index,
                string_table,
                config_path,
            ) {
                Ok(root_folders) => config.root_folders = root_folders,
                Err(folder_errors) => errors.extend(folder_errors),
            }
            continue;
        }

        let Some(value_token) = initializer_tokens.get(value_index) else {
            let mut error = CompilerError::new(
                format!("Missing value for config constant '#{key}'."),
                header.name_location.to_error_location(string_table),
                ErrorType::Config,
            );
            error.metadata.insert(
                crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
                format!("Add a value after '#{key} =' (e.g., '#{key} = \"value\"')")
            );
            errors.push(error);
            continue;
        };
        let Some(value) = parse_config_scalar_value(&value_token.kind, string_table) else {
            let mut error = CompilerError::new(
                format!("Unsupported value for config constant '#{key}'."),
                value_token.location.to_error_location(string_table),
                ErrorType::Config,
            );
            error.metadata.insert(
                crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
                "Config values must be strings, numbers, booleans, or paths".to_string()
            );
            errors.push(error);
            continue;
        };

        // Deprecated key handling: '#src' was renamed to '#entry_root'
        if key == "src" {
            let mut error = CompilerError::new(
                "Config key '#src' is deprecated. Use '#entry_root' instead.",
                header.name_location.to_error_location(string_table),
                ErrorType::Config,
            );
            error.metadata.insert(
                crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
                "Rename '#src' to '#entry_root' in your config file".to_string()
            );
            errors.push(error);
            continue;
        }

        apply_config_entry(config, &key, &value);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn parse_root_folders_value(
    tokens: &[Token],
    index: &mut usize,
    string_table: &StringTable,
    config_path: &Path,
) -> Result<Vec<PathBuf>, Vec<CompilerError>> {
    let mut root_folders = Vec::new();
    let mut errors = Vec::new();
    
    let Some(start_token) = tokens.get(*index) else {
        return Ok(root_folders);
    };

    if matches!(start_token.kind, TokenKind::OpenCurly) {
        *index += 1;
        while let Some(token) = tokens.get(*index) {
            match &token.kind {
                TokenKind::CloseCurly => {
                    *index += 1;
                    break;
                }
                TokenKind::Path(paths) => {
                    for path in paths {
                        match validate_root_folder_path(
                            PathBuf::from(path.to_string(string_table)),
                            token,
                            string_table,
                        ) {
                            Ok(validated_path) => root_folders.push(validated_path),
                            Err(error) => errors.push(error),
                        }
                    }
                }
                TokenKind::StringSliceLiteral(value) | TokenKind::RawStringLiteral(value) => {
                    match validate_root_folder_path(
                        PathBuf::from(string_table.resolve(*value)),
                        token,
                        string_table,
                    ) {
                        Ok(validated_path) => root_folders.push(validated_path),
                        Err(error) => errors.push(error),
                    }
                }
                TokenKind::Symbol(value) => {
                    match validate_root_folder_path(
                        PathBuf::from(string_table.resolve(*value)),
                        token,
                        string_table,
                    ) {
                        Ok(validated_path) => root_folders.push(validated_path),
                        Err(error) => errors.push(error),
                    }
                }
                TokenKind::Comma | TokenKind::Newline => {}
                TokenKind::Eof => {
                    let mut error = CompilerError::new(
                        "Unterminated '#root_folders' block. Missing closing '}'.",
                        token.location.to_error_location(string_table),
                        ErrorType::Config,
                    );
                    error.metadata.insert(
                        crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
                        "Add '}' to close the '#root_folders' block".to_string()
                    );
                    errors.push(error);
                    break;
                }
                _ => {
                    let mut error = CompilerError::new(
                        "Unsupported value in '#root_folders' block.",
                        token.location.to_error_location(string_table),
                        ErrorType::Config,
                    );
                    error.metadata.insert(
                        crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
                        "Use folder names like '@lib' or strings like \"@lib\"".to_string()
                    );
                    errors.push(error);
                }
            }
            *index += 1;
        }
        dedupe_paths(&mut root_folders);
        
        if !errors.is_empty() {
            return Err(errors);
        }
        return Ok(root_folders);
    }

    match &start_token.kind {
        TokenKind::Path(paths) => {
            for path in paths {
                match validate_root_folder_path(
                    PathBuf::from(path.to_string(string_table)),
                    start_token,
                    string_table,
                ) {
                    Ok(validated_path) => root_folders.push(validated_path),
                    Err(error) => errors.push(error),
                }
            }
        }
        TokenKind::StringSliceLiteral(value) | TokenKind::RawStringLiteral(value) => {
            match validate_root_folder_path(
                PathBuf::from(string_table.resolve(*value)),
                start_token,
                string_table,
            ) {
                Ok(validated_path) => root_folders.push(validated_path),
                Err(error) => errors.push(error),
            }
        }
        TokenKind::Symbol(value) => {
            match validate_root_folder_path(
                PathBuf::from(string_table.resolve(*value)),
                start_token,
                string_table,
            ) {
                Ok(validated_path) => root_folders.push(validated_path),
                Err(error) => errors.push(error),
            }
        }
        _ => {
            let mut error = CompilerError::new(
                "Unsupported '#root_folders' value. Use a path, string, or '{ ... }' block.",
                start_token.location.to_error_location(string_table),
                ErrorType::Config,
            );
            error.metadata.insert(
                crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
                "Use '#root_folders = @lib' or '#root_folders = { @lib, @utils }'".to_string()
            );
            errors.push(error);
        }
    }

    if root_folders.is_empty() && errors.is_empty() {
        errors.push(CompilerError::file_error(
            config_path,
            "Expected at least one root folder in '#root_folders'.",
        ));
    }

    *index += 1;
    dedupe_paths(&mut root_folders);
    
    if !errors.is_empty() {
        Err(errors)
    } else {
        Ok(root_folders)
    }
}

/// WHAT: validates one '#root_folders' entry and normalizes it to the stored path form.
/// WHY: only single top-level project folders are legal explicit import roots.
fn validate_root_folder_path(
    root_folder: PathBuf,
    token: &Token,
    string_table: &StringTable,
) -> Result<PathBuf, CompilerError> {
    if root_folder.as_os_str().is_empty() {
        let mut error = CompilerError::new(
            "Invalid '#root_folders' entry. Root folders cannot be empty.",
            token.location.to_error_location(string_table),
            ErrorType::Config,
        );
        error.metadata.insert(
            crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
            "Provide a folder name like '@lib' or '@utils'".to_string()
        );
        return Err(error);
    }

    if root_folder.is_absolute() {
        let mut error = CompilerError::new(
            format!(
                "Invalid '#root_folders' entry '{}'. Root folders must be relative to the project root.",
                root_folder.display()
            ),
            token.location.to_error_location(string_table),
            ErrorType::Config,
        );
        error.metadata.insert(
            crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
            "Use a relative folder name like '@lib' instead of an absolute path".to_string()
        );
        return Err(error);
    }

    let mut components = root_folder.components();
    let Some(first) = components.next() else {
        let mut error = CompilerError::new(
            "Invalid '#root_folders' entry. Root folders cannot be empty.",
            token.location.to_error_location(string_table),
            ErrorType::Config,
        );
        error.metadata.insert(
            crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
            "Provide a folder name like '@lib' or '@utils'".to_string()
        );
        return Err(error);
    };

    if !matches!(first, std::path::Component::Normal(_)) || components.next().is_some() {
        let mut error = CompilerError::new(
            format!(
                "Invalid '#root_folders' entry '{}'. Root folders must be a single top-level folder name such as '@lib'.",
                root_folder.display()
            ),
            token.location.to_error_location(string_table),
            ErrorType::Config,
        );
        error.metadata.insert(
            crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
            "Use a single folder name like '@lib', not a nested path like '@lib/utils'".to_string()
        );
        return Err(error);
    }

    Ok(root_folder)
}

fn parse_config_scalar_value(kind: &TokenKind, string_table: &StringTable) -> Option<String> {
    match kind {
        TokenKind::StringSliceLiteral(value)
        | TokenKind::RawStringLiteral(value)
        | TokenKind::Symbol(value) => Some(string_table.resolve(*value).to_string()),
        TokenKind::IntLiteral(value) => Some(value.to_string()),
        TokenKind::FloatLiteral(value) => Some(value.to_string()),
        TokenKind::BoolLiteral(value) => Some(value.to_string()),
        TokenKind::Path(paths) if paths.len() == 1 => Some(paths[0].to_string(string_table)),
        _ => None,
    }
}

fn apply_config_entry(config: &mut Config, key: &str, value: &str) {
    match key {
        "entry_root" => config.entry_root = PathBuf::from(value),
        "output_folder" => config.release_folder = PathBuf::from(value),
        "dev_folder" => config.dev_folder = PathBuf::from(value),
        "project" => {
            config
                .settings
                .insert("project".to_string(), value.to_string());
        }
        "project_name" | "name" => config.project_name = value.to_string(),
        "version" => config.version = value.to_string(),
        "author" => config.author = value.to_string(),
        "license" => config.license = value.to_string(),
        _ => {
            config.settings.insert(key.to_string(), value.to_string());
        }
    }
}

fn skip_newlines(tokens: &[Token], index: &mut usize) {
    while let Some(token) = tokens.get(*index) {
        if !matches!(token.kind, TokenKind::Newline) {
            break;
        }
        *index += 1;
    }
}

fn dedupe_paths(paths: &mut Vec<PathBuf>) {
    let mut seen = HashSet::new();
    paths.retain(|path| seen.insert(path.clone()));
}

pub fn extract_source_code(file_path: &Path) -> Result<String, CompilerError> {
    match fs::read_to_string(file_path) {
        Ok(content) => Ok(content),
        Err(e) => {
            let suggestion: &'static str = if e.kind() == std::io::ErrorKind::NotFound {
                "Check that the file exists at the specified path"
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                "Check that you have permission to read this file"
            } else {
                "Verify the file is accessible and not corrupted"
            };

            return_file_error!(
                &file_path,
                format!("Error reading file when adding new bst files to parse: {:?}", e), {
                    CompilationStage => String::from("File System"),
                    PrimarySuggestion => String::from(suggestion),
                }
            )
        }
    }
}

/// WHAT: detects duplicate config keys and returns errors for all duplicates.
/// WHY: users should see all duplicate keys at once to fix them in one iteration.
/// Config-specific duplicate detection uses ErrorType::Config for proper categorization.
fn detect_duplicate_config_keys(
    headers: &[Header],
    string_table: &StringTable,
) -> Option<Vec<CompilerError>> {
    let mut seen_keys = HashMap::new();
    let mut errors = Vec::new();
    
    for header in headers {
        let HeaderKind::Constant { .. } = &header.kind else {
            continue;
        };
        
        let Some(key_id) = header.tokens.src_path.name() else {
            continue;
        };
        
        let key = string_table.resolve(key_id);
        
        if let Some(_first_location) = seen_keys.get(key) {
            let mut metadata = HashMap::new();
            metadata.insert(
                ErrorMetaDataKey::PrimarySuggestion, 
                String::from("Remove or rename one of the duplicate keys")
            );
            
            errors.push(CompilerError {
                msg: format!("Duplicate config key '#{key}' found. Each config key must be unique."),
                location: header.name_location.to_error_location(string_table),
                error_type: ErrorType::Config,
                metadata,
            });
        } else {
            seen_keys.insert(key.to_string(), header.name_location.clone());
        }
    }
    
    if errors.is_empty() {
        None
    } else {
        Some(errors)
    }
}

fn compiler_messages_from_error(error: CompilerError) -> CompilerMessages {
    CompilerMessages {
        errors: vec![error],
        warnings: Vec::new(),
    }
}

#[cfg(test)]
#[path = "tests/create_project_modules_tests.rs"]
mod create_project_modules_tests;
