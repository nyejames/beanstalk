//! Header-stage import target resolution.
//!
//! WHAT: resolves a parsed `@path/to/symbol` import into a concrete source symbol or external
//! package symbol.
//! WHY: both explicit imports and re-exports resolve their targets the same way; extracting this
//! avoids duplicating the resolution sequence across re-export and import paths.
//! MUST NOT: register visible names, enforce file-local collision policy, or validate export
//! flags (those belong in the orchestration layer and `visible_names.rs`).

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::external_packages::{ExternalPackageRegistry, ExternalSymbolId};
use crate::compiler_frontend::headers::import_environment::diagnostics;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use rustc_hash::FxHashSet;

/// Resolved target of a single import or re-export path.
///
/// WHY: explicit enums make the resolution path visible in type names and match arms.
pub(crate) enum ResolvedImportTarget {
    Source {
        symbol_path: InternedPath,
        #[allow(dead_code)]
        export_requirement: ExportRequirement,
    },
    External {
        symbol_id: ExternalSymbolId,
    },
}

/// Whether the source symbol still needs an export check.
///
/// WHY: facade-resolved imports are already validated by the facade; direct file imports must
/// still check the source file's export flag.
#[allow(dead_code)]
pub(crate) enum ExportRequirement {
    AlreadyValidatedByFacade,
    MustBeExportedFromSourceFile,
}

/// Input bundle for resolving one import target.
///
/// WHY: avoids threading many state references as separate function parameters.
pub(crate) struct ImportTargetResolutionInput<'a> {
    pub(crate) import_path: &'a InternedPath,
    pub(crate) location: &'a SourceLocation,
    pub(crate) module_file_paths: &'a FxHashSet<InternedPath>,
    pub(crate) importable_symbol_paths: &'a FxHashSet<InternedPath>,
    pub(crate) external_package_registry: &'a ExternalPackageRegistry,
    pub(crate) string_table: &'a StringTable,
}

