//! Branching and match parsing regression tests.
//!
//! WHAT: validates `if`/`else` and `match`-style AST construction.
//! WHY: control-flow lowering relies on branch bodies and match arms staying structurally correct.

use super::*;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::test_support::{parse_single_file_ast, start_function_body};
use crate::compiler_frontend::datatypes::DataType;

#[test]
fn parses_if_else_statements() {
    let (ast, string_table) =
        parse_single_file_ast("flag = true\nif flag:\n    io(\"yes\")\nelse\n    io(\"no\")\n;\n");

    let body = start_function_body(&ast, &string_table);

    let NodeKind::If(condition, then_block, else_block) = &body[1].kind else {
        panic!("expected if statement in start body");
    };

    assert_eq!(condition.data_type, DataType::Bool);
    assert_eq!(then_block.len(), 1);
    assert_eq!(
        else_block.as_ref().map(Vec::len),
        Some(1),
        "else block should contain one host call"
    );
}

#[test]
fn parses_match_statements_with_else_arm() {
    let (ast, string_table) = parse_single_file_ast(
        "value = 42\nif value is:\n    0: io(\"zero\");\n    42: io(\"forty-two\");\n    else: io(\"other\");\n;\n",
    );

    let body = start_function_body(&ast, &string_table);

    let NodeKind::Match(subject, arms, else_block) = &body[1].kind else {
        panic!("expected match statement in start body");
    };

    assert_eq!(subject.data_type, DataType::Int);
    assert_eq!(arms.len(), 2);
    assert!(matches!(arms[0].condition.kind, ExpressionKind::Int(0)));
    assert!(matches!(arms[1].condition.kind, ExpressionKind::Int(42)));
    assert_eq!(
        else_block.as_ref().map(Vec::len),
        Some(1),
        "match should keep the default arm body"
    );
}
