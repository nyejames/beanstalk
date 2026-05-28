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
use crate::compiler_frontend::datatypes::{BuiltinScalarReceiver, ReceiverKey};
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
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
    pub(crate) exported: bool,
    pub(crate) receiver_mutable: bool,
    pub(crate) signature: FunctionSignature,
}

/// Receiver-method lookup tables used by parser and diagnostics.
#[derive(Clone, Default)]
pub(crate) struct ReceiverMethodCatalog {
    pub(crate) by_receiver_and_name: FxHashMap<(ReceiverKey, StringId), ReceiverMethodEntry>,
    pub(crate) by_method_name: FxHashMap<StringId, Vec<ReceiverMethodEntry>>,
    /// Maps canonical function path to its receiver-method entry.
    /// WHY: file-local import visibility resolves by function path (to handle aliases),
    ///      so a path-keyed index lets lookups avoid scanning the whole catalog.
    pub(crate) by_function_path: FxHashMap<InternedPath, ReceiverMethodEntry>,
}

/// Render a human-readable receiver name for diagnostics.
pub(crate) fn receiver_kind_label(receiver: &ReceiverKey, string_table: &StringTable) -> String {
    match receiver {
        ReceiverKey::Struct(path) => path.to_string(string_table),
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

/// Collect receiver methods and validate receiver/member naming constraints.
pub(crate) fn build_receiver_method_catalog(
    sorted_headers: &[Header],
    resolved_function_signatures_by_path: &FxHashMap<InternedPath, ResolvedFunctionSignature>,
    struct_fields_by_path: &FxHashMap<InternedPath, Vec<Declaration>>,
    struct_source_by_path: &FxHashMap<InternedPath, InternedPath>,
    source_file_by_symbol_path: &FxHashMap<InternedPath, InternedPath>,
    string_table: &StringTable,
) -> Result<ReceiverMethodCatalog, ReceiverMethodCatalogError> {
    // WHAT: materializes receiver methods into lookup tables keyed by receiver/name and by name.
    // WHY: parser diagnostics and dot-call lowering both need stable, deterministic method lookup
    // without scanning declaration vectors at call sites.
    let mut catalog = ReceiverMethodCatalog::default();

    // ----------------------------
    //  Filter function headers with receivers
    // ----------------------------
    for header in sorted_headers {
        let HeaderKind::Function { .. } = &header.kind else {
            continue;
        };

        let Some(resolved_signature) =
            resolved_function_signatures_by_path.get(&header.tokens.src_path)
        else {
            continue;
        };

        let Some(receiver) = resolved_signature.receiver.as_ref() else {
            continue;
        };

        let Some(method_name) = header.tokens.src_path.name() else {
            continue;
        };

        let Some(method_source_file) = source_file_by_symbol_path
            .get(&header.tokens.src_path)
            .cloned()
        else {
            return Err(CompilerError::compiler_error(format!(
                "Receiver method '{}' is missing canonical source-file metadata.",
                header.tokens.src_path.to_string(string_table)
            ))
            .into());
        };

        // ----------------------------
        //  Validate struct receiver constraints
        // ----------------------------
        if let ReceiverKey::Struct(struct_path) = receiver {
            let Some(struct_source_file) = struct_source_by_path.get(struct_path) else {
                return Err(CompilerDiagnostic::invalid_receiver_declaration(
                    InvalidReceiverDeclarationReason::UnknownStructTarget,
                    header.name_location.clone(),
                )
                .into());
            };

            if *struct_source_file != method_source_file {
                return Err(CompilerDiagnostic::invalid_receiver_declaration(
                    InvalidReceiverDeclarationReason::WrongSourceFile,
                    header.name_location.clone(),
                )
                .into());
            }

            if struct_fields_by_path
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
        }

        // ----------------------------
        //  Check for duplicate receiver methods
        // ----------------------------
        let key = (receiver.to_owned(), method_name);
        if catalog.by_receiver_and_name.contains_key(&key) {
            return Err(CompilerDiagnostic::invalid_receiver_declaration(
                InvalidReceiverDeclarationReason::DuplicateMethod,
                header.name_location.clone(),
            )
            .into());
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
            exported: true,
            receiver_mutable,
            signature: resolved_signature.signature.to_owned(),
        };

        catalog.by_receiver_and_name.insert(key, entry.to_owned());
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
                .to_string(string_table)
                .cmp(&right.function_path.to_string(string_table))
                .then_with(|| left.exported.cmp(&right.exported))
        });
    }

    Ok(catalog)
}
