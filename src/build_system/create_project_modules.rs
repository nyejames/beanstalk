// Core build functionality shared across all project types
//
// Contains the common compilation pipeline steps that are used by all project builders.
// This now only compiles the HIR and runs the borrow checker.
// This is because both a Wasm and JS backend must be supported, so it is agnostic about what happens after that.

use crate::backends::function_registry::HostRegistry;
use crate::build_system::build::{InputFile, Module};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages, ErrorType};
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind, parse_headers};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokenizer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, Token, TokenKind, TokenizeMode};
use crate::compiler_frontend::{CompilerFrontend, Flag};
use crate::projects::settings;
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};
use crate::{borrow_log, return_err_as_messages, return_file_error, timer_log};
use std::collections::{BTreeSet, HashSet, VecDeque};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// External function import required by the compiled WASM
#[derive(Debug, Clone)]
pub struct ExternalImport {
    /// Module name (e.g., "env", "beanstalk_io", "host")
    pub module: String,
    /// Function name
    pub function: String,
    /// Function signature for validation
    pub signature: FunctionSignature,
    /// Whether this is a built-in compiler_frontend function or user-defined import
    pub import_type: ImportType,
}

/// Function signature for external imports
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Parameter types
    pub params: Vec<WasmType>,
    /// Return types
    pub returns: Vec<WasmType>,
}

/// Type of external import
#[derive(Debug, Clone)]
pub enum ImportType {
    /// Built-in compiler_frontend library function (IO, memory management, etc.)
    BuiltIn(BuiltInFunction),
    /// User-defined external function from the host environment
    External,
}

/// Built-in compiler_frontend functions that the runtime must provide
#[derive(Debug, Clone)]
pub enum BuiltInFunction {
    /// IO operations
    Print,
    ReadInput,
    WriteFile,
    ReadFile,
    /// Memory management
    Malloc,
    Free,
    /// Environment access
    GetEnv,
    SetEnv,
    /// System operations
    Exit,
}

/// WASM value types for function signatures
#[derive(Debug, Clone)]
pub enum WasmType {
    I32,
    I64,
    F32,
    F64,
}

struct DiscoveredModule {
    entry_point: PathBuf,
    input_files: Vec<InputFile>,
}

#[derive(Debug, Clone)]
struct ImportPathSpec {
    components: Vec<String>,
    display: String,
}

