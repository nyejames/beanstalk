//! Module discovery, source loading, and frontend compilation pipeline for Beanstalk projects.
//!
//! This module owns the single-file and directory-project frontend flows: discovering entry
//! modules, collecting reachable source files, and running each module through the full frontend
//! pipeline (tokenization → headers → dependency sort → AST → HIR → borrow check).
//!
//! Stage 0 config loading lives in `project_config`. This module begins after config has been
//! applied to `Config`.

use crate::build_system::build::{InputFile, Module};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::{
    ProjectPathResolver, resolve_project_entry_root,
};
use crate::compiler_frontend::paths::paths::collect_paths_from_tokens;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::{StyleDirectiveRegistry, StyleDirectiveSpec};
use crate::compiler_frontend::tokenizer::tokenizer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenizeMode};
use crate::compiler_frontend::{CompilerFrontend, Flag, FrontendBuildProfile};
use crate::projects::settings;
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};
use crate::{borrow_log, return_err_as_messages, return_file_error, timer_log};

// Re-export so existing tests can access `parse_project_config_file` via `super::*`
#[cfg(test)]
pub(crate) use crate::build_system::project_config::parse_project_config_file;
use std::collections::{BTreeSet, VecDeque};
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

/// Compile all project modules through the frontend pipeline.
///
/// WHAT: dispatches to single-file or directory-project flow depending on the entry path.
/// WHY: separating the two flows keeps each path readable as orchestration over named steps.
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

    // Dispatch: single-file entry vs. directory project.
    if let Some(extension) = config.entry_dir.extension() {
        return compile_single_file_frontend(config, build_profile, &style_directives, extension);
    }

    if !config.entry_dir.is_dir() {
        let err = CompilerError::file_error(
            &config.entry_dir,
            format!(
                "Found a file without an extension set. Beanstalk files use .{BEANSTALK_FILE_EXTENSION}"
            ),
        );
        return_err_as_messages!(err);
    }

    compile_directory_frontend(config, build_profile, &style_directives)
}

/// Compile a single `.bst` file as its own module.
fn compile_single_file_frontend(
    config: &Config,
    build_profile: FrontendBuildProfile,
    style_directives: &StyleDirectiveRegistry,
    extension: &std::ffi::OsStr,
) -> Result<Vec<Module>, CompilerMessages> {
    match extension.to_str().unwrap_or_default() {
        BEANSTALK_FILE_EXTENSION => {}
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

    let source_root = entry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let project_path_resolver = match ProjectPathResolver::new(
        source_root.clone(),
        source_root.clone(),
        &config.root_folders,
    ) {
        Ok(resolver) => resolver,
        Err(error) => return_err_as_messages!(error),
    };

    let input_files =
        collect_reachable_input_files(&entry_path, &project_path_resolver, style_directives)?;
    let module = compile_module(
        input_files,
        config,
        &entry_path,
        build_profile,
        Some(project_path_resolver),
        style_directives,
    )?;
    Ok(vec![module])
}

/// Discover all entry modules in a directory project and compile each one.
fn compile_directory_frontend(
    config: &Config,
    build_profile: FrontendBuildProfile,
    style_directives: &StyleDirectiveRegistry,
) -> Result<Vec<Module>, CompilerMessages> {
    let project_path_resolver = build_project_path_resolver(config)?;

    let discovered_modules =
        match discover_all_modules_in_project(config, &project_path_resolver, style_directives) {
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
            style_directives,
        )?;
        compiled_modules.push(module);
    }

    Ok(compiled_modules)
}

/// Build the canonical path resolver for a directory project.
///
/// WHY: both project_root and entry_root must be canonicalized before path resolution; doing
/// this in one helper keeps the canonicalization logic in one place.
fn build_project_path_resolver(config: &Config) -> Result<ProjectPathResolver, CompilerMessages> {
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
    let entry_root_path = resolve_project_entry_root(config);
    if !entry_root_path.exists() {
        let file_error = CompilerError::file_error(
            &entry_root_path,
            format!(
                "Configured entry root '{}' does not exist",
                entry_root_path.display()
            ),
        );
        return_err_as_messages!(file_error);
    }
    let entry_root = match fs::canonicalize(&entry_root_path) {
        Ok(path) => path,
        Err(error) => {
            let file_error = CompilerError::file_error(
                &entry_root_path,
                format!("Failed to canonicalize configured entry root: {error}"),
            );
            return_err_as_messages!(file_error);
        }
    };
    match ProjectPathResolver::new(project_root, entry_root, &config.root_folders) {
        Ok(resolver) => Ok(resolver),
        Err(error) => return_err_as_messages!(error),
    }
}

/// Collect all reachable source files for a given entry point and load their content.
fn collect_reachable_input_files(
    entry_path: &Path,
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
) -> Result<Vec<InputFile>, CompilerMessages> {
    let reachable_files =
        match discover_reachable_files(entry_path, project_path_resolver, style_directives) {
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
    Ok(input_files)
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

    let imports = collect_paths_from_tokens(&tokens.tokens, &string_table)
        .map_err(|error| error.with_file_path(file_path.to_path_buf()))?;

    Ok(ParsedImportPaths {
        paths: imports,
        string_table,
    })
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

#[cfg(test)]
#[path = "tests/create_project_modules_tests.rs"]
mod create_project_modules_tests;
