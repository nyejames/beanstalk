//! Collection literal parsing regression tests.
//!
//! WHAT: validates collection item parsing and unterminated-literal diagnostics.
//! WHY: collections use expression recursion and can silently drift without focused coverage.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::test_support::{
    parse_single_file_ast, parse_single_file_ast_error, start_function_body,
};

#[test]
fn parses_collection_literal_items() {
    let (ast, string_table) = parse_single_file_ast("values ~= {1, 2, 3}\n");
    let body = start_function_body(&ast, &string_table);

    let NodeKind::VariableDeclaration(values_decl) = &body[0].kind else {
        panic!("expected collection declaration");
    };

    let ExpressionKind::Collection(items) = &values_decl.value.kind else {
        panic!("expected collection expression");
    };

    assert_eq!(items.len(), 3);
    assert!(matches!(items[0].kind, ExpressionKind::Int(1)));
    assert!(matches!(items[1].kind, ExpressionKind::Int(2)));
    assert!(matches!(items[2].kind, ExpressionKind::Int(3)));
}

#[test]
fn rejects_missing_collection_item_after_comma() {
    let error = parse_single_file_ast_error("values ~= {1, , 2}\n");
    assert!(
        error
            .msg
            .contains("Expected a collection item after the comma")
    );
}
