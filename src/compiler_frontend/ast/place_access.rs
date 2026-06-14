//! AST place-shape helpers shared by parser and call validation.
//!
//! WHAT: classifies AST nodes as readable/writable places.
//! WHY: receiver-method parsing, builtin member parsing, and assignment/call validation all
//! enforce the same place rules, so one helper module keeps diagnostics and semantics aligned.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression_rpn::PlaceExpression;
use crate::compiler_frontend::ast::expressions::parse_expression_places::{
    place_expression_from_expression, place_expression_is_mutable,
};

fn place_expression_from_node(node: &AstNode) -> Option<PlaceExpression> {
    let NodeKind::ExpressionStatement(expression) = &node.kind else {
        return None;
    };

    place_expression_from_expression(expression)
}

/// Returns true when the node resolves to a valid place expression.
pub(crate) fn ast_node_is_place(node: &AstNode) -> bool {
    place_expression_from_node(node).is_some()
}

/// Returns true when the node resolves to a mutable place expression.
pub(crate) fn ast_node_is_mutable_place(node: &AstNode) -> bool {
    place_expression_from_node(node)
        .as_ref()
        .is_some_and(place_expression_is_mutable)
}