/// Find and compile all modules in the project.
/// This function is agnostic for all projects,
/// every builder will use it. It defines the structure of all Beanstalk projects
pub fn compile_project_frontend(
    config: &mut Config,
    _flags: &[Flag],
) -> Result<Vec<Module>, CompilerMessages> {
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
                let library_roots = resolve_library_roots(config, &source_root);
                let reachable_files = match discover_reachable_files(
                    &entry_path,
                    &source_root,
                    &library_roots,
                    &source_root,
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

                let module = compile_module(input_files, config, &entry_path)?;
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

    // --------------------
    //   PARSE THE CONFIG
    // --------------------
    let config_path = config.entry_dir.join(settings::CONFIG_FILE_NAME);
    if config_path.exists() {
        if let Err(error) = parse_project_config_file(config, &config_path) {
            return_err_as_messages!(error);
        }
    }

    // -------------------------------------
    //  DISCOVER ALL MODULES IN THE PROJECT
    // -------------------------------------
    // Root module entries are #*.bst files (excluding #config.bst).
    // Each entry compiles as its own frontend module with reachable-only inputs.
    let discovered_modules = match discover_all_modules_in_project(config) {
        Ok(modules) => modules,
        Err(error) => return_err_as_messages!(error),
    };

    let mut compiled_modules = Vec::with_capacity(discovered_modules.len());
    for discovered in discovered_modules {
        let module = compile_module(discovered.input_files, config, &discovered.entry_point)?;
        compiled_modules.push(module);
    }

    Ok(compiled_modules)
}

/// Perform the core compilation pipeline shared by all project types
pub fn compile_module(
    module: Vec<InputFile>,
    config: &Config,
    entry_file_path: &Path,
) -> Result<Module, CompilerMessages> {
    // Module capacity heuristic
    // Just a guess of how many strings we might need to intern per file
    const FILE_MIN_UNIQUE_SYMBOLS_CAPACITY: usize = 16;

    // Create a new string table for interning strings
    let string_table = StringTable::with_capacity(module.len() * FILE_MIN_UNIQUE_SYMBOLS_CAPACITY);

    // Create the compiler_frontend instance
    let mut compiler = CompilerFrontend::new(config, string_table);

    let time = Instant::now();

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

    timer_log!(time, "Tokenized in: ");

    // ----------------------------------
    //           Parse Headers
    // ----------------------------------
    // This will parse all the top level declarations across the token_stream
    // This is to split up the AST generation into discreet blocks and make all the public declarations known during AST generation.
    // All imports are figured out at this stage, so each header can be ordered depending on their dependencies.
    let time = Instant::now();

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

    timer_log!(time, "Headers Parsed in: ");

    // ----------------------------------
    //       Dependency resolution
    // ----------------------------------
    let time = Instant::now();
    let sorted_modules = match compiler.sort_headers(module_headers.headers) {
        Ok(modules) => modules,
        Err(error) => {
            compiler_messages.errors.extend(error);
            return Err(compiler_messages);
        }
    };

    timer_log!(time, "Dependency graph created in: ");

    // ----------------------------------
    //          AST generation
    // ----------------------------------
    let time = Instant::now();
    // Combine all headers into one AST for this module.
    let module_ast = match compiler.headers_to_ast(
        sorted_modules,
        module_headers.top_level_template_items,
        entry_file_path,
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

    timer_log!(time, "AST created in: ");

    // ----------------------------------
    //          HIR generation
    // ----------------------------------
    let time = Instant::now();

    let hir_module = match compiler.generate_hir(module_ast) {
        Ok(nodes) => nodes,
        Err(e) => {
            compiler_messages.errors.extend(e.errors);
            compiler_messages.warnings.extend(e.warnings);
            return Err(compiler_messages);
        }
    };

    timer_log!(time, "HIR generated in: ");

    // ----------------------------------
    //          BORROW CHECKING
    // ----------------------------------
    let time = Instant::now();

    let borrow_analysis = match compiler.check_borrows(&hir_module) {
        Ok(outcome) => outcome,
        Err(e) => {
            compiler_messages.errors.extend(e.errors);
            compiler_messages.warnings.extend(e.warnings);
            return Err(compiler_messages);
        }
    };

    timer_log!(time, "Borrow checking completed in: ");

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
        folder_name: entry_file_path
            .file_name()
            .unwrap_or(OsStr::new(""))
            .to_str()
            .unwrap_or("")
            .to_string(),
        entry_point: entry_file_path.to_path_buf(),
        hir: hir_module,
        borrow_analysis,
        required_module_imports: Vec::new(), //TODO: parse imports for external modules and add to requirements list
        exported_functions: Vec::new(), //TODO: Get the list of exported functions from the AST (with their signatures)
        warnings: compiler_messages.warnings,
        string_table: compiler.string_table,
    })
}

