//! Header-stage constant dependency extraction.
//!
//! WHAT: classifies symbol-shaped references captured from constant initializer tokens and adds
//! top-level dependency edges between constants.
//! WHY: dependency sorting must order constants before AST folds their initializer expressions.
//! MUST NOT: type-check expressions or decide whether a full initializer is foldable.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::declaration_syntax::declaration_shell::InitializerReference;
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::import_environment::{
    FileVisibility, HeaderImportEnvironment,
};
use crate::compiler_frontend::headers::module_symbols::{GenericDeclarationKind, ModuleSymbols};
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) struct ConstantDependencyInput<'a> {
    pub(crate) headers: &'a mut [Header],
    pub(crate) module_symbols: &'a ModuleSymbols,
    pub(crate) import_environment: &'a HeaderImportEnvironment,
    pub(crate) string_table: &'a mut StringTable,
}

pub(crate) struct ConstantDependencyReport {
    pub(crate) added_edges: usize,
    pub(crate) same_file_edges: usize,
    pub(crate) cross_file_edges: usize,
}

pub(crate) enum ConstantReferenceResolution {
    SourceConstant {
        path: InternedPath,
        source_file: InternedPath,
    },
    SourceNonConstant {
        _path: InternedPath,
    },
    SourceTypeAlias {
        _path: InternedPath,
    },
    ExternalConstant {
        _symbol_id: ExternalSymbolId,
    },
    ExternalNonConstant {
        _symbol_id: ExternalSymbolId,
    },
    ConstructorLikeSource {
        _path: InternedPath,
    },
    NotVisible {
        name: StringId,
    },
}

