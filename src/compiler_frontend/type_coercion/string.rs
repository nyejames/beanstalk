//! String coercion policy for the Beanstalk compiler frontend.
//!
//! WHAT: defines what expression types are renderable as string content and
//! provides the coercion logic used at template boundaries.
//! WHY: previously, the rules for "what can become a string in a template"
//! were inlined directly into `template_folding.rs`. Moving them here makes
//! the policy explicit and reusable without touching template mechanics.

use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::symbols::string_interning::StringTable;

/// Attempts to coerce a constant expression kind to its string representation
/// for use in template folding.
///
/// WHAT: converts compile-time scalar and template expression kinds to their
/// string content. Returns `None` for expression kinds that cannot be folded
/// into a string at compile time.
/// WHY: centralises the "what can fold to a string" decision that was
/// previously inlined in `template_folding::fold_plan`.
pub(crate) fn fold_expression_kind_to_string(
    kind: &ExpressionKind,
    string_table: &StringTable,
) -> Option<FoldedStringPiece> {
    match kind {
        ExpressionKind::StringSlice(string) => Some(FoldedStringPiece::Text(
            string_table.resolve(*string).to_owned(),
        )),
        ExpressionKind::Float(value) => Some(FoldedStringPiece::Text(value.to_string())),
        ExpressionKind::Int(value) => Some(FoldedStringPiece::Text(value.to_string())),
        ExpressionKind::Bool(value) => Some(FoldedStringPiece::Text(value.to_string())),
        ExpressionKind::Char(value) => Some(FoldedStringPiece::Char(*value)),
        ExpressionKind::Template(template) => {
            if matches!(template.kind, TemplateType::Comment(_)) {
                Some(FoldedStringPiece::Skip)
            } else {
                Some(FoldedStringPiece::NestedTemplate)
            }
        }
        _ => None,
    }
}

/// The result of folding an expression kind into string content.
///
/// WHAT: discriminates between the different outcomes of compile-time string
/// coercion so callers can handle each case appropriately.
pub(crate) enum FoldedStringPiece {
    /// A plain text fragment that can be appended directly.
    Text(String),
    /// A single character to push onto the string buffer.
    Char(char),
    /// A nested template that must be recursively folded by the caller.
    NestedTemplate,
    /// Content that should be silently dropped (comments, omitted slots).
    Skip,
}

#[cfg(test)]
#[path = "tests/string_tests.rs"]
mod string_tests;