fn discover_all_modules_in_project(
    config: &Config,
) -> Result<Vec<DiscoveredModule>, CompilerError> {
    let source_root = resolve_project_source_root(config);
    if !source_root.exists() {
        return_file_error!(
            &source_root,
            format!(
                "Configured source root '{}' does not exist",
                source_root.display()
            ),
            {
                CompilationStage => "Project Structure",
                PrimarySuggestion => "Set '#src' in #config.bst to an existing directory",
            }
        );
    }

    let entry_points = discover_root_entry_files(&source_root)?;
    if entry_points.is_empty() {
        return_file_error!(
            &source_root,
            "No root module entries were found. Expected at least one '#*.bst' file under the source directory.",
            {
                CompilationStage => "Project Structure",
                PrimarySuggestion => "Add at least one entry file like '#page.bst' under the configured source directory",
            }
        );
    }

    let library_roots = resolve_library_roots(config, &source_root);

    let mut modules = Vec::with_capacity(entry_points.len());
    for entry_point in entry_points {
        let reachable_files = discover_reachable_files(
            &entry_point,
            &source_root,
            &library_roots,
            &config.entry_dir,
        )?;

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

fn resolve_project_source_root(config: &Config) -> PathBuf {
    if config.src.as_os_str().is_empty() {
        return config.entry_dir.clone();
    }

    if config.src.is_absolute() {
        config.src.clone()
    } else {
        config.entry_dir.join(&config.src)
    }
}

fn resolve_library_roots(config: &Config, source_root: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::with_capacity(config.libraries.len());
    for library in &config.libraries {
        if library.as_os_str().is_empty() {
            continue;
        }

        if library.is_absolute() {
            roots.push(library.clone());
            continue;
        }

        let from_source = source_root.join(library);
        if from_source.exists() {
            roots.push(from_source);
        } else {
            roots.push(config.entry_dir.join(library));
        }
    }
    roots
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

            if path.extension() != Some(OsStr::new(BEANSTALK_FILE_EXTENSION)) {
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
    source_root: &Path,
    library_roots: &[PathBuf],
    project_root: &Path,
) -> Result<Vec<PathBuf>, CompilerError> {
    let source_root = fs::canonicalize(source_root).map_err(|error| {
        CompilerError::file_error(
            source_root,
            format!("Failed to canonicalize configured source root: {error}"),
        )
    })?;

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

        let import_paths = extract_import_paths(&canonical_file)?;
        for import_path in import_paths {
            let resolved = resolve_import_to_file(
                &import_path,
                &canonical_file,
                &source_root,
                library_roots,
                project_root,
            )?;
            if !reachable.contains(&resolved) {
                queue.push_back(resolved);
            }
        }
    }

    Ok(reachable.into_iter().collect())
}

fn extract_import_paths(file_path: &Path) -> Result<Vec<ImportPathSpec>, CompilerError> {
    let source = extract_source_code(file_path)?;
    let mut string_table = StringTable::new();
    let interned_path = InternedPath::from_path_buf(file_path, &mut string_table);
    let tokens = tokenize(
        &source,
        &interned_path,
        TokenizeMode::Normal,
        &mut string_table,
    )
    .map_err(|error| error.with_file_path(file_path.to_path_buf()))?;

    let mut imports = Vec::new();
    let mut index = 0usize;
    while index < tokens.tokens.len() {
        if !matches!(tokens.tokens[index].kind, TokenKind::Import) {
            index += 1;
            continue;
        }

        index += 1;
        while index < tokens.tokens.len() && matches!(tokens.tokens[index].kind, TokenKind::Newline)
        {
            index += 1;
        }

        let Some(token) = tokens.tokens.get(index) else {
            return Err(CompilerError::file_error(
                file_path,
                "Import statement ended unexpectedly without a path.",
            ));
        };

        let TokenKind::Path(paths) = &token.kind else {
            return Err(CompilerError::new(
                "Expected a path token after 'import'.",
                token.location.to_error_location(&string_table),
                ErrorType::Syntax,
            ));
        };

        for path in paths {
            let components = path
                .components()
                .map(|component| string_table.resolve(component).to_string())
                .collect::<Vec<_>>();
            let display = if components.is_empty() {
                String::from("<empty>")
            } else {
                components.join("/")
            };

            imports.push(ImportPathSpec {
                components,
                display,
            });
        }
        index += 1;
    }

    Ok(imports)
}

fn resolve_import_to_file(
    import_path: &ImportPathSpec,
    importer_file: &Path,
    source_root: &Path,
    library_roots: &[PathBuf],
    project_root: &Path,
) -> Result<PathBuf, CompilerError> {
    let importer_dir = importer_file.parent().ok_or_else(|| {
        CompilerError::file_error(
            importer_file,
            "Could not determine parent directory for importing file.",
        )
    })?;

    if import_path.components.is_empty() {
        return_file_error!(
            importer_file,
            "Import path cannot be empty",
            {
                CompilationStage => "Project Structure",
                PrimarySuggestion => "Use a path like @(folder/file) after import",
            }
        );
    }

    let is_relative = matches!(
        import_path.components.first().map(String::as_str),
        Some(".") | Some("..")
    );

    let mut search_roots = Vec::new();
    if is_relative {
        search_roots.push(importer_dir.to_path_buf());
    } else {
        search_roots.push(source_root.to_path_buf());
        search_roots.extend(library_roots.iter().cloned());
    }

    for root in search_roots {
        for candidate in candidate_import_files(&root, &import_path.components) {
            if candidate.is_file() {
                return fs::canonicalize(candidate).map_err(|error| {
                    CompilerError::file_error(
                        importer_file,
                        format!(
                            "Failed to canonicalize resolved import from '{}': {error}",
                            import_path.display
                        ),
                    )
                });
            }
        }
    }

    Err(CompilerError::file_error(
        importer_file,
        format!(
            "Could not resolve import '{}'. Tried source root '{}' and configured library roots.",
            import_path.display,
            source_root
                .strip_prefix(project_root)
                .unwrap_or(source_root)
                .display(),
        ),
    ))
}

fn candidate_import_files(root: &Path, components: &[String]) -> Vec<PathBuf> {
    let mut candidates = Vec::with_capacity(2);

    let mut exact = root.to_path_buf();
    for component in components {
        exact.push(component);
    }
    candidates.push(with_bst_extension(exact));

    if components.len() > 1 {
        let mut parent = root.to_path_buf();
        for component in &components[..components.len() - 1] {
            parent.push(component);
        }
        candidates.push(with_bst_extension(parent));
    }

    candidates
}

fn with_bst_extension(path: PathBuf) -> PathBuf {
    if path.extension() == Some(OsStr::new(BEANSTALK_FILE_EXTENSION)) {
        path
    } else {
        path.with_extension(BEANSTALK_FILE_EXTENSION)
    }
}

// `#config.bst` follows regular Beanstalk syntax. Stage 0 only extracts top-level
// constant headers from it and maps the values that builders care about.
fn parse_project_config_file(config: &mut Config, config_path: &Path) -> Result<(), CompilerError> {
    let source = extract_source_code(config_path)?;
    let mut string_table = StringTable::new();
    let interned_path = InternedPath::from_path_buf(config_path, &mut string_table);
    let token_stream = tokenize(
        &source,
        &interned_path,
        TokenizeMode::Normal,
        &mut string_table,
    )
    .map_err(|error| error.with_file_path(config_path.to_path_buf()))?;

    // Explicitly reject legacy config assignment shorthand (`#key value`).
    validate_config_hash_assignments(&token_stream.tokens, &string_table)?;

    let host_registry = HostRegistry::new(&mut string_table);
    let mut warnings = Vec::new();
    let parsed_headers = parse_headers(
        vec![token_stream],
        &host_registry,
        &mut warnings,
        config_path,
        &mut string_table,
    )
    .map_err(|errors| {
        errors.into_iter().next().unwrap_or_else(|| {
            CompilerError::file_error(config_path, "Failed to parse project config headers.")
        })
    })?;

    apply_config_constants_from_headers(config, &parsed_headers.headers, &string_table, config_path)
}

fn validate_config_hash_assignments(
    tokens: &[Token],
    string_table: &StringTable,
) -> Result<(), CompilerError> {
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
            return Err(CompilerError::new(
                format!(
                    "Invalid config declaration '#{name} ...'. Use standard constant syntax: '#{name} = value'."
                ),
                next_token.location.to_error_location(string_table),
                ErrorType::Config,
            ));
        }
    }

    Ok(())
}