pub(crate) fn add_constant_initializer_dependencies(
    input: ConstantDependencyInput<'_>,
) -> Result<ConstantDependencyReport, Vec<CompilerError>> {
    let ConstantDependencyInput {
        headers,
        module_symbols,
        import_environment,
        string_table,
    } = input;

    let mut errors: Vec<CompilerError> = Vec::new();
    let mut report = ConstantDependencyReport {
        added_edges: 0,
        same_file_edges: 0,
        cross_file_edges: 0,
    };

    // Build indexes for fast constant and struct/choice lookups.
    let mut constant_paths: FxHashSet<InternedPath> = FxHashSet::default();
    let mut constants_by_name: FxHashMap<StringId, Vec<InternedPath>> = FxHashMap::default();
    let mut constant_source_orders: FxHashMap<InternedPath, usize> = FxHashMap::default();
    let mut struct_or_choice_paths: FxHashSet<InternedPath> = FxHashSet::default();

    for header in headers.iter() {
        match &header.kind {
            HeaderKind::Constant { source_order, .. } => {
                let path = header.tokens.src_path.clone();
                constant_paths.insert(path.clone());
                constant_source_orders.insert(path.clone(), *source_order);
                if let Some(name) = path.name() {
                    constants_by_name.entry(name).or_default().push(path);
                }
            }
            HeaderKind::Struct { .. } | HeaderKind::Choice { .. } => {
                struct_or_choice_paths.insert(header.tokens.src_path.clone());
            }
            _ => {}
        }
    }

    // Collect edges and errors in a first pass, then apply edges in a second pass.
    // WHY: avoids borrowing headers both immutably (for reading kind/source_file) and mutably
    // (for inserting into dependencies) at the same time.
    let mut edges_to_add: Vec<(usize, InternedPath)> = Vec::new();

    for (header_index, header) in headers.iter().enumerate() {
        let HeaderKind::Constant {
            declaration,
            source_order,
        } = &header.kind
        else {
            continue;
        };

        let visibility = match import_environment.visibility_for(&header.source_file) {
            Ok(v) => v,
            Err(error) => {
                errors.push(error);
                continue;
            }
        };

        let current_path = header.tokens.src_path.clone();

        for reference in &declaration.initializer_references {
            let resolution = classify_reference(
                reference,
                visibility,
                &constant_paths,
                &struct_or_choice_paths,
                module_symbols,
            );

            match resolution {
                // Constants create ordering edges. Same-file edges are still constrained by source order.
                ConstantReferenceResolution::SourceConstant { path, source_file } => {
                    if path == current_path {
                        errors.push(self_reference_error(reference, string_table));
                        continue;
                    }

                    // Compare canonical source files because module_symbols stores canonical paths
                    // while header.source_file may be a logical/relative path.
                    let current_canonical_source = header.canonical_source_file(string_table);
                    if source_file == current_canonical_source {
                        let target_order = constant_source_orders.get(&path).copied().unwrap_or(0);
                        if target_order > *source_order {
                            errors.push(same_file_forward_reference_error(
                                &current_path,
                                &path,
                                reference,
                                string_table,
                            ));
                            continue;
                        }
                        report.same_file_edges += 1;
                    } else {
                        report.cross_file_edges += 1;
                    }

                    edges_to_add.push((header_index, path));
                }

                // Type aliases live in the type namespace. They do not create value dependency edges.
                ConstantReferenceResolution::SourceTypeAlias { .. }
                | ConstantReferenceResolution::ExternalConstant { .. }
                | ConstantReferenceResolution::ConstructorLikeSource { .. } => {}

                // Source non-constants are structurally invalid in constant initializers.
                // External non-constants are deferred to AST because header stage cannot
                // determine whether an external call is foldable or valid in all contexts.
                ConstantReferenceResolution::SourceNonConstant { .. } => {
                    errors.push(non_constant_reference_error(reference, string_table));
                }

                // External references are deferred to AST folding validation.
                ConstantReferenceResolution::ExternalNonConstant { .. } => {}

                // A constant with this name exists in the module but is not visible to this file.
                ConstantReferenceResolution::NotVisible { name } => {
                    if constants_by_name.contains_key(&name) {
                        errors.push(not_visible_constant_error(reference, string_table));
                    }
                    // If no constant with this name exists anywhere, treat as Unknown so AST
                    // can produce a more precise diagnostic during expression parsing.
                }
            }
        }
    }

    for (header_index, path) in edges_to_add {
        let header = &mut headers[header_index];
        if header.dependencies.insert(path) {
            report.added_edges += 1;
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(report)
}

fn classify_reference(
    reference: &InitializerReference,
    visibility: &FileVisibility,
    constant_paths: &FxHashSet<InternedPath>,
    struct_or_choice_paths: &FxHashSet<InternedPath>,
    module_symbols: &ModuleSymbols,
) -> ConstantReferenceResolution {
    // 1. External symbols: constants are valid references; non-constants are errors.
    if let Some(symbol_id) = visibility.visible_external_symbols.get(&reference.name) {
        return if matches!(symbol_id, ExternalSymbolId::Constant(_)) {
            ConstantReferenceResolution::ExternalConstant {
                _symbol_id: *symbol_id,
            }
        } else {
            ConstantReferenceResolution::ExternalNonConstant {
                _symbol_id: *symbol_id,
            }
        };
    }

    // 2. Type aliases: valid to resolve but do not create value dependency edges.
    if let Some(path) = visibility.visible_type_alias_names.get(&reference.name) {
        return ConstantReferenceResolution::SourceTypeAlias {
            _path: path.clone(),
        };
    }

    // 3. Source-visible names: may be constants, constructors, or non-constants.
    let Some(target_path) = visibility.visible_source_names.get(&reference.name) else {
        return ConstantReferenceResolution::NotVisible {
            name: reference.name,
        };
    };

    let is_constant = constant_paths.contains(target_path);

    if is_constant {
        // Even if the target is a constant, it might be used as a constructor-like
        // nominal if followed by a call or namespace accessor.
        if (reference.followed_by_call || reference.followed_by_choice_namespace)
            && is_nominal_constructor(target_path, struct_or_choice_paths, module_symbols)
        {
            return ConstantReferenceResolution::ConstructorLikeSource {
                _path: target_path.clone(),
            };
        }

        return ConstantReferenceResolution::SourceConstant {
            path: target_path.clone(),
            source_file: module_symbols
                .canonical_source_by_symbol_path
                .get(target_path)
                .cloned()
                .unwrap_or_else(|| target_path.clone()),
        };
    }

    // Not a constant: check if it's a legitimate constructor-like reference.
    if (reference.followed_by_call || reference.followed_by_choice_namespace)
        && is_nominal_constructor(target_path, struct_or_choice_paths, module_symbols)
    {
        return ConstantReferenceResolution::ConstructorLikeSource {
            _path: target_path.clone(),
        };
    }

    ConstantReferenceResolution::SourceNonConstant {
        _path: target_path.clone(),
    }
}

/// Determine whether a visible source name refers to a struct or choice declaration
/// and can therefore be used as a nominal constructor in a constant initializer.
///
/// WHY: constants may construct struct/choice literals at compile time, but function calls
/// and other non-constant references are not valid in constant initializers.
fn is_nominal_constructor(
    target_path: &InternedPath,
    struct_or_choice_paths: &FxHashSet<InternedPath>,
    module_symbols: &ModuleSymbols,
) -> bool {
    // Fast path: the header itself is a struct or choice.
    if struct_or_choice_paths.contains(target_path) {
        return true;
    }

    // Fallback: generic declarations with struct/choice kinds are also constructors.
    if let Some(metadata) = module_symbols.generic_declarations_by_path.get(target_path) {
        return matches!(
            metadata.kind,
            GenericDeclarationKind::Struct | GenericDeclarationKind::Choice
        );
    }

    false
}

// ---------------------------------------------------------------------------
// Diagnostic helpers
// ---------------------------------------------------------------------------

fn self_reference_error(
    reference: &InitializerReference,
    string_table: &StringTable,
) -> CompilerError {
    let name = string_table.resolve(reference.name).to_owned();
    let mut error = CompilerError::new_rule_error(
        format!("Constant '{name}' cannot reference itself in its initializer."),
        reference.location.clone(),
    );
    error.new_metadata_entry(ErrorMetaDataKey::VariableName, name);
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        "Header Constant Dependency Resolution".into(),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        "A constant cannot depend on itself. Use a different value or compute it differently."
            .into(),
    );
    error
}

fn not_visible_constant_error(
    reference: &InitializerReference,
    string_table: &StringTable,
) -> CompilerError {
    let name = string_table.resolve(reference.name).to_owned();
    let mut error = CompilerError::new_rule_error(
        format!("Constant '{name}' is not visible in this file."),
        reference.location.clone(),
    );
    error.new_metadata_entry(ErrorMetaDataKey::VariableName, name);
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        "Header Constant Dependency Resolution".into(),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        "Import the exported constant before using it in this constant initializer.".into(),
    );
    error
}

fn non_constant_reference_error(
    reference: &InitializerReference,
    string_table: &StringTable,
) -> CompilerError {
    let name = string_table.resolve(reference.name).to_owned();
    let mut error = CompilerError::new_rule_error(
        format!(
            "Constants can only reference other constants. '{name}' resolves to a non-constant value."
        ),
        reference.location.clone(),
    );
    error.new_metadata_entry(ErrorMetaDataKey::VariableName, name);
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        "Header Constant Dependency Resolution".into(),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        "Only reference constants in constant declarations and const templates.".into(),
    );
    error
}

fn same_file_forward_reference_error(
    constant_path: &InternedPath,
    target_path: &InternedPath,
    reference: &InitializerReference,
    string_table: &StringTable,
) -> CompilerError {
    let current_name = constant_path.name_str(string_table).unwrap_or("<constant>");
    let target_name = target_path.name_str(string_table).unwrap_or("<constant>");
    let mut error = CompilerError::new_rule_error(
        format!(
            "Constant '{current_name}' cannot reference same-file constant '{target_name}' before it is declared."
        ),
        reference.location.clone(),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        "Header Constant Dependency Resolution".into(),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        "Move the referenced constant above this declaration, or import it from another file."
            .into(),
    );
    error
}
