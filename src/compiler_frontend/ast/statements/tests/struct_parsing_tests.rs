//! Struct parsing regression tests.
//!
//! WHAT: validates struct definitions, defaults, constructors, and field access.
//! WHY: struct parsing feeds both type resolution and HIR place lowering.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::tests::test_support::{parse_single_file_ast, start_function_body};

#[test]
fn parses_struct_definitions_with_field_defaults() {
    let (ast, string_table) = parse_single_file_ast("Point = |\n    x Int,\n    y Int = 2,\n|\n");

    let struct_node = ast
        .nodes
        .iter()
        .find(|node| {
            matches!(
                &node.kind,
                NodeKind::StructDefinition(path, ..)
                    if path.name_str(&string_table) == Some("Point")
            )
        })
        .expect("expected struct definition");

    let NodeKind::StructDefinition(path, fields) = &struct_node.kind else {
        panic!("expected struct definition node");
    };

    assert_eq!(path.name_str(&string_table), Some("Point"));
    assert_eq!(fields.len(), 2);
    assert!(matches!(fields[0].value.kind, ExpressionKind::NoValue));
    assert!(matches!(fields[1].value.kind, ExpressionKind::Int(2)));
}

#[test]
fn parses_struct_construction_and_field_access_in_declarations() {
    let (ast, string_table) = parse_single_file_ast(
        "Point = |\n    x Int,\n    y Int,\n|\n\npoint = Point(1, 2)\nvalue = point.x\n",
    );

    let body = start_function_body(&ast, &string_table);

    let NodeKind::VariableDeclaration(point_decl) = &body[0].kind else {
        panic!("expected point declaration");
    };
    assert!(matches!(
        point_decl.value.kind,
        ExpressionKind::StructInstance(..)
    ));

    let NodeKind::VariableDeclaration(value_decl) = &body[1].kind else {
        panic!("expected field-read declaration");
    };
    let ExpressionKind::Runtime(nodes) = &value_decl.value.kind else {
        panic!("field access should stay as a runtime expression");
    };
    assert!(
        nodes
            .iter()
            .any(|node| matches!(node.kind, NodeKind::FieldAccess { .. })),
        "runtime field access should preserve a field-access AST node"
    );
}
