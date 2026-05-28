//! AST place-shape helpers shared by parser and call validation.
//!
//! WHAT: classifies AST nodes as readable/writable places.
//! WHY: receiver-method parsing, builtin member parsing, and assignment/call validation all
//! enforce the same place rules, so one helper module keeps diagnostics and semantics aligned.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;

/// Returns true when the node resolves to a valid place expression.
pub(crate) fn ast_node_is_place(node: &AstNode) -> bool {
    match &node.kind {
        NodeKind::Rvalue(expression) => {
            matches!(expression.kind, ExpressionKind::Reference(_))
        }

        NodeKind::FieldAccess { base, .. } => ast_node_is_place(base),

        _ => false,
    }
}

/// Returns true when the node resolves to a mutable place expression.
pub(crate) fn ast_node_is_mutable_place(node: &AstNode) -> bool {
    match &node.kind {
        NodeKind::Rvalue(expression) => {
            matches!(expression.kind, ExpressionKind::Reference(_))
                && expression.value_mode.is_mutable()
        }

        NodeKind::FieldAccess { base, .. } => ast_node_is_mutable_place(base),

        _ => false,
    }
}
