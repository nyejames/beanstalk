//! Shared type-diagnostic text helpers.
//!
//! WHAT: centralizes common expected/found formatting and shared numeric-mix
//! hint wording.
//! WHY: parser/evaluator/call-validation sites previously duplicated message
//! fragments that should stay consistent.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(crate) const NUMERIC_MIX_HINT: &str =
    "Only Int + Float and Float + Int mix numeric types implicitly.";

pub(crate) fn expected_found_clause(
    expected: &DataType,
    found: &DataType,
    string_table: &StringTable,
) -> String {
    format!(
        "Expected '{}', but found '{}'.",
        expected.display_with_table(string_table),
        found.display_with_table(string_table)
    )
}

pub(crate) fn argument_conversion_hint(expected: &DataType, found: &DataType) -> &'static str {
    if matches!((expected, found), (DataType::Float, DataType::Int)) {
        NUMERIC_MIX_HINT
    } else {
        "Convert the argument to the expected type."
    }
}

pub(crate) fn should_report_regular_division_int_context(
    expected: &DataType,
    found: &DataType,
    expression: &Expression,
) -> bool {
    matches!((expected, found), (DataType::Int, DataType::Float))
        && expression.contains_regular_division
}

pub(crate) fn regular_division_int_context_guidance() -> &'static str {
    "Regular division returns 'Float'. Use '//' for integer division. Use 'Int(...)' for an explicit conversion."
}

/// Renders a compact user-facing value snippet for type diagnostics.
///
/// WHAT: extracts short literal/reference-first previews from expressions.
/// WHY: mismatch diagnostics are easier to act on when they name the concrete
/// value where practical, but should still remain stable for complex
/// expressions that cannot be rendered directly.
pub(crate) fn offending_value_snippet(
    expression: &Expression,
    string_table: &StringTable,
) -> String {
    match &expression.kind {
        ExpressionKind::Int(value) => value.to_string(),
        ExpressionKind::Float(value) => value.to_string(),
        ExpressionKind::Bool(value) => value.to_string(),
        ExpressionKind::Char(value) => format!("'{}'", value.escape_default()),
        ExpressionKind::StringSlice(value) => {
            let rendered = truncate_diagnostic_text(string_table.resolve(*value), 24);
            format!("\"{rendered}\"")
        }
        ExpressionKind::Reference(path) => {
            let name = path.name_str(string_table).unwrap_or("<value>");
            format!("'{name}'")
        }
        ExpressionKind::OptionNone => String::from("none"),
        ExpressionKind::Template(_) => String::from("<template>"),
        ExpressionKind::Collection(_) => String::from("<collection>"),
        ExpressionKind::StructInstance(_) => String::from("<struct instance>"),
        ExpressionKind::FunctionCall(path, _) => {
            let name = path.name_str(string_table).unwrap_or("<function>");
            format!("{name}(...)")
        }
        ExpressionKind::ResultHandledFunctionCall { name, .. } => {
            let call_name = name.name_str(string_table).unwrap_or("<function>");
            format!("{call_name}(...)[handled]")
        }
        ExpressionKind::HostFunctionCall(path, _) => {
            let name = path.name_str(string_table).unwrap_or("<host function>");
            format!("{name}(...)")
        }
        ExpressionKind::BuiltinCast { kind, .. } => match kind {
            crate::compiler_frontend::ast::expressions::expression::BuiltinCastKind::Int => {
                String::from("Int(...)")
            }
            crate::compiler_frontend::ast::expressions::expression::BuiltinCastKind::Float => {
                String::from("Float(...)")
            }
        },
        ExpressionKind::Coerced { value, .. } => offending_value_snippet(value, string_table),
        _ => String::from("<expression>"),
    }
}

pub(crate) fn offending_value_clause(
    expression: &Expression,
    string_table: &StringTable,
) -> String {
    format!(
        "Offending value: {}.",
        offending_value_snippet(expression, string_table)
    )
}

fn truncate_diagnostic_text(value: &str, max_chars: usize) -> String {
    let mut collected = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        collected.push_str("...");
    }
    collected
}
