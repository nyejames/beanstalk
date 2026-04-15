//! Result-handler semantic validation helpers.
//!
//! WHAT: validates handler bindings, name conflicts, and value-required fallthrough rules.
//! WHY: parser entrypoints should stay focused on syntax while this module owns semantic checks
//! shared by call and expression result handling.

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::string_interning::StringId;
use crate::compiler_frontend::symbols::identifier_policy::{
    IdentifierNamingKind, ensure_not_keyword_shadow_identifier, naming_warning_for_identifier,
};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_rule_error;

use super::termination::body_guarantees_termination;

pub(super) fn validate_named_result_handler_binding(
    handler_name: &str,
    location: SourceLocation,
    compilation_stage: &str,
    warnings: &mut Vec<CompilerWarning>,
) -> Result<(), CompilerError> {
    ensure_not_keyword_shadow_identifier(handler_name, location.to_owned(), compilation_stage)?;

    if let Some(warning) =
        naming_warning_for_identifier(handler_name, location, IdentifierNamingKind::ValueLike)
    {
        warnings.push(warning);
    }

    Ok(())
}

pub(super) fn validate_named_result_handler_conflict(
    context: &ScopeContext,
    handler_name: StringId,
    handler_name_text: &str,
    location: SourceLocation,
    compilation_stage: &str,
) -> Result<(), CompilerError> {
    if context.get_reference(&handler_name).is_some() {
        return_rule_error!(
            format!(
                "Named handler '{}' conflicts with an existing visible declaration.",
                handler_name_text
            ),
            location,
            {
                CompilationStage => compilation_stage,
                PrimarySuggestion => "Choose a unique handler variable name",
            }
        );
    }

    Ok(())
}

pub(super) fn validate_named_result_handler_value_requirement(
    fallback: &Option<Vec<Expression>>,
    value_required: bool,
    success_result_types: &[DataType],
    handler_body: &[AstNode],
    location: SourceLocation,
    compilation_stage: &str,
) -> Result<(), CompilerError> {
    // WHAT: rejects handler bodies that can simply fall through when the call expression must
    // still produce success values for the surrounding statement/expression.
    // WHY: without fallback values, a fallthrough path would leave the handled call with no value
    // continuation to merge back into.
    if fallback.is_none()
        && value_required
        && !success_result_types.is_empty()
        && !body_guarantees_termination(handler_body)
    {
        return_rule_error!(
            "Named handler without fallback can fall through while success values are required.",
            location,
            {
                CompilationStage => compilation_stage,
                PrimarySuggestion => "Add fallback values before ':' or make the handler body terminate with return/return!",
            }
        );
    }

    Ok(())
}
