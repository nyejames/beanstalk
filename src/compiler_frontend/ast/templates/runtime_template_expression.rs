//! Runtime-template expression unwrapping shared by AST/TIR migration paths.
//!
//! WHAT: exposes narrow helpers for finding a template expression through
//! string-boundary coercions and single-operand runtime wrappers.
//! WHY: HIR already treats these wrappers as transparent when appending runtime
//! templates. AST/TIR conversion and finalization must recognize the same shape
//! so nested runtime slot applications are materialized as owned child-template
//! handoffs before HIR sees them.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::expression_rpn::ExpressionRpnItem;
use crate::compiler_frontend::ast::templates::template::Template;

pub(crate) fn runtime_template_expression(expression: &Expression) -> Option<&Template> {
    match &expression.kind {
        ExpressionKind::Template(template) => Some(template),

        ExpressionKind::Coerced { value, .. } => runtime_template_expression(value),

        ExpressionKind::Runtime(rpn) if rpn.items.len() == 1 => match &rpn.items[0] {
            ExpressionRpnItem::Operand(expression) => runtime_template_expression(expression),
            ExpressionRpnItem::Operator { .. } => None,
        },

        _ => None,
    }
}
