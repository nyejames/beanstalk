//! Shared receiver-method diagnostics and formatting helpers.
//!
//! WHAT: centralizes receiver-method error construction and receiver-kind display strings.
//! WHY: parser entrypoints report the same receiver-method misuse errors, so one helper keeps
//! diagnostics deterministic and avoids drift in wording/metadata.

use crate::compiler_frontend::ast::module_ast::ReceiverMethodEntry;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::datatypes::{BuiltinScalarReceiver, ReceiverKey};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

pub(crate) fn receiver_kind_label(receiver: &ReceiverKey, string_table: &StringTable) -> String {
    match receiver {
        ReceiverKey::Struct(path) => path.to_string(string_table),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Int) => String::from("Int"),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Float) => String::from("Float"),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::Bool) => String::from("Bool"),
        ReceiverKey::BuiltinScalar(BuiltinScalarReceiver::String) => String::from("String"),
    }
}

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
