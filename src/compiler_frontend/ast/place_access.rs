//! AST place-shape helpers shared by parser and call validation.
//!
//! WHAT: classifies AST nodes as readable/writable places and renders receiver-place hints.
//! WHY: receiver-method parsing, builtin member parsing, and assignment/call validation all
//! enforce the same place rules, so one helper module keeps diagnostics and semantics aligned.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::builtins::BuiltinMethodKind;
use crate::compiler_frontend::string_interning::StringTable;

/// Returns true when the node resolves to a valid place expression.
pub(crate) fn ast_node_is_place(node: &AstNode) -> bool {
    match &node.kind {
        NodeKind::Rvalue(expr) => matches!(expr.kind, ExpressionKind::Reference(_)),
        NodeKind::FieldAccess { base, .. } => ast_node_is_place(base),
        NodeKind::MethodCall {
            receiver,
            builtin: Some(BuiltinMethodKind::CollectionGet),
            ..
        } => ast_node_is_place(receiver),
        _ => false,
    }
}

/// Returns true when the node resolves to a mutable place expression.
pub(crate) fn ast_node_is_mutable_place(node: &AstNode) -> bool {
    match &node.kind {
        NodeKind::Rvalue(expr) => {
            matches!(expr.kind, ExpressionKind::Reference(_)) && expr.ownership.is_mutable()
        }
        NodeKind::FieldAccess { base, .. } => ast_node_is_mutable_place(base),
        NodeKind::MethodCall {
            receiver,
            builtin: Some(BuiltinMethodKind::CollectionGet),
            ..
        } => ast_node_is_mutable_place(receiver),
        _ => false,
    }
}

/// Builds a user-facing receiver hint for diagnostics like `~value.method(...)`.
pub(crate) fn receiver_access_hint(node: &AstNode, string_table: &StringTable) -> String {
    match &node.kind {
        NodeKind::Rvalue(expr) => match &expr.kind {
            ExpressionKind::Reference(path) => path
                .name_str(string_table)
                .map(str::to_owned)
                .unwrap_or_else(|| path.to_string(string_table)),
            _ => String::from("receiver"),
        },
        NodeKind::FieldAccess { base, field, .. } => {
            format!(
                "{}.{}",
                receiver_access_hint(base, string_table),
                string_table.resolve(*field)
            )
        }
        _ => String::from("receiver"),
    }
}
