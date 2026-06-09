//! Shared receiver-method diagnostics and formatting helpers.
//!
//! WHAT: centralizes receiver-method error construction and receiver-kind display strings.
//! WHY: parser entrypoints report the same receiver-method misuse errors, so one helper keeps
//! diagnostics deterministic and avoids drift in wording/metadata.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::type_resolution::ResolvedFunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidReceiverCallReason, InvalidReceiverDeclarationReason,
};
use crate::compiler_frontend::datatypes::{BuiltinScalarReceiver, DataType, ReceiverKey};
use crate::compiler_frontend::external_packages::{
    ExternalPackageRegistry, ExternalSignatureType, ExternalSymbolId, ExternalTypeId,
};
use crate::compiler_frontend::headers::import_environment::{FileVisibility, NamespaceTypeMember};
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use rustc_hash::FxHashMap;

pub(crate) enum ReceiverMethodCatalogError {
    Diagnostic(CompilerDiagnostic),
    Infrastructure(CompilerError),
}

impl From<CompilerDiagnostic> for ReceiverMethodCatalogError {
    fn from(diagnostic: CompilerDiagnostic) -> Self {
        ReceiverMethodCatalogError::Diagnostic(diagnostic)
    }
}

impl From<CompilerError> for ReceiverMethodCatalogError {
    fn from(error: CompilerError) -> Self {
        ReceiverMethodCatalogError::Infrastructure(error)
    }
}

/// Canonical metadata for one receiver method declaration.
#[derive(Clone)]
pub(crate) struct ReceiverMethodEntry {
    pub(crate) function_path: InternedPath,
    pub(crate) receiver: ReceiverKey,
    pub(crate) source_file: InternedPath,
    pub(crate) visibility_source_file: InternedPath,
    pub(crate) kind: ReceiverMethodKind,
    pub(crate) exported: bool,
    pub(crate) receiver_mutable: bool,
    pub(crate) signature: FunctionSignature,
}

/// How a receiver method relates to the receiver type declaration.
///
/// WHAT: canonical methods belong to the receiver type's declaring file and keep the existing
/// import/export behavior. File-local extensions target imported, external, or builtin scalar
/// receiver types and are visible only in their declaring file.
/// WHY: trait evidence needs to distinguish reusable receiver evidence from local extension
/// evidence without adding a second method catalog. User-authored builtin scalar extensions are
/// file-local because the compiler owns the canonical builtin surface; compiler-owned builtin
/// methods are registered outside the user header catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReceiverMethodKind {
    Canonical,
    FileLocalExtension,
}

/// Receiver-method lookup tables used by parser and diagnostics.
#[derive(Clone, Default)]
pub(crate) struct ReceiverMethodCatalog {
    pub(crate) by_receiver_and_name: FxHashMap<(ReceiverKey, StringId), Vec<ReceiverMethodEntry>>,
    pub(crate) by_method_name: FxHashMap<StringId, Vec<ReceiverMethodEntry>>,
    /// Maps canonical function path to its receiver-method entry.
    /// WHY: file-local import visibility resolves by function path (to handle aliases),
    ///      so a path-keyed index lets lookups avoid scanning the whole catalog.
    pub(crate) by_function_path: FxHashMap<InternedPath, ReceiverMethodEntry>,
}

/// Inputs required to build the receiver-method catalog.
///
/// WHAT: groups the resolved signature, source ownership, and file-visibility side tables used
/// while classifying receiver methods.
/// WHY: catalog construction sits at the join between header visibility and AST semantic typing;
/// keeping the inputs named makes that boundary easier to audit than a long positional list.
pub(crate) struct BuildReceiverMethodCatalogInput<'a> {
    pub(crate) sorted_headers: &'a [Header],
    pub(crate) resolved_function_signatures_by_path:
        &'a FxHashMap<InternedPath, ResolvedFunctionSignature>,
    pub(crate) struct_fields_by_path: &'a FxHashMap<InternedPath, Vec<Declaration>>,
    pub(crate) struct_source_by_path: &'a FxHashMap<InternedPath, InternedPath>,
    pub(crate) choice_source_by_path: &'a FxHashMap<InternedPath, InternedPath>,
    pub(crate) source_file_by_symbol_path: &'a FxHashMap<InternedPath, InternedPath>,
    pub(crate) file_visibility_by_source: &'a FxHashMap<InternedPath, FileVisibility>,
    pub(crate) resolved_type_aliases_by_path: &'a FxHashMap<InternedPath, DataType>,
    pub(crate) external_package_registry: &'a ExternalPackageRegistry,
    pub(crate) string_table: &'a StringTable,
}

