//! Header-stage import, re-export, facade, alias, and collision diagnostic helpers.
//!
//! WHAT: named helpers that construct structured diagnostics for import-environment failures.
//! WHY: centralizing diagnostic construction keeps error messages consistent and makes it
//! easy to update wording or metadata across all import-related failures.
//! MUST NOT: contain business logic for resolution or registration.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::headers::import_environment::facade_resolution::FacadeType;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

/// Diagnostic when two visible bindings in the same file target different symbols.
pub(super) fn import_name_collision(
    local_name: StringId,
    location: SourceLocation,
    string_table: &StringTable,
) -> CompilerError {
    let name = string_table.resolve(local_name);
    let mut error = CompilerError::new_rule_error(
        format!(
            "Import name collision: '{name}' is already visible in this file. Choose a different alias or rename the existing declaration."
        ),
        location,
    );
    error.new_metadata_entry(ErrorMetaDataKey::CompilationStage, "Import Binding".into());
    error.new_metadata_entry(ErrorMetaDataKey::ConflictType, "ImportNameCollision".into());
    error.new_metadata_entry(ErrorMetaDataKey::VariableName, name.to_owned());
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        "Use a different import alias with `as`, or rename the existing declaration.".into(),
    );
    error
}

/// Diagnostic when a facade import resolves to a symbol that the facade does not export.
pub(super) fn not_exported_by_facade(
    import_path: &InternedPath,
    facade_name: &str,
    facade_type: FacadeType,
    location: SourceLocation,
    string_table: &StringTable,
) -> CompilerError {
    let message = match facade_type {
        FacadeType::SourceLibrary => format!(
            "Cannot import '{}' from '@{facade_name}' because it is not exported by the source library module facade. Source-library modules expose symbols only through #mod.bst.",
            import_path.to_portable_string(string_table)
        ),
        FacadeType::ModuleRoot => format!(
            "Cannot import '{}' from module '{facade_name}' because it is not exported by the module's #mod.bst facade. Modules expose symbols only through their facade.",
            import_path.to_portable_string(string_table)
        ),
    };
    CompilerError::new_rule_error(message, location)
}

/// Diagnostic when an import path directly references a `#mod.bst` file.
pub(super) fn direct_mod_file_import(
    path: &InternedPath,
    location: SourceLocation,
    string_table: &StringTable,
) -> CompilerError {
    CompilerError::new_rule_error(
        format!(
            "Cannot import or re-export directly from '#mod.bst' via '{}'. Module facades are resolved automatically; import exported symbols through the module path instead.",
            path.to_portable_string(string_table)
        ),
        location,
    )
}

/// Diagnostic when an import path matches a source file but not a symbol.
pub(super) fn bare_file_import(
    path: &InternedPath,
    location: SourceLocation,
    string_table: &StringTable,
) -> CompilerError {
    CompilerError::new_rule_error(
        format!(
            "Bare file import '{}' is not supported. Import specific exported symbols using '@path/to/file/symbol' instead.",
            path.to_portable_string(string_table)
        ),
        location,
    )
}

/// Diagnostic when an import path cannot be resolved to any known source or external symbol.
pub(super) fn missing_import_target(
    path: &InternedPath,
    location: SourceLocation,
    string_table: &StringTable,
) -> CompilerError {
    CompilerError::new_rule_error(
        format!(
            "Missing import target '{}'. Could not resolve this dependency in the current module.",
            path.to_portable_string(string_table)
        ),
        location,
    )
}

/// Diagnostic when a direct source import targets a symbol that is not exported.
pub(super) fn not_exported_by_source_file(
    symbol_path: &InternedPath,
    location: SourceLocation,
    string_table: &StringTable,
) -> CompilerError {
    CompilerError::new_rule_error(
        format!(
            "Cannot import '{}' because it is not exported. Add '#' to export it from its source file.",
            symbol_path.to_portable_string(string_table)
        ),
        location,
    )
}

/// Diagnostic when an import path matches multiple source symbols ambiguously.
pub(super) fn ambiguous_import_target(
    path: &InternedPath,
    location: SourceLocation,
    string_table: &StringTable,
) -> CompilerError {
    CompilerError::new_rule_error(
        format!(
            "Ambiguous import target '{}'. Use a more specific path.",
            path.to_portable_string(string_table)
        ),
        location,
    )
}

/// Diagnostic when a virtual package exists but the requested symbol is not found.
pub(super) fn missing_package_symbol(
    symbol_name: &str,
    package_path: &str,
    location: SourceLocation,
) -> CompilerError {
    CompilerError::new_rule_error(
        format!(
            "Cannot import '{symbol_name}' from package '{package_path}': symbol not found in package."
        ),
        location,
    )
}

/// Diagnostic when a re-export uses a duplicate export name in a module facade.
pub(super) fn duplicate_facade_export_name(
    export_name: &str,
    location: SourceLocation,
) -> CompilerError {
    CompilerError::new_rule_error(
        format!(
            "Duplicate export name '{export_name}' in module facade. Each exported name must be unique."
        ),
        location,
    )
}

/// Diagnostic when a re-export path is missing a symbol name.
pub(super) fn missing_reexport_symbol_name(location: SourceLocation) -> CompilerError {
    CompilerError::new_rule_error("Re-export path is missing a symbol name.", location)
}

/// Diagnostic when a module has no facade and an external importer tries to import from it.
pub(super) fn missing_module_facade(
    symbol_path: &InternedPath,
    location: SourceLocation,
    string_table: &StringTable,
) -> CompilerError {
    CompilerError::new_rule_error(
        format!(
            "Cannot import '{}' because the module has no #mod.bst facade. A module without a facade has no outward public API.",
            symbol_path.to_portable_string(string_table)
        ),
        location,
    )
}

/// Diagnostic when an import targets a symbol in another module root that is not exported by that module's facade.
pub(super) fn cross_module_import_not_exported(
    symbol_path: &InternedPath,
    location: SourceLocation,
    string_table: &StringTable,
) -> CompilerError {
    CompilerError::new_rule_error(
        format!(
            "Cannot import '{}' because it is not exported by the module's #mod.bst facade. Modules expose symbols only through their facade.",
            symbol_path.to_portable_string(string_table)
        ),
        location,
    )
}
