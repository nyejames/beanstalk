//! Strict header dependency edge collection.
//!
//! WHAT: converts type references from declaration shells into dependency edges for top-level
//! declaration sorting.
//! WHY: dependency sorting uses strict structural edges only; expression/body references stay soft
//! and are resolved later by AST.

use crate::compiler_frontend::builtins::error_type::is_reserved_builtin_symbol;
use crate::compiler_frontend::declaration_syntax::declaration_shell::DeclarationSyntax;
use crate::compiler_frontend::declaration_syntax::type_syntax::for_each_named_type_in_data_type;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::types::HeaderBuildContext;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use std::collections::HashSet;

/// Collect strict dependency edges from a constant's declared type annotation.
///
/// WHY: only the declared type creates a structural ordering constraint; initializer-expression
/// symbol references are soft hints that are intentionally excluded from strict graph edges.
pub(super) fn collect_constant_type_dependencies(
    declaration_syntax: &DeclarationSyntax,
    context: &HeaderBuildContext<'_>,
    dependencies: &mut HashSet<InternedPath>,
) {
    for_each_named_type_in_data_type(&declaration_syntax.type_annotation, &mut |type_name| {
        collect_named_type_dependency_edge(
            type_name,
            context.file_imports,
            context.source_file,
            context.external_package_registry,
            context.string_table,
            dependencies,
        );
    });
}

pub(super) fn collect_named_type_dependency_edge(
    type_name: StringId,
    file_imports: &HashSet<InternedPath>,
    source_file: &InternedPath,
    external_package_registry: &ExternalPackageRegistry,
    string_table: &StringTable,
    dependencies: &mut HashSet<InternedPath>,
) {
    if is_reserved_builtin_symbol(string_table.resolve(type_name)) {
        return;
    }

    let edge = file_imports
        .iter()
        .find(|import_path| import_path.name() == Some(type_name))
        .cloned();

    // Virtual package imports are not source graph participants, so they must not
    // create strict dependency edges. AST import binding resolution handles them later.
    if let Some(ref import_path) = edge
        && external_package_registry.is_virtual_package_import(import_path, string_table)
    {
        return;
    }

    let edge = edge.unwrap_or_else(|| source_file.append(type_name));
    dependencies.insert(edge);
}
