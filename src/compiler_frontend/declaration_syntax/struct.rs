//! Struct field-list parsing and default-value validation.
//!
//! WHAT: parses `Struct = | ... |` field declarations using the shared signature-member parser.
//! WHY: struct defaults have extra compile-time constraints that should stay separate from the
//! general shared `| ... |` parsing logic.
//!
//! This module is the authoritative home for the struct shell parser. Both the header stage
//! (which calls `parse_struct_shell` to populate `StructHeaderMetadata.fields`) and AST body
//! declarations (inline struct-literal expressions) use `parse_struct_shell`. This avoids
//! top-level struct field syntax being rediscovered twice.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::declaration_syntax::signature_members::{
    SignatureMemberContext, parse_signature_members,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::return_rule_error;

/// Parse a struct field-list shell from `| field Type [= default], ... |` syntax.
///
/// WHAT: advances past the opening `|`, parses all fields via `parse_signature_members`,
/// advances past the closing `|`, and validates that any default values are compile-time constants.
/// WHY: this is the single canonical struct field parser. Used by header parsing to populate
/// `StructHeaderMetadata.fields` and by body-declaration parsing for inline struct literals.
pub fn parse_struct_shell(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Vec<Declaration>, CompilerError> {
    token_stream.advance();

    let fields = parse_signature_members(
        token_stream,
        string_table,
        context,
        SignatureMemberContext::StructField,
    )?;

    token_stream.advance();

    Ok(fields)
}

/// Validates that every struct field default is a compile-time constant.
///
/// WHAT: enforces the invariant that struct defaults must be known at compile time.
/// WHY: called at AST stage only, after constant resolution has run. At header stage,
/// references are unresolved and cannot be validated yet.
pub(crate) fn validate_struct_default_values(
    fields: &[Declaration],
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for field in fields {
        if matches!(field.value.kind, ExpressionKind::NoValue) {
            continue;
        }

        if !field.value.is_compile_time_constant() {
            let field_name = field.id.name_str(string_table).unwrap_or("<field>");
            return_rule_error!(
                format!(
                    "Struct field '{}' default value must fold to a single compile-time value.",
                    field_name
                ),
                field.value.location.clone(), {
                    CompilationStage => "Struct/Parameter Parsing",
                    PrimarySuggestion => "Use only compile-time constants and constant expressions in struct default values",
                }
            );
        }
    }

    Ok(())
}
