//! Import-binding and constant-header resolution for AST construction.
//!
//! This module separates *file-local visibility* from *module declarations*:
//! - `declarations` keeps every declaration known in the module so lookups can resolve full paths.
//! - `visible_symbol_paths` limits what a specific source file is allowed to reference.
//! - `start_aliases` tracks bare-file imports that map to implicit start functions.

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::statements::declarations::resolve_declaration_syntax;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::headers::parse_file_headers::{FileImport, Header, HeaderKind};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Default)]
pub(crate) struct FileImportBindings {
    /// Import-visible symbols for one source file.
    /// This is a path set rather than names-only so resolution stays globally unique.
    pub(crate) visible_symbol_paths: FxHashSet<InternedPath>,
    /// Bare file imports (`@foo/bar`) bind an alias (`bar`) to that file's implicit start.
    pub(crate) start_aliases: FxHashMap<StringId, InternedPath>,
}

#[derive(Clone)]
enum ImportPathResolution {
    Missing,
    Ambiguous,
    Resolved(InternedPath),
}

pub(crate) fn resolve_file_import_bindings(
    file_imports_by_source: &FxHashMap<InternedPath, Vec<FileImport>>,
    module_file_paths: &FxHashSet<InternedPath>,
    importable_symbol_exported: &FxHashMap<InternedPath, bool>,
    declared_paths_by_file: &FxHashMap<InternedPath, FxHashSet<InternedPath>>,
    declared_names_by_file: &FxHashMap<InternedPath, FxHashSet<StringId>>,
    host_registry: &HostRegistry,
    string_table: &mut StringTable,
) -> Result<FxHashMap<InternedPath, FileImportBindings>, CompilerError> {
    let mut bindings_by_file = FxHashMap::default();
    let mut sorted_files = module_file_paths.iter().cloned().collect::<Vec<_>>();
    sorted_files.sort_by(|left, right| {
        left.to_portable_string(string_table)
            .cmp(&right.to_portable_string(string_table))
    });

    let importable_symbol_paths = importable_symbol_exported
        .keys()
        .cloned()
        .collect::<FxHashSet<_>>();

    for source_file in sorted_files {
        let mut bindings = FileImportBindings {
            visible_symbol_paths: declared_paths_by_file
                .get(&source_file)
                .cloned()
                .unwrap_or_default(),
            start_aliases: FxHashMap::default(),
        };
        let mut bound_names = declared_names_by_file
            .get(&source_file)
            .cloned()
            .unwrap_or_default();

        let imports = file_imports_by_source
            .get(&source_file)
            .cloned()
            .unwrap_or_default();

        for import in imports {
            // First, try bare-file import resolution. This is the only path that creates
            // a callable alias (file stem -> imported start function).
            match resolve_import_target_path(&import.header_path, module_file_paths, string_table) {
                ImportPathResolution::Resolved(file_path) => {
                    let Some(alias_name) =
                        file_start_alias_from_import_path(&import.header_path, string_table)
                    else {
                        return Err(CompilerError::new_rule_error(
                            "Imported file path must include a valid file name.",
                            import.location,
                        ));
                    };

                    if bound_names.contains(&alias_name) {
                        return Err(CompilerError::new_rule_error(
                            format!(
                                "Import name collision: '{}' is already declared in this file.",
                                string_table.resolve(alias_name)
                            ),
                            import.location,
                        ));
                    }

                    if host_registry
                        .get_function(string_table.resolve(alias_name))
                        .is_some()
                    {
                        return Err(CompilerError::new_rule_error(
                            format!(
                                "Import name collision: '{}' conflicts with a host function name.",
                                string_table.resolve(alias_name)
                            ),
                            import.location,
                        ));
                    }

                    let start_target = file_path.join_str(IMPLICIT_START_FUNC_NAME, string_table);
                    bindings.start_aliases.insert(alias_name, start_target);
                    bound_names.insert(alias_name);
                    continue;
                }
                ImportPathResolution::Ambiguous => {
                    return Err(CompilerError::new_rule_error(
                        format!(
                            "Ambiguous import target '{}'. Use a more specific path.",
                            import.header_path.to_portable_string(string_table)
                        ),
                        import.location,
                    ));
                }
                ImportPathResolution::Missing => {}
            }

            // Otherwise, resolve as a symbol import (single or grouped path-expanded import).
            // Symbol imports are export-only by language rule.
            match resolve_import_target_path(
                &import.header_path,
                &importable_symbol_paths,
                string_table,
            ) {
                ImportPathResolution::Resolved(symbol_path) => {
                    if !importable_symbol_exported
                        .get(&symbol_path)
                        .copied()
                        .unwrap_or(false)
                    {
                        return Err(CompilerError::new_rule_error(
                            format!(
                                "Cannot import '{}' because it is not exported. Add '#' to export it from its source file.",
                                symbol_path.to_portable_string(string_table)
                            ),
                            import.location,
                        ));
                    }

                    let Some(symbol_name) = symbol_path.name() else {
                        return Err(CompilerError::new_rule_error(
                            "Imported symbol path is missing a symbol name.",
                            import.location,
                        ));
                    };

                    if host_registry
                        .get_function(string_table.resolve(symbol_name))
                        .is_some()
                    {
                        return Err(CompilerError::new_rule_error(
                            format!(
                                "Import name collision: '{}' conflicts with a host function name.",
                                string_table.resolve(symbol_name)
                            ),
                            import.location,
                        ));
                    }

                    if bound_names.contains(&symbol_name)
                        && !bindings.visible_symbol_paths.contains(&symbol_path)
                    {
                        return Err(CompilerError::new_rule_error(
                            format!(
                                "Import name collision: '{}' is already declared in this file.",
                                string_table.resolve(symbol_name)
                            ),
                            import.location,
                        ));
                    }

                    bindings.visible_symbol_paths.insert(symbol_path.to_owned());
                    bound_names.insert(symbol_name);
                }
                ImportPathResolution::Ambiguous => {
                    return Err(CompilerError::new_rule_error(
                        format!(
                            "Ambiguous import target '{}'. Use a more specific path.",
                            import.header_path.to_portable_string(string_table)
                        ),
                        import.location,
                    ));
                }
                ImportPathResolution::Missing => {
                    return Err(CompilerError::new_rule_error(
                        format!(
                            "Missing import target '{}'. Could not resolve this dependency in the current module.",
                            import.header_path.to_portable_string(string_table)
                        ),
                        import.location,
                    ));
                }
            }
        }

        bindings_by_file.insert(source_file, bindings);
    }

    Ok(bindings_by_file)
}

