//! Import-binding and constant-header resolution for AST construction.
//!
//! WHAT: builds per-file visibility gates and resolves constant header declarations.
//!
//! WHY: this module enforces the boundary between header parsing (Stage 2) and AST
//! lowering (Stage 4). Header parsing discovers imports and declaration shells; AST
//! resolves those imports into concrete symbol paths and validates that constants are
//! compile-time foldable.
//!
//! Virtual package imports are resolved into stable `ExternalSymbolId` values by
//! `(package_path, symbol_name)` and stored in `visible_external_symbols`. Later
//! expression and type resolution never re-resolves those names globally.
//!
//! ## Header/AST responsibility split
//!
//! *Header parsing owns:*
//! - discovering which files import which symbols
//! - parsing the syntactic shape of constant headers
//!
//! *AST owns:*
//! - resolving import paths to concrete `InternedPath` symbols
//! - validating that imported symbols are actually exported
//! - building the per-file `visible_symbol_paths` gate used during body parsing
//! - folding constant expressions and rejecting non-constant references
//!
//! Bare file imports (`@path/to/file` without an explicit symbol) are rejected: start functions
//! are build-system-only and are not importable or callable from modules.

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::declarations::resolve_declaration_syntax;
use crate::compiler_frontend::ast::templates::template::TemplateAtom;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext, TopLevelDeclarationIndex};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::ErrorMetaDataKey;
use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::external_packages::{ExternalPackageRegistry, ExternalSymbolId};
use crate::compiler_frontend::headers::parse_file_headers::{FileImport, Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Default)]
pub(crate) struct FileImportBindings {
    /// Source declarations visible from this file (including builtins).
    /// Used as an access gate for permission checks and constant deferral only;
    /// name lookup goes through `visible_source_bindings`.
    pub(crate) visible_symbol_paths: FxHashSet<InternedPath>,

    /// External package functions/types visible from this file.
    /// Populated by explicit virtual-package imports and prelude symbols.
    pub(crate) visible_external_symbols: FxHashMap<StringId, ExternalSymbolId>,

    /// Source-visible names → canonical declaration path.
    /// Includes same-file declarations and imported source symbols (aliased or not).
    pub(crate) visible_source_bindings: FxHashMap<StringId, InternedPath>,

    /// Type aliases: local visible name → canonical type alias path.
    pub(crate) visible_type_aliases: FxHashMap<StringId, InternedPath>,
}

#[derive(Clone)]
enum ImportPathResolution {
    Missing,
    Ambiguous,
    Resolved(InternedPath),
}

enum VisibleNameKind {
    SameFileDeclaration,
    SourceImport,
    TypeAliasImport,
    ExternalImport,
    PreludeExternal,
    Builtin,
}

struct VisibleNameBinding {
    kind: VisibleNameKind,
    canonical_path: Option<InternedPath>,
    external_symbol_id: Option<ExternalSymbolId>,
    location: Option<SourceLocation>,
}

struct VisibleNameRegistry {
    names: FxHashMap<StringId, VisibleNameBinding>,
}

impl VisibleNameRegistry {
    fn new() -> Self {
        Self {
            names: FxHashMap::default(),
        }
    }

    fn register(
        &mut self,
        local_name: StringId,
        binding: VisibleNameBinding,
        string_table: &StringTable,
    ) -> Result<(), CompilerError> {
        if let Some(previous) = self.names.get(&local_name) {
            if is_same_target(previous, &binding) {
                return Ok(());
            }
            return Err(report_visible_name_collision(
                local_name,
                binding.location.clone().unwrap_or_default(),
                previous,
                string_table,
            ));
        }
        self.names.insert(local_name, binding);
        Ok(())
    }
}

fn is_same_target(a: &VisibleNameBinding, b: &VisibleNameBinding) -> bool {
    if let (Some(a_path), Some(b_path)) = (&a.canonical_path, &b.canonical_path)
        && a_path == b_path
    {
        return true;
    }
    if let (Some(a_id), Some(b_id)) = (&a.external_symbol_id, &b.external_symbol_id)
        && a_id == b_id
    {
        return true;
    }
    false
}