fn apply_config_constants_from_headers(
    config: &mut Config,
    headers: &[Header],
    string_table: &StringTable,
    config_path: &Path,
) -> Result<(), CompilerError> {
    for header in headers {
        let HeaderKind::Constant { metadata } = &header.kind else {
            continue;
        };

        let Some(key_id) = header.tokens.src_path.name() else {
            return Err(CompilerError::compiler_error(
                "Config constant header is missing a symbol name.",
            ));
        };
        let key = string_table.resolve(key_id).to_string();

        let mut initializer_tokens = metadata.declaration_syntax.initializer_tokens.clone();
        initializer_tokens.push(Token::new(TokenKind::Eof, header.name_location.to_owned()));
        let mut value_index = 0usize;
        skip_newlines(&initializer_tokens, &mut value_index);

        if key == "libraries" {
            config.libraries = parse_libraries_value(
                &initializer_tokens,
                &mut value_index,
                string_table,
                config_path,
            )?;
            continue;
        }

        let Some(value_token) = initializer_tokens.get(value_index) else {
            return Err(CompilerError::new(
                format!("Missing value for config constant '#{key}'."),
                header.name_location.to_error_location(string_table),
                ErrorType::Config,
            ));
        };
        let Some(value) = parse_config_scalar_value(&value_token.kind, string_table) else {
            return Err(CompilerError::new(
                format!("Unsupported value for config constant '#{key}'."),
                value_token.location.to_error_location(string_table),
                ErrorType::Config,
            ));
        };

        apply_config_entry(config, &key, &value);
    }

    Ok(())
}