/// WHAT: Carries all mutable/immutable context needed to parse one constant header.
/// WHY: Grouping these parameters keeps the resolver call sites explicit while avoiding
/// overly-wide function signatures that are harder to maintain.
pub(crate) struct ConstantHeaderParseContext<'a> {
    pub declarations: &'a [Declaration],
    pub visible_declaration_ids: &'a FxHashSet<InternedPath>,
    pub start_import_aliases: &'a FxHashMap<StringId, InternedPath>,
    pub host_registry: &'a HostRegistry,
    pub style_directives: &'a StyleDirectiveRegistry,
    pub project_path_resolver: Option<ProjectPathResolver>,
    pub path_format_config: PathStringFormatConfig,
    pub build_profile: FrontendBuildProfile,
    pub warnings: &'a mut Vec<CompilerWarning>,
    pub rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,
    pub string_table: &'a mut StringTable,
}

pub(crate) fn parse_constant_header_declaration(
    header: &Header,
    context: ConstantHeaderParseContext<'_>,
) -> Result<Declaration, CompilerError> {
    let ConstantHeaderParseContext {
        declarations,
        visible_declaration_ids,
        start_import_aliases,
        host_registry,
        style_directives,
        project_path_resolver,
        path_format_config,
        build_profile,
        warnings,
        rendered_path_usages,
        string_table,
    } = context;

    let HeaderKind::Constant { metadata } = &header.kind else {
        return Err(CompilerError::compiler_error(
            "Constant header resolver called for a non-constant header.",
        ));
    };

    let source_file_scope = header
        .tokens
        .canonical_os_path
        .as_ref()
        .map(|canonical_path| InternedPath::from_path_buf(canonical_path, string_table))
        .unwrap_or_else(|| header.source_file.to_owned());

    let context = ScopeContext::new(
        ContextKind::ConstantHeader,
        header.tokens.src_path.to_owned(),
        declarations,
        host_registry.clone(),
        vec![],
    )
    .with_style_directives(style_directives)
    .with_build_profile(build_profile)
    .with_project_path_resolver(project_path_resolver)
    .with_path_format_config(path_format_config)
    .with_rendered_path_usage_sink(rendered_path_usages)
    // Keep full module declarations for path identity, but explicitly gate what this file
    // can see to enforce import boundaries and prevent cross-file leakage.
    .with_visible_declarations(visible_declaration_ids.to_owned())
    .with_start_import_aliases(start_import_aliases.to_owned())
    .with_source_file_scope(source_file_scope);

    let declaration_result = resolve_declaration_syntax(
        metadata.declaration_syntax.clone(),
        header.tokens.src_path.to_owned(),
        &context,
        string_table,
    );
    warnings.extend(context.take_emitted_warnings());
    let declaration = declaration_result?;

    if !declaration.value.is_compile_time_constant() {
        return Err(CompilerError::new_rule_error(
            format!(
                "Constant '{}' is not compile-time resolvable. Constants may only contain compile-time values and constant references.",
                declaration.id.to_portable_string(string_table)
            ),
            header.name_location.clone(),
        ));
    }

    Ok(declaration)
}

