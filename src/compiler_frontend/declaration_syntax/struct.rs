//! Struct field-list shell parsing and default-value validation.
//!
//! WHAT: wraps shared record-body parsing for `Struct = | ... |` field declarations.
//! WHY: struct defaults have extra compile-time constraints that should stay separate from the
//! general shared `| ... |` parsing logic.
//!
//! This module is the authoritative home for the struct shell parser. It returns neutral field
//! syntax; AST type resolution later turns that syntax into typed declarations.

#![allow(clippy::result_large_err)]
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, DiagnosticBag};
use crate::compiler_frontend::declaration_syntax::record_body::parse_record_body;
use crate::compiler_frontend::declaration_syntax::signature_members::{
    SignatureMemberContext, SignatureMemberSyntax,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;

/// Parse a struct field-list shell from `| field Type [= default], ... |` syntax.
///
/// WHAT: advances past the opening `|`, parses all fields via `parse_signature_members`,
/// advances past the closing `|`, and validates that any default values are compile-time constants.
/// WHY: this is the single canonical struct field parser. Used by header parsing to populate
/// `StructHeaderMetadata.fields` and by body-declaration parsing for inline struct literals.
pub fn parse_struct_shell(
    token_stream: &mut FileTokens,
    string_table: &mut StringTable,
    warnings: &mut Vec<CompilerDiagnostic>,
    owner_path: &crate::compiler_frontend::interned_path::InternedPath,
) -> Result<Vec<SignatureMemberSyntax>, CompilerDiagnostic> {
    parse_record_body(
        token_stream,
        string_table,
        warnings,
        SignatureMemberContext::StructField,
        owner_path,
    )
}

/// Validates that every struct field default is a compile-time constant.
///
/// WHAT: enforces the invariant that struct defaults must be known at compile time.
/// WHY: called at AST stage only, after constant resolution has run. At header stage,
/// references are unresolved and cannot be validated yet.
pub(crate) fn validate_struct_default_values(fields: &[Declaration]) -> Result<(), DiagnosticBag> {
    let mut bag = DiagnosticBag::new();

    for field in fields {
        if matches!(field.value.kind, ExpressionKind::NoValue) {
            continue;
        }

        if !field.value.is_compile_time_constant() {
            bag.push(CompilerDiagnostic::invalid_struct_default_value(
                field.value.location.clone(),
            ));
        }
    }

    if bag.has_errors() {
        return Err(bag);
    }

    Ok(())
}