fn report_visible_name_collision(
    local_name: StringId,
    new_location: SourceLocation,
    previous: &VisibleNameBinding,
    string_table: &StringTable,
) -> CompilerError {
    let name = string_table.resolve(local_name);
    let mut msg = format!(
        "Import name collision: '{}' is already visible in this file.",
        name
    );
    if previous.location.is_some() {
        msg.push_str(" Choose a different alias or rename the existing declaration.");
    }
    let mut error = CompilerError::new_rule_error(msg, new_location);
    error.new_metadata_entry(ErrorMetaDataKey::CompilationStage, "Import Binding".into());
    error.new_metadata_entry(ErrorMetaDataKey::ConflictType, "ImportNameCollision".into());
    error.new_metadata_entry(ErrorMetaDataKey::VariableName, name.to_owned());
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        "Use a different import alias with `as`, or rename the existing declaration.".into(),
    );
    error
}

fn check_alias_case_warning(
    import: &FileImport,
    local_name: StringId,
    symbol_name: StringId,
    string_table: &StringTable,
    warnings: &mut Vec<CompilerWarning>,
) {
    let alias_str = string_table.resolve(local_name);
    let symbol_str = string_table.resolve(symbol_name);

    let alias_first = alias_str.chars().next();
    let symbol_first = symbol_str.chars().next();

    let Some(a) = alias_first else { return };
    let Some(s) = symbol_first else { return };

    if !a.is_alphabetic() || !s.is_alphabetic() {
        return;
    }

    let alias_upper = a.is_uppercase();
    let symbol_upper = s.is_uppercase();

    if alias_upper != symbol_upper {
        let location = import
            .alias_location
            .clone()
            .unwrap_or_else(|| import.path_location.clone());
        warnings.push(CompilerWarning::new(
            &format!(
                "Import alias '{}' uses different leading-name case than imported symbol '{}'.",
                alias_str, symbol_str
            ),
            location,
            WarningKind::ImportAliasCaseMismatch,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_file_import_bindings(
    file_imports_by_source: &FxHashMap<InternedPath, Vec<FileImport>>,
    module_file_paths: &FxHashSet<InternedPath>,
    importable_symbol_exported: &FxHashMap<InternedPath, bool>,
    declared_paths_by_file: &FxHashMap<InternedPath, FxHashSet<InternedPath>>,
    type_alias_paths: &FxHashSet<InternedPath>,
    builtin_paths: &FxHashSet<InternedPath>,
    external_package_registry: &ExternalPackageRegistry,
    string_table: &mut StringTable,
) -> Result<
    (
        FxHashMap<InternedPath, FileImportBindings>,
        Vec<CompilerWarning>,
    ),
    CompilerError,
> {
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

    let mut warnings: Vec<CompilerWarning> = Vec::new();

    for source_file in sorted_files {
        let mut bindings = FileImportBindings {
            visible_symbol_paths: declared_paths_by_file
                .get(&source_file)
                .cloned()
                .unwrap_or_default(),
            visible_external_symbols: FxHashMap::default(),
            visible_source_bindings: FxHashMap::default(),
            visible_type_aliases: FxHashMap::default(),
        };

        let mut registry = VisibleNameRegistry::new();

        // Register same-file declarations.
        if let Some(declared_paths) = declared_paths_by_file.get(&source_file) {
            for path in declared_paths {
                if let Some(name) = path.name() {
                    let _ = registry.register(
                        name,
                        VisibleNameBinding {
                            kind: VisibleNameKind::SameFileDeclaration,
                            canonical_path: Some(path.to_owned()),
                            external_symbol_id: None,
                            location: None,
                        },
                        string_table,
                    );
                    bindings
                        .visible_source_bindings
                        .insert(name, path.to_owned());
                }
            }
        }

        // Pre-register prelude names so explicit imports cannot collide with them.
        // WHY: import aliases like `as io` must be rejected because prelude names are
        // already visible. Prelude symbols are NOT added to visible_external_symbols yet;
        // they are injected after all explicit imports so that user-declared/imported names
        // that shadow prelude symbols do not leave stale prelude entries in the map.
        for (prelude_name, symbol_id) in external_package_registry.prelude_symbols_by_name() {
            let symbol_name = string_table.intern(prelude_name);
            let _ = registry.register(
                symbol_name,
                VisibleNameBinding {
                    kind: VisibleNameKind::PreludeExternal,
                    canonical_path: None,
                    external_symbol_id: Some(*symbol_id),
                    location: None,
                },
                string_table,
            );
        }

        // Pre-register builtin error types so import aliases cannot shadow them.
        // WHY: builtins like `Error` are reserved language types; an import alias
        //      must not silently replace them in name lookup.
        for builtin_path in builtin_paths {
            if let Some(name) = builtin_path.name() {
                let _ = registry.register(
                    name,
                    VisibleNameBinding {
                        kind: VisibleNameKind::Builtin,
                        canonical_path: Some(builtin_path.to_owned()),
                        external_symbol_id: None,
                        location: None,
                    },
                    string_table,
                );
            }
        }

        let imports = file_imports_by_source
            .get(&source_file)
            .cloned()
            .unwrap_or_default();

        for import in imports {
            // Bare-file imports (`@path/to/file` resolving to a module file path) are rejected.
            // Start functions are build-system-only and are not importable or callable from modules.
            if let ImportPathResolution::Resolved(_) | ImportPathResolution::Ambiguous =
                resolve_import_target_path(&import.header_path, module_file_paths, string_table)
            {
                return Err(CompilerError::new_rule_error(
                    format!(
                        "Bare file import '{}' is not supported. Import specific exported symbols using '@path/to/file/symbol' instead.",
                        import.header_path.to_portable_string(string_table)
                    ),
                    import.location,
                ));
            }

            // Resolve as a symbol import (single or grouped path-expanded import).
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

                    let local_name = import.alias.unwrap_or(symbol_name);

                    let kind = if type_alias_paths.contains(&symbol_path) {
                        VisibleNameKind::TypeAliasImport
                    } else {
                        VisibleNameKind::SourceImport
                    };

                    registry.register(
                        local_name,
                        VisibleNameBinding {
                            kind,
                            canonical_path: Some(symbol_path.to_owned()),
                            external_symbol_id: None,
                            location: Some(import.location.clone()),
                        },
                        string_table,
                    )?;

                    if import.alias.is_some() {
                        check_alias_case_warning(
                            &import,
                            local_name,
                            symbol_name,
                            string_table,
                            &mut warnings,
                        );
                    }

                    if type_alias_paths.contains(&symbol_path) {
                        bindings
                            .visible_type_aliases
                            .insert(local_name, symbol_path.to_owned());
                    } else {
                        bindings.visible_symbol_paths.insert(symbol_path.to_owned());
                        bindings
                            .visible_source_bindings
                            .insert(local_name, symbol_path.to_owned());
                    }
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
                    // Try to resolve as a virtual package import before failing.
                    match resolve_virtual_package_import(
                        &import.header_path,
                        external_package_registry,
                        string_table,
                    ) {
                        VirtualPackageMatch::Found(package_path, symbol_name) => {
                            let local_name = import.alias.unwrap_or(symbol_name);

                            let symbol_name_str = string_table.resolve(symbol_name);
                            let external_symbol_id = if let Some((func_id, _)) =
                                external_package_registry
                                    .resolve_package_function(&package_path, symbol_name_str)
                            {
                                Some(ExternalSymbolId::Function(func_id))
                            } else if let Some((type_id, _)) = external_package_registry
                                .resolve_package_type(&package_path, symbol_name_str)
                            {
                                Some(ExternalSymbolId::Type(type_id))
                            } else if let Some((const_id, _)) = external_package_registry
                                .resolve_package_constant(&package_path, symbol_name_str)
                            {
                                Some(ExternalSymbolId::Constant(const_id))
                            } else {
                                None
                            };

                            registry.register(
                                local_name,
                                VisibleNameBinding {
                                    kind: VisibleNameKind::ExternalImport,
                                    canonical_path: Some(import.header_path.clone()),
                                    external_symbol_id,
                                    location: Some(import.location.clone()),
                                },
                                string_table,
                            )?;

                            if import.alias.is_some() {
                                check_alias_case_warning(
                                    &import,
                                    local_name,
                                    symbol_name,
                                    string_table,
                                    &mut warnings,
                                );
                            }

                            if let Some(symbol_id) = external_symbol_id {
                                bindings
                                    .visible_external_symbols
                                    .insert(local_name, symbol_id);
                            }
                            continue;
                        }
                        VirtualPackageMatch::PackageFoundSymbolMissing(package_path) => {
                            let symbol_name = import
                                .header_path
                                .name_str(string_table)
                                .unwrap_or("<unknown>");
                            return Err(CompilerError::new_rule_error(
                                format!(
                                    "Cannot import '{symbol_name}' from package '{package_path}': symbol not found in package."
                                ),
                                import.location,
                            ));
                        }
                        VirtualPackageMatch::NoMatch => {}
                    }

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

        // Inject prelude symbols into visible_external_symbols only for names that were
        // not shadowed by same-file declarations or explicit imports.
        // WHY: if a user declares `io = "shadow"` or imports `as io`, the prelude `io`
        // should not remain visible. The registry was pre-loaded with prelude names;
        // any name whose registry entry is still PreludeExternal was never overwritten.
        for (prelude_name, symbol_id) in external_package_registry.prelude_symbols_by_name() {
            let symbol_name = string_table.intern(prelude_name);
            if let Some(binding) = registry.names.get(&symbol_name)
                && matches!(binding.kind, VisibleNameKind::PreludeExternal)
            {
                bindings
                    .visible_external_symbols
                    .insert(symbol_name, *symbol_id);
            }
        }

        bindings_by_file.insert(source_file, bindings);
    }

    Ok((bindings_by_file, warnings))
}

/// WHAT: Carries all mutable/immutable context needed to parse one constant header.
/// WHY: Grouping these parameters keeps the resolver call sites explicit while avoiding
/// overly-wide function signatures that are harder to maintain.
pub(crate) struct ConstantHeaderParseContext<'a> {
    pub top_level_declarations: Rc<TopLevelDeclarationIndex>,
    pub visible_declaration_ids: &'a FxHashSet<InternedPath>,
    pub visible_external_symbols: &'a FxHashMap<StringId, ExternalSymbolId>,
    pub visible_source_bindings: &'a FxHashMap<StringId, InternedPath>,
    pub visible_type_aliases: &'a FxHashMap<StringId, InternedPath>,
    pub resolved_type_aliases: Rc<FxHashMap<InternedPath, DataType>>,
    pub external_package_registry: &'a ExternalPackageRegistry,
    pub style_directives: &'a StyleDirectiveRegistry,
    pub project_path_resolver: Option<ProjectPathResolver>,
    pub path_format_config: PathStringFormatConfig,
    pub build_profile: FrontendBuildProfile,
    pub warnings: &'a mut Vec<CompilerWarning>,
    pub rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,
    pub unresolved_constant_paths: &'a FxHashSet<InternedPath>,
    pub string_table: &'a mut StringTable,
}

pub(crate) fn parse_constant_header_declaration(
    header: &Header,
    context: ConstantHeaderParseContext<'_>,
) -> Result<Declaration, CompilerError> {
    let ConstantHeaderParseContext {
        top_level_declarations,
        visible_declaration_ids,
        visible_external_symbols,
        visible_source_bindings,
        visible_type_aliases,
        resolved_type_aliases,
        external_package_registry,
        style_directives,
        project_path_resolver,
        path_format_config,
        build_profile,
        warnings,
        rendered_path_usages,
        unresolved_constant_paths,
        string_table,
    } = context;

    let HeaderKind::Constant { declaration } = &header.kind else {
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
        top_level_declarations,
        external_package_registry.clone(),
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
    .with_visible_external_symbols(visible_external_symbols.to_owned())
    .with_visible_source_bindings(visible_source_bindings.to_owned())
    .with_visible_type_aliases(visible_type_aliases.to_owned())
    .with_resolved_type_aliases((*resolved_type_aliases).clone())
    .with_source_file_scope(source_file_scope);

    let declaration_result = resolve_declaration_syntax(
        declaration.clone(),
        header.tokens.src_path.to_owned(),
        &context,
        string_table,
    );
    warnings.extend(context.take_emitted_warnings());
    let declaration = declaration_result?;

    if !declaration.value.is_compile_time_constant() {
        // Check if the expression contains a reference to a visible constant that
        // hasn't been resolved yet. If so, this is a deferrable error — the fixed-point
        // loop will retry after its dependencies are resolved.
        if let Some(unresolved_path) = find_unresolved_constant_reference(
            &declaration.value,
            unresolved_constant_paths,
            visible_declaration_ids,
        ) {
            let variable_name = unresolved_path
                .name()
                .map(|name| string_table.resolve(name).to_owned())
                .unwrap_or_default();
            let mut error = CompilerError::new_rule_error(
                format!(
                    "Constant '{}' depends on '{}' which has not been resolved yet.",
                    declaration.id.to_portable_string(string_table),
                    unresolved_path.to_portable_string(string_table)
                ),
                header.name_location.clone(),
            );
            error.new_metadata_entry(ErrorMetaDataKey::VariableName, variable_name);
            return Err(error);
        }

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

/// Recursively scans an expression for references to visible declarations that are
/// still unresolved constant placeholders.
///
/// WHAT: when a constant header references another constant that hasn't been resolved
/// yet (e.g. due to cross-file or soft-dependency ordering), the expression will contain
/// a `Reference` to a `NoValue` placeholder. Detecting this allows the fixed-point loop
/// to defer the constant instead of failing permanently.
///
/// WHY: the deferred resolution mechanism relies on `ErrorMetaDataKey::VariableName` to
/// identify deferrable errors. This helper bridges the gap between "expression parsed as
/// Reference" and "variable not found" by surfacing the unresolved path name.
fn find_unresolved_constant_reference(
    expression: &Expression,
    unresolved_constant_paths: &FxHashSet<InternedPath>,
    visible_declaration_ids: &FxHashSet<InternedPath>,
) -> Option<InternedPath> {
    match &expression.kind {
        ExpressionKind::Reference(path) => {
            if visible_declaration_ids.contains(path) && unresolved_constant_paths.contains(path) {
                return Some(path.clone());
            }
            None
        }
        ExpressionKind::Template(template) => {
            for atom in &template.content.atoms {
                if let TemplateAtom::Content(segment) = atom
                    && let Some(path) = find_unresolved_constant_reference(
                        &segment.expression,
                        unresolved_constant_paths,
                        visible_declaration_ids,
                    )
                {
                    return Some(path);
                }
            }
            None
        }
        ExpressionKind::Collection(items) => {
            for item in items {
                if let Some(path) = find_unresolved_constant_reference(
                    item,
                    unresolved_constant_paths,
                    visible_declaration_ids,
                ) {
                    return Some(path);
                }
            }
            None
        }
        ExpressionKind::StructInstance(fields) | ExpressionKind::StructDefinition(fields) => {
            for field in fields {
                if let Some(path) = find_unresolved_constant_reference(
                    &field.value,
                    unresolved_constant_paths,
                    visible_declaration_ids,
                ) {
                    return Some(path);
                }
            }
            None
        }
        ExpressionKind::Range(start, end) => find_unresolved_constant_reference(
            start,
            unresolved_constant_paths,
            visible_declaration_ids,
        )
        .or_else(|| {
            find_unresolved_constant_reference(
                end,
                unresolved_constant_paths,
                visible_declaration_ids,
            )
        }),
        ExpressionKind::BuiltinCast { value, .. }
        | ExpressionKind::ResultConstruct { value, .. }
        | ExpressionKind::Coerced { value, .. } => find_unresolved_constant_reference(
            value,
            unresolved_constant_paths,
            visible_declaration_ids,
        ),
        _ => None,
    }
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

enum VirtualPackageMatch {
    Found(String, StringId),
    PackageFoundSymbolMissing(String),
    NoMatch,
}

/// Attempts to resolve an import path as a virtual package symbol.
///
/// WHAT: checks whether the import path matches `package/path/symbol` where `package/path`
/// is a known virtual package in the builder-provided registry.
/// WHY: virtual package imports share the same `@`-prefixed path syntax as file imports,
/// so they are distinguished at resolution time rather than tokenization time.
fn resolve_virtual_package_import(
    requested_path: &InternedPath,
    registry: &ExternalPackageRegistry,
    string_table: &StringTable,
) -> VirtualPackageMatch {
    let components = requested_path.as_components();
    if components.is_empty() {
        return VirtualPackageMatch::NoMatch;
    }

    // Build candidate package paths by joining progressively more components.
    // For @core/io/io we try "@core/io/io", "@core/io", "@core".
    for package_len in (1..=components.len()).rev() {
        let package_components = &components[..package_len];
        let package_path = format!(
            "@{}",
            package_components
                .iter()
                .map(|&id| string_table.resolve(id))
                .collect::<Vec<_>>()
                .join("/")
        );

        if !registry.has_package(&package_path) {
            continue;
        }

        // The remaining components are the symbol path within the package.
        // For now, we only support a single symbol name after the package path.
        let symbol_components = &components[package_len..];
        if symbol_components.len() != 1 {
            // Multi-component symbol paths within packages are not supported yet.
            return VirtualPackageMatch::PackageFoundSymbolMissing(package_path);
        }

        let symbol_name = symbol_components[0];
        let symbol_name_str = string_table.resolve(symbol_name);
        if registry
            .resolve_package_symbol(&package_path, symbol_name_str)
            .is_some()
            || registry
                .resolve_package_type(&package_path, symbol_name_str)
                .is_some()
        {
            return VirtualPackageMatch::Found(package_path, symbol_name);
        }

        // Package exists but symbol doesn't — stop searching shorter prefixes
        // so we report the missing symbol accurately.
        return VirtualPackageMatch::PackageFoundSymbolMissing(package_path);
    }

    VirtualPackageMatch::NoMatch
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
