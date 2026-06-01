//! Order-independent header symbol collection.
//!
//! WHAT: validates declared names, builds per-file import/export maps, records generic declaration
//! metadata, and stages builtin declarations during header parsing.
//! WHY: this work depends only on parsed headers, not dependency order. Keeping it separate makes
//! `parse_headers` orchestration-first and leaves dependency sorting as the owner of declaration
//! ordering.

#![allow(clippy::result_large_err)]

use crate::compiler_frontend::builtins::error_type::{
    is_reserved_builtin_symbol, register_builtin_error_types,
};
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, DiagnosticBag};
use crate::compiler_frontend::datatypes::generic_parameters::GenericParameterList;
use crate::compiler_frontend::datatypes::parsed::ParsedTypeRef;
use crate::compiler_frontend::declaration_syntax::signature_members::FunctionSignatureSyntax;
use crate::compiler_frontend::headers::module_symbols::{
    GenericDeclarationKind, GenericDeclarationMetadata, ModuleSymbols, register_declared_symbol,
};
use crate::compiler_frontend::headers::types::{Header, HeaderKind};
use crate::compiler_frontend::symbols::identifier_policy::ensure_not_keyword_shadow_identifier;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

/// Collect all order-independent top-level symbol metadata from parsed (unsorted) headers.
pub(super) fn build_module_symbols(
    headers: &[Header],
    string_table: &mut StringTable,
) -> Result<ModuleSymbols, DiagnosticBag> {
    let mut module_symbols = ModuleSymbols::empty();
    let mut diagnostic_bag = DiagnosticBag::new();

    for header in headers {
        if !validate_declared_name(header, string_table, &mut diagnostic_bag) {
            continue;
        }

        module_symbols
            .module_file_paths
            .insert(header.source_file.to_owned());
        // Mutation: canonical OS paths are project-derived inputs that must be interned
        // before downstream stages can use them as InternedPath values.
        module_symbols.canonical_source_by_symbol_path.insert(
            header.tokens.src_path.to_owned(),
            header.canonical_source_file(string_table),
        );

        merge_header_imports(&mut module_symbols, header);
        register_header_symbol(&mut module_symbols, header, string_table);
    }

    if diagnostic_bag.has_errors() {
        return Err(diagnostic_bag);
    }

    register_builtin_symbols(&mut module_symbols, string_table);

    Ok(module_symbols)
}

fn validate_declared_name(
    header: &Header,
    string_table: &StringTable,
    diagnostic_bag: &mut DiagnosticBag,
) -> bool {
    let Some(symbol_name) = header.tokens.src_path.name() else {
        return true;
    };

    let symbol_name_text = string_table.resolve(symbol_name).to_owned();

    if let Err(diagnostic) = ensure_not_keyword_shadow_identifier(
        symbol_name,
        header.name_location.to_owned(),
        string_table,
    ) {
        diagnostic_bag.push(diagnostic);
        return false;
    }

    if is_reserved_builtin_symbol(&symbol_name_text) {
        diagnostic_bag.push(CompilerDiagnostic::reserved_builtin_name(
            symbol_name,
            header.name_location.to_owned(),
        ));
        return false;
    }

    true
}

fn merge_header_imports(module_symbols: &mut ModuleSymbols, header: &Header) {
    module_symbols
        .file_imports_by_source
        .entry(header.source_file.to_owned())
        .and_modify(|existing| {
            for import in &header.file_imports {
                let already_present = existing.iter().any(|entry| {
                    entry.header_path == import.header_path && entry.alias == import.alias
                });
                if !already_present {
                    existing.push(import.clone());
                }
            }
        })
        .or_insert_with(|| header.file_imports.to_owned());
}

/// Detect whether a parsed function signature is a receiver method candidate.
///
/// WHAT: checks if the first parameter is named `this`.
/// WHY: header stage needs to route receiver methods away from free-function
///      value-member paths without waiting for AST type resolution.
/// NOTE: invalid receiver types (unsupported types, wrong file, etc.) are left
///       for AST validation; this helper only identifies the candidate shape.
fn is_receiver_method_candidate(
    signature: &FunctionSignatureSyntax,
    string_table: &StringTable,
) -> bool {
    let Some(first_parameter) = signature.parameters.first() else {
        return false;
    };

    first_parameter.id.name_str(string_table) == Some("this")
}

/// Extract the parsed receiver type name from a receiver-method candidate.
///
/// WHAT: records `Counter` from `tick |this Counter|` before semantic type resolution.
/// WHY: header import preparation can then auto-import only methods attached to an imported
///      nominal type from the same surface instead of every receiver method in that file.
fn receiver_method_receiver_name(
    signature: &FunctionSignatureSyntax,
    string_table: &mut StringTable,
) -> Option<crate::compiler_frontend::symbols::string_interning::StringId> {
    let first_parameter = signature.parameters.first()?;

    if first_parameter.id.name_str(string_table) != Some("this") {
        return None;
    }

    match &first_parameter.type_annotation {
        ParsedTypeRef::Named { name, .. } => Some(*name),
        ParsedTypeRef::Namespaced { name, .. } => Some(*name),
        // Builtin scalar types are parsed directly; map them to their language-visible names.
        ParsedTypeRef::BuiltinInt { .. } => Some(string_table.intern("Int")),
        ParsedTypeRef::BuiltinFloat { .. } => Some(string_table.intern("Float")),
        ParsedTypeRef::BuiltinBool { .. } => Some(string_table.intern("Bool")),
        ParsedTypeRef::BuiltinString { .. } => Some(string_table.intern("String")),
        ParsedTypeRef::BuiltinChar { .. } => Some(string_table.intern("Char")),
        _ => None,
    }
}