/// Resolve an `@path/to/symbol` to its concrete target.
///
/// WHAT: performs source-symbol resolution (exact match and suffix match with optional `.bst`
/// extension), file→symbol inference, and virtual-package lookup.
///
/// Returns `MustBeExportedFromSourceFile` for all source targets because this function does not
/// know whether the caller already validated the path through a facade.
pub(crate) fn resolve_import_target(
    input: ImportTargetResolutionInput<'_>,
) -> Result<ResolvedImportTarget, CompilerError> {
    // Resolve as a source symbol import first.
    match resolve_import_target_path(
        input.import_path,
        input.importable_symbol_paths,
        input.string_table,
    ) {
        ImportPathMatch::Resolved(symbol_path) => Ok(ResolvedImportTarget::Source {
            symbol_path,
            export_requirement: ExportRequirement::MustBeExportedFromSourceFile,
        }),
        ImportPathMatch::Ambiguous => Err(diagnostics::ambiguous_import_target(
            input.import_path,
            input.location.clone(),
            input.string_table,
        )),
        ImportPathMatch::Missing => {
            // File→symbol inference: if the path matches a source file but not a symbol,
            // try appending the path's last component to the file path as the symbol name.
            if let ImportPathMatch::Resolved(ref file_path) = resolve_import_target_path(
                input.import_path,
                input.module_file_paths,
                input.string_table,
            ) && let Some(inferred_name) = input.import_path.name()
            {
                let inferred_path = file_path.append(inferred_name);
                match resolve_import_target_path(
                    &inferred_path,
                    input.importable_symbol_paths,
                    input.string_table,
                ) {
                    ImportPathMatch::Resolved(symbol_path) => {
                        return Ok(ResolvedImportTarget::Source {
                            symbol_path,
                            export_requirement: ExportRequirement::MustBeExportedFromSourceFile,
                        });
                    }
                    ImportPathMatch::Ambiguous => {
                        return Err(diagnostics::ambiguous_import_target(
                            &inferred_path,
                            input.location.clone(),
                            input.string_table,
                        ));
                    }
                    ImportPathMatch::Missing => {
                        // The file exists but the inferred symbol does not.
                        // Fall through to standard error handling.
                    }
                }
            }

            // Try to resolve as a virtual package import.
            match resolve_virtual_package_import(
                input.import_path,
                input.external_package_registry,
                input.string_table,
            ) {
                VirtualPackageMatch::Found(package_path, symbol_name) => {
                    let symbol_name_str = input.string_table.resolve(symbol_name);
                    let external_symbol_id = if let Some((func_id, _)) = input
                        .external_package_registry
                        .resolve_package_function(&package_path, symbol_name_str)
                    {
                        Some(ExternalSymbolId::Function(func_id))
                    } else if let Some((type_id, _)) = input
                        .external_package_registry
                        .resolve_package_type(&package_path, symbol_name_str)
                    {
                        Some(ExternalSymbolId::Type(type_id))
                    } else if let Some((const_id, _)) = input
                        .external_package_registry
                        .resolve_package_constant(&package_path, symbol_name_str)
                    {
                        Some(ExternalSymbolId::Constant(const_id))
                    } else {
                        None
                    };

                    if let Some(id) = external_symbol_id {
                        return Ok(ResolvedImportTarget::External { symbol_id: id });
                    }

                    let symbol_name = input
                        .import_path
                        .name_str(input.string_table)
                        .unwrap_or("<unknown>");
                    return Err(diagnostics::missing_package_symbol(
                        symbol_name,
                        &package_path,
                        input.location.clone(),
                    ));
                }
                VirtualPackageMatch::PackageFoundSymbolMissing(package_path) => {
                    let symbol_name = input
                        .import_path
                        .name_str(input.string_table)
                        .unwrap_or("<unknown>");
                    return Err(diagnostics::missing_package_symbol(
                        symbol_name,
                        &package_path,
                        input.location.clone(),
                    ));
                }
                VirtualPackageMatch::NoMatch => {}
            }

            // If the path matches a module file but not a symbol, report a bare-file import error.
            if let ImportPathMatch::Resolved(_) | ImportPathMatch::Ambiguous =
                resolve_import_target_path(
                    input.import_path,
                    input.module_file_paths,
                    input.string_table,
                )
            {
                return Err(diagnostics::bare_file_import(
                    input.import_path,
                    input.location.clone(),
                    input.string_table,
                ));
            }

            Err(diagnostics::missing_import_target(
                input.import_path,
                input.location.clone(),
                input.string_table,
            ))
        }
    }
}

/// Internal result of matching one import path against a candidate set.
enum ImportPathMatch {
    Missing,
    Ambiguous,
    Resolved(InternedPath),
}

/// Match a requested path against a set of candidate paths.
///
/// WHAT: first tries exact component match (with optional `.bst` extension), then tries suffix
/// match (with optional `.bst` extension).
/// WHY: `@path/to/symbol` may match `@path/to/symbol.bst` or a longer path ending in the
/// requested suffix.
fn resolve_import_target_path(
    requested_path: &InternedPath,
    candidates: &FxHashSet<InternedPath>,
    string_table: &StringTable,
) -> ImportPathMatch {
    let exact_matches: Vec<_> = candidates
        .iter()
        .filter(|candidate| exact_path_matches_candidate(candidate, requested_path, string_table))
        .cloned()
        .collect();

    match exact_matches.len() {
        1 => {
            if let Some(path) = exact_matches.into_iter().next() {
                return ImportPathMatch::Resolved(path);
            }
            return ImportPathMatch::Missing;
        }
        2.. => return ImportPathMatch::Ambiguous,
        _ => {}
    }

    let matches: Vec<_> = candidates
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
        .collect();

    match matches.len() {
        0 => ImportPathMatch::Missing,
        1 => matches
            .into_iter()
            .next()
            .map(ImportPathMatch::Resolved)
            .unwrap_or(ImportPathMatch::Missing),
        _ => ImportPathMatch::Ambiguous,
    }
}

/// Result of attempting to resolve an import path as a virtual package symbol.
enum VirtualPackageMatch {
    Found(
        String,
        crate::compiler_frontend::symbols::string_interning::StringId,
    ),
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
            || registry
                .resolve_package_constant(&package_path, symbol_name_str)
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
    candidate_components: &[crate::compiler_frontend::symbols::string_interning::StringId],
    requested_components: &[crate::compiler_frontend::symbols::string_interning::StringId],
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
