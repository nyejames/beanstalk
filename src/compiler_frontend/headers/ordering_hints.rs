//! Header-stage local declaration-ordering hint collection.
//!
//! WHAT: converts type references from declaration shells into conservative local
//! declaration-ordering hints retained before provider binding.
//! WHY: syntax preparation records the import spelling or same-file spelling uniformly without
//! knowing which imports are source graph participants versus virtual or provider bindings.
//! Binding canonicalizes or drops import-spelled hints; Stage 3 resolves retained local hints
//! into sortable graph edges.

use crate::compiler_frontend::builtins::error_type::is_reserved_builtin_symbol;
use crate::compiler_frontend::declaration_syntax::declaration_shell::DeclarationSyntax;
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    collect_capacity_references_in_parsed_ref, for_each_named_type_in_parsed_ref,
};
use crate::compiler_frontend::headers::parse_file_headers::FileImport;
use crate::compiler_frontend::headers::types::{HeaderBuildContext, LocalDeclarationOrderingHint};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::utilities::token_scan::InitializerReference;
use std::collections::HashSet;

/// Collect local declaration-ordering hints from a constant's declared type annotation.
///
/// WHY: only the declared type creates a structural ordering constraint.
/// Initializer-expression constant references are handled by
/// `constant_dependencies::add_constant_initializer_dependencies`.
pub(super) fn collect_constant_type_hints(
    declaration_syntax: &DeclarationSyntax,
    context: &HeaderBuildContext<'_>,
    hints: &mut HashSet<LocalDeclarationOrderingHint>,
    capacity_references: &mut Vec<InitializerReference>,
) {
    for_each_named_type_in_parsed_ref(&declaration_syntax.type_annotation, &mut |type_name| {
        collect_named_type_ordering_hint(
            type_name,
            context.file_import_entries,
            context.source_file,
            context.string_table,
            hints,
        );
    });
    collect_capacity_references_in_parsed_ref(
        &declaration_syntax.type_annotation,
        capacity_references,
    );
}

/// Record one conservative local declaration-ordering hint for a named type reference.
///
/// WHAT: records the import spelling when the name matches a file import, otherwise records the
/// same-file spelling. Builtin symbol names are excluded as compiler-owned syntax policy.
/// WHY: syntax preparation must not consult provider availability to decide whether a named type
/// reference is a virtual or provider import. Binding later canonicalizes or drops import-spelled
/// hints using bound visibility; Stage 3 resolves retained local hints into graph edges.
pub(super) fn collect_named_type_ordering_hint(
    type_name: StringId,
    file_imports: &[FileImport],
    source_file: &InternedPath,
    string_table: &StringTable,
    hints: &mut HashSet<LocalDeclarationOrderingHint>,
) {
    if is_reserved_builtin_symbol(string_table.resolve(type_name)) {
        return;
    }

    // WHY: match by local name, which is either the explicit import alias or
    // the original symbol name from the path. This records the import spelling
    // when an import alias is used as a type reference.
    let referenced_path = file_imports
        .iter()
        .find(|import| {
            let local_name = import.alias.or_else(|| import.provider.path.name());
            local_name == Some(type_name)
        })
        .map(|import| import.provider.path.clone())
        .unwrap_or_else(|| source_file.append(type_name));

    hints.insert(LocalDeclarationOrderingHint::new(referenced_path));
}