/// Render a human-readable receiver name for diagnostics.
pub(crate) fn receiver_kind_label(receiver: &ReceiverKey, string_table: &StringTable) -> String {
    match receiver {
        ReceiverKey::Struct(path) | ReceiverKey::Choice(path) => path.to_string(string_table),
        ReceiverKey::External(type_id) => format!("External({})", type_id.0),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Int) => String::from("Int"),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Float) => String::from("Float"),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Bool) => String::from("Bool"),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::String) => String::from("String"),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Char) => String::from("Char"),
    }
}

/// Build a diagnostic for free-function style calls to receiver-only methods.
pub(crate) fn free_function_receiver_method_call_error(
    method_name: StringId,
    method_entry: &ReceiverMethodEntry,
    location: SourceLocation,
    string_table: &mut StringTable,
) -> CompilerDiagnostic {
    let receiver_label = receiver_kind_label(&method_entry.receiver, string_table);
    let receiver_type_id = string_table.intern(&receiver_label);

    CompilerDiagnostic::invalid_receiver_call(
        InvalidReceiverCallReason::CalledAsFreeFunction,
        Some(receiver_type_id),
        Some(method_name),
        location,
    )
}

fn source_receiver_type_is_visible(
    file_visibility: &FileVisibility,
    receiver_path: &InternedPath,
    resolved_type_aliases_by_path: &FxHashMap<InternedPath, DataType>,
) -> bool {
    if file_visibility
        .visible_declaration_paths
        .contains(receiver_path)
    {
        return true;
    }

    file_visibility
        .visible_type_alias_names
        .values()
        .any(|alias_path| {
            source_alias_matches_receiver(alias_path, receiver_path, resolved_type_aliases_by_path)
        })
        || file_visibility
            .visible_namespace_records
            .values()
            .any(|record| {
                record.type_members.values().any(|member| {
                    matches!(
                        member,
                        NamespaceTypeMember::SourceDeclaration(type_path)
                        if source_alias_matches_receiver(
                            type_path,
                            receiver_path,
                            resolved_type_aliases_by_path,
                        )
                    )
                })
            })
}

fn source_alias_matches_receiver(
    type_path: &InternedPath,
    receiver_path: &InternedPath,
    resolved_type_aliases_by_path: &FxHashMap<InternedPath, DataType>,
) -> bool {
    if type_path == receiver_path {
        return true;
    }

    matches!(
        resolved_type_aliases_by_path.get(type_path),
        Some(DataType::Struct {
            nominal_path,
            const_record: false,
            ..
        }) | Some(DataType::Choices {
            nominal_path,
            ..
        }) if nominal_path == receiver_path
    )
}

fn external_receiver_type_is_visible(
    file_visibility: &FileVisibility,
    receiver_type_id: ExternalTypeId,
    resolved_type_aliases_by_path: &FxHashMap<InternedPath, DataType>,
) -> bool {
    if file_visibility.visible_external_symbols.values().any(
        |symbol_id| matches!(symbol_id, ExternalSymbolId::Type(type_id) if *type_id == receiver_type_id),
    ) {
        return true;
    }

    if file_visibility
        .visible_type_alias_names
        .values()
        .any(|alias_path| {
            matches!(
                resolved_type_aliases_by_path.get(alias_path),
                Some(DataType::External { type_id }) if *type_id == receiver_type_id
            )
        })
    {
        return true;
    }

    file_visibility
        .visible_namespace_records
        .values()
        .any(|record| {
            record.type_members.values().any(|member| {
                matches!(
                    member,
                    NamespaceTypeMember::ExternalSymbol(ExternalSymbolId::Type(type_id))
                    if *type_id == receiver_type_id
                )
            })
        })
}

pub(crate) fn receiver_type_is_visible_in_file(
    receiver: &ReceiverKey,
    file_visibility: &FileVisibility,
    resolved_type_aliases_by_path: &FxHashMap<InternedPath, DataType>,
) -> bool {
    match receiver {
        ReceiverKey::Struct(path) | ReceiverKey::Choice(path) => {
            source_receiver_type_is_visible(file_visibility, path, resolved_type_aliases_by_path)
        }
        ReceiverKey::External(type_id) => external_receiver_type_is_visible(
            file_visibility,
            *type_id,
            resolved_type_aliases_by_path,
        ),
        ReceiverKey::BuiltinScalar(_) => true,
    }
}

fn receiver_method_path_is_visible(
    function_path: &InternedPath,
    method_name: StringId,
    file_visibility: &FileVisibility,
) -> bool {
    file_visibility
        .visible_receiver_methods
        .get(&method_name)
        .is_some_and(|methods| {
            methods
                .iter()
                .any(|method| method.function_path == *function_path)
        })
}

