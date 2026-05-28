//! Header-stage import, facade, alias, and collision diagnostic helpers.
//!
//! WHAT: named helpers that construct structured diagnostics for import-environment failures.
//! WHY: centralizing diagnostic construction keeps error messages consistent and makes it
//! easy to update wording or metadata across all import-related failures.
//! MUST NOT: contain business logic for resolution or registration.

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::compiler_messages::ImportFacadeType;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

/// Diagnostic when two visible bindings in the same file target different symbols.
pub(super) fn import_name_collision(
    local_name: StringId,
    location: SourceLocation,
    previous_location: Option<SourceLocation>,
) -> CompilerDiagnostic {
    CompilerDiagnostic::import_name_collision(local_name, previous_location, location)
}

/// Diagnostic when a facade import resolves to a symbol that the facade does not export.
pub(super) fn not_exported_by_facade(
    import_path: &InternedPath,
    facade_name: StringId,
    facade_type: ImportFacadeType,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::not_exported_by_facade(
        import_path.clone(),
        facade_name,
        facade_type,
        location,
    )
}

/// Diagnostic when an import path directly references a special file (`#mod.bst`,
/// `#page.bst`, or `#config.bst`).
pub(super) fn direct_special_file_import(
    path: &InternedPath,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::direct_special_file_import(path.clone(), location)
}

/// Diagnostic when an import path matches a source file but not a symbol.
pub(super) fn bare_file_import(
    path: &InternedPath,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::bare_file_import(path.clone(), location)
}

/// Diagnostic when an import path cannot be resolved to any known source or external symbol.
pub(super) fn missing_import_target(
    path: &InternedPath,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::missing_import_target(path.clone(), location)
}

/// Diagnostic when an import path has no name component and therefore no target.
pub(super) fn missing_import_target_no_path(location: SourceLocation) -> CompilerDiagnostic {
    CompilerDiagnostic::missing_import_target_no_path(location)
}

/// Diagnostic when a direct source import targets a symbol that is not exported.
pub(super) fn not_exported_by_source_file(
    symbol_path: &InternedPath,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::not_exported_by_source_file(symbol_path.clone(), location)
}

/// Diagnostic when an import path matches multiple source symbols ambiguously.
pub(super) fn ambiguous_import_target(
    path: &InternedPath,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::ambiguous_import_target(path.clone(), location)
}

/// Diagnostic when a virtual package exists but the requested symbol is not found.
pub(super) fn missing_package_symbol(
    symbol: StringId,
    package_path: StringId,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::missing_package_symbol(symbol, package_path, location)
}

/// Diagnostic when a module has no facade and an external importer tries to import from it.
pub(super) fn missing_module_facade(
    symbol_path: &InternedPath,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::missing_module_facade(symbol_path.clone(), location)
}

/// Diagnostic when an import targets a symbol in another module root that is not exported by that module's facade.
pub(super) fn cross_module_import_not_exported(
    symbol_path: &InternedPath,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::cross_module_import_not_exported(symbol_path.clone(), location)
}