fn resolve_import_target_path(
    requested_path: &InternedPath,
    candidates: &FxHashSet<InternedPath>,
    string_table: &StringTable,
) -> ImportPathResolution {
    let exact_matches = candidates
        .iter()
        .filter(|candidate| exact_path_matches_candidate(candidate, requested_path, string_table))
        .cloned()
        .collect::<Vec<_>>();

    match exact_matches.len() {
        1 => {
            if let Some(path) = exact_matches.into_iter().next() {
                return ImportPathResolution::Resolved(path);
            }
            return ImportPathResolution::Missing;
        }
        2.. => return ImportPathResolution::Ambiguous,
        _ => {}
    }

    let matches = candidates
        .iter()
        .filter(|candidate| {
            candidate.ends_with(requested_path)
                || suffix_matches_with_optional_bst_extension(
                    candidate,
                    requested_path,
                    string_table,
                )
        })
        .cloned()
        .collect::<Vec<_>>();

    match matches.len() {
        0 => ImportPathResolution::Missing,
        1 => matches
            .into_iter()
            .next()
            .map(ImportPathResolution::Resolved)
            .unwrap_or(ImportPathResolution::Missing),
        _ => ImportPathResolution::Ambiguous,
    }
}

fn exact_path_matches_candidate(
    candidate: &InternedPath,
    requested: &InternedPath,
    string_table: &StringTable,
) -> bool {
    components_match_with_optional_bst_extension(
        candidate.as_components(),
        requested.as_components(),
        string_table,
    )
}

fn suffix_matches_with_optional_bst_extension(
    candidate: &InternedPath,
    requested: &InternedPath,
    string_table: &StringTable,
) -> bool {
    if requested.len() > candidate.len() {
        return false;
    }

    let candidate_components = candidate.as_components();
    let requested_components = requested.as_components();
    let start_index = candidate_components.len() - requested_components.len();

    components_match_with_optional_bst_extension(
        &candidate_components[start_index..],
        requested_components,
        string_table,
    )
}

fn components_match_with_optional_bst_extension(
    candidate_components: &[StringId],
    requested_components: &[StringId],
    string_table: &StringTable,
) -> bool {
    if candidate_components.len() != requested_components.len() {
        return false;
    }

    candidate_components
        .iter()
        .zip(requested_components.iter())
        .all(|(candidate_component, requested_component)| {
            if candidate_component == requested_component {
                return true;
            }

            let candidate_str = string_table.resolve(*candidate_component);
            let requested_str = string_table.resolve(*requested_component);

            candidate_str.strip_suffix(".bst") == Some(requested_str)
                || requested_str.strip_suffix(".bst") == Some(candidate_str)
        })
}

fn file_start_alias_from_import_path(
    import_path: &InternedPath,
    string_table: &mut StringTable,
) -> Option<StringId> {
    let name = import_path.name_str(string_table)?;
    let alias = name.strip_suffix(".bst").unwrap_or(name).trim();
    if alias.is_empty() {
        return None;
    }

    Some(string_table.get_or_intern(alias.to_string()))
}