fn external_signature_matches_receiver(
    receiver: &ReceiverKey,
    signature_type: &ExternalSignatureType,
) -> bool {
    matches!(
        (receiver, signature_type),
        (ReceiverKey::External(receiver_id), ExternalSignatureType::External(expected_id))
        if receiver_id == expected_id
    )
}

fn visible_external_receiver_method_matches(
    receiver: &ReceiverKey,
    method_name: StringId,
    file_visibility: &FileVisibility,
    external_package_registry: &ExternalPackageRegistry,
) -> bool {
    file_visibility
        .visible_external_receiver_methods
        .get(&method_name)
        .is_some_and(|methods| {
            methods.iter().any(|function_id| {
                external_package_registry
                    .get_function_by_id(*function_id)
                    .and_then(|function| function.receiver_type.as_ref())
                    .is_some_and(|receiver_type| {
                        external_signature_matches_receiver(receiver, receiver_type)
                    })
            })
        })
}

fn receiver_method_kind_for_declaration(
    receiver: &ReceiverKey,
    method_source_file: &InternedPath,
    method_file_visibility: &FileVisibility,
    struct_source_by_path: &FxHashMap<InternedPath, InternedPath>,
    choice_source_by_path: &FxHashMap<InternedPath, InternedPath>,
    resolved_type_aliases_by_path: &FxHashMap<InternedPath, DataType>,
    location: SourceLocation,
) -> Result<ReceiverMethodKind, CompilerDiagnostic> {
    match receiver {
        ReceiverKey::Struct(struct_path) => {
            let Some(struct_source_file) = struct_source_by_path.get(struct_path) else {
                return Err(CompilerDiagnostic::invalid_receiver_declaration(
                    InvalidReceiverDeclarationReason::UnknownStructTarget,
                    location,
                ));
            };

            if struct_source_file == method_source_file {
                return Ok(ReceiverMethodKind::Canonical);
            }
        }

        ReceiverKey::Choice(choice_path) => {
            let Some(choice_source_file) = choice_source_by_path.get(choice_path) else {
                return Err(CompilerDiagnostic::invalid_receiver_declaration(
                    InvalidReceiverDeclarationReason::UnknownStructTarget,
                    location,
                ));
            };

            if choice_source_file == method_source_file {
                return Ok(ReceiverMethodKind::Canonical);
            }
        }

        ReceiverKey::External(_) => {}

        // User-authored builtin scalar receiver methods are file-local extensions.
        // Compiler-owned builtin methods are not present in `sorted_headers`, so any
        // `BuiltinScalar` method seen here is user-authored and should be file-local.
        ReceiverKey::BuiltinScalar(_) => {}
    }

    if receiver_type_is_visible_in_file(
        receiver,
        method_file_visibility,
        resolved_type_aliases_by_path,
    ) {
        Ok(ReceiverMethodKind::FileLocalExtension)
    } else {
        Err(CompilerDiagnostic::invalid_receiver_declaration(
            InvalidReceiverDeclarationReason::ReceiverTypeNotVisible,
            location,
        ))
    }
}