fn parse_libraries_value(
    tokens: &[Token],
    index: &mut usize,
    string_table: &StringTable,
    config_path: &Path,
) -> Result<Vec<PathBuf>, CompilerError> {
    let mut libraries = Vec::new();
    let Some(start_token) = tokens.get(*index) else {
        return Ok(libraries);
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
                        libraries.push(PathBuf::from(path.to_string(string_table)));
                    }
                }
                TokenKind::StringSliceLiteral(value) | TokenKind::RawStringLiteral(value) => {
                    libraries.push(PathBuf::from(string_table.resolve(*value)));
                }
                TokenKind::Symbol(value) => {
                    libraries.push(PathBuf::from(string_table.resolve(*value)));
                }
                TokenKind::Comma | TokenKind::Newline => {}
                TokenKind::Eof => {
                    return Err(CompilerError::new(
                        "Unterminated '#libraries' block. Missing closing '}'.",
                        token.location.to_error_location(string_table),
                        ErrorType::Config,
                    ));
                }
                _ => {
                    return Err(CompilerError::new(
                        "Unsupported value in '#libraries' block.",
                        token.location.to_error_location(string_table),
                        ErrorType::Config,
                    ));
                }
            }
            *index += 1;
        }
        dedupe_paths(&mut libraries);
        return Ok(libraries);
    }

    match &start_token.kind {
        TokenKind::Path(paths) => {
            for path in paths {
                libraries.push(PathBuf::from(path.to_string(string_table)));
            }
        }
        TokenKind::StringSliceLiteral(value) | TokenKind::RawStringLiteral(value) => {
            libraries.push(PathBuf::from(string_table.resolve(*value)));
        }
        TokenKind::Symbol(value) => {
            libraries.push(PathBuf::from(string_table.resolve(*value)));
        }
        _ => {
            return Err(CompilerError::new(
                "Unsupported '#libraries' value. Use a path, string, or '{ ... }' block.",
                start_token.location.to_error_location(string_table),
                ErrorType::Config,
            ));
        }
    }

    if libraries.is_empty() {
        return Err(CompilerError::file_error(
            config_path,
            "Expected at least one library path in '#libraries'.",
        ));
    }

    *index += 1;
    dedupe_paths(&mut libraries);
    Ok(libraries)
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
        "src" => config.src = PathBuf::from(value),
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
                    CompilationStage => "File System",
                    PrimarySuggestion => suggestion,
                }
            )
        }
    }
}

#[cfg(test)]
#[path = "tests/create_project_modules_tests.rs"]
mod create_project_modules_tests;