fn register_header_symbol(
    module_symbols: &mut ModuleSymbols,
    header: &Header,
    string_table: &mut StringTable,
) {
    match &header.kind {
        HeaderKind::Function {
            generic_parameters,
            signature,
        } => {
            register_declared_symbol(
                module_symbols,
                &header.tokens.src_path,
                &header.source_file,
                Some(true),
            );
            register_generic_declaration_metadata(
                module_symbols,
                header,
                generic_parameters,
                GenericDeclarationKind::Function,
            );
            if is_receiver_method_candidate(signature, string_table) {
                module_symbols
                    .receiver_method_paths
                    .insert(header.tokens.src_path.to_owned());

                if let Some(receiver_name) = receiver_method_receiver_name(signature, string_table)
                {
                    module_symbols
                        .receiver_method_receiver_names
                        .insert(header.tokens.src_path.to_owned(), receiver_name);
                }
            }
        }

        HeaderKind::Struct {
            generic_parameters, ..
        } => {
            register_declared_symbol(
                module_symbols,
                &header.tokens.src_path,
                &header.source_file,
                Some(true),
            );
            module_symbols
                .nominal_type_paths
                .insert(header.tokens.src_path.to_owned());
            register_generic_declaration_metadata(
                module_symbols,
                header,
                generic_parameters,
                GenericDeclarationKind::Struct,
            );
        }

        HeaderKind::Choice {
            generic_parameters, ..
        } => {
            register_declared_symbol(
                module_symbols,
                &header.tokens.src_path,
                &header.source_file,
                Some(true),
            );
            module_symbols
                .nominal_type_paths
                .insert(header.tokens.src_path.to_owned());
            register_generic_declaration_metadata(
                module_symbols,
                header,
                generic_parameters,
                GenericDeclarationKind::Choice,
            );
        }

        HeaderKind::StartFunction => {
            // Register the compiler-owned implicit start function under its entry source file.
            let start_name = header
                .source_file
                .join_str(IMPLICIT_START_FUNC_NAME, string_table);
            register_declared_symbol(module_symbols, &start_name, &header.source_file, None);
        }

        HeaderKind::Constant { .. } => {
            register_declared_symbol(
                module_symbols,
                &header.tokens.src_path,
                &header.source_file,
                Some(true),
            );
        }

        HeaderKind::TypeAlias { .. } => {
            register_declared_symbol(
                module_symbols,
                &header.tokens.src_path,
                &header.source_file,
                Some(true),
            );
            module_symbols
                .type_alias_paths
                .insert(header.tokens.src_path.to_owned());
        }

        HeaderKind::ConstTemplate { .. } => {}

        HeaderKind::Trait { .. } => {
            register_declared_symbol(
                module_symbols,
                &header.tokens.src_path,
                &header.source_file,
                Some(true),
            );
            module_symbols
                .trait_paths
                .insert(header.tokens.src_path.clone());
        }

        HeaderKind::TraitConformance { .. } => {
            // Conformance declarations are compile-time metadata. They do not introduce a new
            // importable symbol; AST validates and indexes evidence later.
        }
    }
}

fn register_builtin_symbols(module_symbols: &mut ModuleSymbols, string_table: &mut StringTable) {
    // Builtins are merged once here so AST passes see them without a separate absorption step.
    // Mutation: builtin error types register compiler-owned fixed symbols into the table.
    let builtin_manifest = register_builtin_error_types(string_table);
    module_symbols
        .builtin_visible_symbol_paths
        .extend(builtin_manifest.visible_symbol_paths.iter().cloned());
    module_symbols.builtin_declarations = builtin_manifest.declarations;
    module_symbols
        .resolved_struct_fields_by_path
        .extend(builtin_manifest.resolved_struct_fields_by_path);
    module_symbols
        .struct_source_by_path
        .extend(builtin_manifest.struct_source_by_path);
    module_symbols
        .builtin_struct_ast_nodes
        .extend(builtin_manifest.ast_struct_nodes);
}

fn register_generic_declaration_metadata(
    module_symbols: &mut ModuleSymbols,
    header: &Header,
    generic_parameters: &GenericParameterList,
    kind: GenericDeclarationKind,
) {
    if generic_parameters.is_empty() {
        return;
    }

    // Semantic generic behavior belongs to the generics implementation plan; this header-stage
    // metadata only preserves parsed declaration facts for later AST work.
    module_symbols.generic_declarations_by_path.insert(
        header.tokens.src_path.to_owned(),
        GenericDeclarationMetadata {
            kind,
            parameters: generic_parameters.to_owned(),
            declaration_location: header.name_location.to_owned(),
        },
    );
}