/// Collect receiver methods and validate receiver/member naming constraints.
pub(crate) fn build_receiver_method_catalog(
    input: BuildReceiverMethodCatalogInput<'_>,
) -> Result<ReceiverMethodCatalog, ReceiverMethodCatalogError> {
    // WHAT: materializes receiver methods into lookup tables keyed by receiver/name and by name.
    // WHY: parser diagnostics and dot-call lowering both need stable, deterministic method lookup
    // without scanning declaration vectors at call sites.
    let mut catalog = ReceiverMethodCatalog::default();

    // ----------------------------
    //  Filter function headers with receivers
    // ----------------------------
    for header in input.sorted_headers {
        let HeaderKind::Function { .. } = &header.kind else {
            continue;
        };

        let Some(resolved_signature) = input
            .resolved_function_signatures_by_path
            .get(&header.tokens.src_path)
        else {
            continue;
        };

        let Some(receiver) = resolved_signature.receiver.as_ref() else {
            continue;
        };

        let Some(method_name) = header.tokens.src_path.name() else {
            continue;
        };

        let Some(method_source_file) = input
            .source_file_by_symbol_path
            .get(&header.tokens.src_path)
            .cloned()
        else {
            return Err(CompilerError::compiler_error(format!(
                "Receiver method '{}' is missing canonical source-file metadata.",
                header.tokens.src_path.to_string(input.string_table)
            ))
            .into());
        };

        let Some(method_file_visibility) = input.file_visibility_by_source.get(&header.source_file)
        else {
            return Err(CompilerError::compiler_error(format!(
                "Receiver method '{}' is missing file visibility metadata.",
                header.tokens.src_path.to_string(input.string_table)
            ))
            .into());
        };

        let method_kind = receiver_method_kind_for_declaration(
            receiver,
            &method_source_file,
            method_file_visibility,
            input.struct_source_by_path,
            input.choice_source_by_path,
            input.resolved_type_aliases_by_path,
            header.name_location.clone(),
        )?;

        if method_kind == ReceiverMethodKind::FileLocalExtension
            && visible_external_receiver_method_matches(
                receiver,
                method_name,
                method_file_visibility,
                input.external_package_registry,
            )
        {
            return Err(CompilerDiagnostic::invalid_receiver_declaration(
                InvalidReceiverDeclarationReason::ExtensionOverridesCanonicalMethod,
                header.name_location.clone(),
            )
            .into());
        }

        // ----------------------------
        //  Validate canonical struct receiver constraints
        // ----------------------------
        if let ReceiverKey::Struct(struct_path) = receiver
            && method_kind == ReceiverMethodKind::Canonical
            && input
                .struct_fields_by_path
                .get(struct_path)
                .is_some_and(|fields| {
                    fields
                        .iter()
                        .any(|field| field.id.name() == Some(method_name))
                })
        {
            return Err(CompilerDiagnostic::invalid_receiver_declaration(
                InvalidReceiverDeclarationReason::FieldNameConflict,
                header.name_location.clone(),
            )
            .into());
        }

        // ----------------------------
        //  Check for duplicate receiver methods
        // ----------------------------
        let key = (receiver.to_owned(), method_name);
        if let Some(existing_entries) = catalog.by_receiver_and_name.get(&key) {
            for existing_entry in existing_entries {
                if existing_entry.source_file == method_source_file {
                    return Err(CompilerDiagnostic::invalid_receiver_declaration(
                        InvalidReceiverDeclarationReason::DuplicateMethod,
                        header.name_location.clone(),
                    )
                    .into());
                }

                match (existing_entry.kind, method_kind) {
                    (ReceiverMethodKind::Canonical, ReceiverMethodKind::Canonical) => {
                        return Err(CompilerDiagnostic::invalid_receiver_declaration(
                            InvalidReceiverDeclarationReason::DuplicateMethod,
                            header.name_location.clone(),
                        )
                        .into());
                    }

                    (ReceiverMethodKind::Canonical, ReceiverMethodKind::FileLocalExtension)
                        if receiver_method_path_is_visible(
                            &existing_entry.function_path,
                            method_name,
                            method_file_visibility,
                        ) =>
                    {
                        return Err(CompilerDiagnostic::invalid_receiver_declaration(
                            InvalidReceiverDeclarationReason::ExtensionOverridesCanonicalMethod,
                            header.name_location.clone(),
                        )
                        .into());
                    }

                    (ReceiverMethodKind::FileLocalExtension, ReceiverMethodKind::Canonical) => {
                        let Some(extension_visibility) = input
                            .file_visibility_by_source
                            .get(&existing_entry.visibility_source_file)
                        else {
                            continue;
                        };

                        if receiver_method_path_is_visible(
                            &header.tokens.src_path,
                            method_name,
                            extension_visibility,
                        ) {
                            return Err(CompilerDiagnostic::invalid_receiver_declaration(
                                InvalidReceiverDeclarationReason::ExtensionOverridesCanonicalMethod,
                                header.name_location.clone(),
                            )
                            .into());
                        }
                    }

                    _ => {}
                }
            }
        }

        let receiver_mutable = resolved_signature
            .signature
            .parameters
            .first()
            .is_some_and(|parameter| parameter.value.value_mode.is_mutable());

        let entry = ReceiverMethodEntry {
            function_path: header.tokens.src_path.to_owned(),
            receiver: receiver.to_owned(),
            source_file: method_source_file,
            visibility_source_file: header.source_file.to_owned(),
            kind: method_kind,
            exported: method_kind == ReceiverMethodKind::Canonical,
            receiver_mutable,
            signature: resolved_signature.signature.to_owned(),
        };

        catalog
            .by_receiver_and_name
            .entry(key)
            .or_default()
            .push(entry.to_owned());
        catalog
            .by_method_name
            .entry(method_name)
            .or_default()
            .push(entry.to_owned());
        catalog
            .by_function_path
            .insert(header.tokens.src_path.to_owned(), entry);
    }

    // ----------------------------
    //  Sort catalog entries for deterministic lookup
    // ----------------------------
    for entries in catalog.by_method_name.values_mut() {
        entries.sort_by(|left, right| {
            left.function_path
                .to_string(input.string_table)
                .cmp(&right.function_path.to_string(input.string_table))
                .then_with(|| left.exported.cmp(&right.exported))
        });
    }

    Ok(catalog)
}
