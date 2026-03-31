//! Shared receiver-method diagnostics and formatting helpers.
//!
//! WHAT: centralizes receiver-method error construction and receiver-kind display strings.
//! WHY: parser entrypoints report the same receiver-method misuse errors, so one helper keeps
//! diagnostics deterministic and avoids drift in wording/metadata.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::type_resolution::ResolvedFunctionSignature;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::datatypes::{BuiltinScalarReceiver, ReceiverKey};
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_rule_error;
use rustc_hash::FxHashMap;

#[derive(Clone)]
/// Canonical metadata for one receiver method declaration.
pub(crate) struct ReceiverMethodEntry {
    pub(crate) function_path: InternedPath,
    pub(crate) receiver: ReceiverKey,
    pub(crate) source_file: InternedPath,
    pub(crate) exported: bool,
    pub(crate) receiver_mutable: bool,
    pub(crate) signature: FunctionSignature,
}

#[derive(Clone, Default)]
/// Receiver-method lookup tables used by parser and diagnostics.
pub(crate) struct ReceiverMethodCatalog {
    pub(crate) by_receiver_and_name: FxHashMap<(ReceiverKey, StringId), ReceiverMethodEntry>,
    pub(crate) by_method_name: FxHashMap<StringId, Vec<ReceiverMethodEntry>>,
}

/// Render a human-readable receiver name for diagnostics.
pub(crate) fn receiver_kind_label(receiver: &ReceiverKey, string_table: &StringTable) -> String {
    match receiver {
        ReceiverKey::Struct(path) => path.to_string(string_table),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Int) => String::from("Int"),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Float) => String::from("Float"),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Bool) => String::from("Bool"),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::String) => String::from("String"),
    }
}

/// Build a diagnostic for free-function style calls to receiver-only methods.
pub(crate) fn free_function_receiver_method_call_error(
    method_name: StringId,
    method_entry: &ReceiverMethodEntry,
    location: SourceLocation,
    compilation_stage: &str,
    string_table: &StringTable,
) -> CompilerError {
    let mut error = CompilerError::new_rule_error(
        format!(
            "'{}' is a receiver method for '{}' and cannot be called as a free function.",
            string_table.resolve(method_name),
            receiver_kind_label(&method_entry.receiver, string_table)
        ),
        location,
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        compilation_stage.to_owned(),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        String::from("Call the method with receiver syntax like 'value.method(...)' instead of 'method(value, ...)'"),
    );
    error
}

/// Collect receiver methods and validate receiver/member naming constraints.
pub(crate) fn build_receiver_method_catalog(
    sorted_headers: &[Header],
    resolved_function_signatures_by_path: &FxHashMap<InternedPath, ResolvedFunctionSignature>,
    struct_fields_by_path: &FxHashMap<InternedPath, Vec<Declaration>>,
    struct_source_by_path: &FxHashMap<InternedPath, InternedPath>,
    source_file_by_symbol_path: &FxHashMap<InternedPath, InternedPath>,
    string_table: &StringTable,
) -> Result<ReceiverMethodCatalog, CompilerError> {
    // WHAT: materializes receiver methods into lookup tables keyed by receiver/name and by name.
    // WHY: parser diagnostics and dot-call lowering both need stable, deterministic method lookup
    // without scanning declaration vectors at call sites.
    let mut catalog = ReceiverMethodCatalog::default();

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
            return_rule_error!(
                format!(
                    "Receiver method '{}' is missing canonical source-file metadata.",
                    header.tokens.src_path.to_string(string_table)
                ),
                header.name_location.clone(),
                {
                    CompilationStage => "AST Construction",
                }
            );
        };

        if let ReceiverKey::Struct(struct_path) = receiver {
            let Some(struct_source_file) = struct_source_by_path.get(struct_path) else {
                return_rule_error!(
                    format!(
                        "Receiver method '{}' targets unknown struct '{}'.",
                        header.tokens.src_path.to_string(string_table),
                        struct_path.to_string(string_table)
                    ),
                    header.name_location.clone(),
                    {
                        CompilationStage => "AST Construction",
                    }
                );
            };

            if *struct_source_file != method_source_file {
                return_rule_error!(
                    format!(
                        "Method '{}' for struct '{}' must be declared in the same file as the struct definition.",
                        string_table.resolve(method_name),
                        struct_path.to_string(string_table)
                    ),
                    header.name_location.clone(),
                    {
                        CompilationStage => "AST Construction",
                        PrimarySuggestion => "Move the method into the struct's defining file",
                    }
                );
            }

            if struct_fields_by_path
                .get(struct_path)
                .is_some_and(|fields| {
                    fields
                        .iter()
                        .any(|field| field.id.name() == Some(method_name))
                })
            {
                return_rule_error!(
                    format!(
                        "Struct '{}' declares both a field and method named '{}'.",
                        struct_path.to_string(string_table),
                        string_table.resolve(method_name)
                    ),
                    header.name_location.clone(),
                    {
                        CompilationStage => "AST Construction",
                        PrimarySuggestion => "Rename the field or method so receiver members stay unambiguous",
                    }
                );
            }
        }

        let key = (receiver.to_owned(), method_name);
        if catalog.by_receiver_and_name.contains_key(&key) {
            return_rule_error!(
                format!(
                    "Duplicate receiver method '{}' for receiver '{}'.",
                    string_table.resolve(method_name),
                    receiver_kind_label(receiver, string_table)
                ),
                header.name_location.clone(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Keep exactly one method with a given receiver and name in the module",
                }
            );
        }

        let entry = ReceiverMethodEntry {
            function_path: header.tokens.src_path.to_owned(),
            receiver: receiver.to_owned(),
            source_file: method_source_file,
            exported: header.exported,
            receiver_mutable: resolved_signature
                .signature
                .parameters
                .first()
                .is_some_and(|parameter| parameter.value.ownership.is_mutable()),
            signature: resolved_signature.signature.to_owned(),
        };

        catalog.by_receiver_and_name.insert(key, entry.to_owned());
        catalog
            .by_method_name
            .entry(method_name)
            .or_default()
            .push(entry);
    }

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
