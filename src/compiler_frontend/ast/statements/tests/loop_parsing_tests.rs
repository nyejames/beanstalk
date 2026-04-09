//! Loop parsing regression tests.
//!
//! WHAT: validates conditional and range-loop AST shapes.
//! WHY: loop lowering depends on the parser preserving range bounds, inclusivity, and steps.

use super::*;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::test_support::{parse_single_file_ast, parse_single_file_ast_error};
use crate::compiler_frontend::ast::test_support::start_function_body;

#[test]
fn parses_boolean_conditional_loops() {
    let (ast, string_table) =
        parse_single_file_ast("counter ~= 0\nloop counter < 3:\n    counter = counter + 1\n;\n");

    let body = start_function_body(&ast, &string_table);

    let NodeKind::WhileLoop(condition, loop_body) = &body[1].kind else {
        panic!("expected conditional loop in start body");
    };

    assert!(matches!(condition.data_type, DataType::Bool));
    assert_eq!(loop_body.len(), 1);
    assert!(matches!(loop_body[0].kind, NodeKind::Assignment { .. }));
}

#[test]
fn parses_range_loops_with_inclusive_end_and_step() {
    let (ast, string_table) =
        parse_single_file_ast("sum ~= 0\nloop i in 1 upto 5 by 2:\n    sum = sum + i\n;\n");

    let body = start_function_body(&ast, &string_table);

    let NodeKind::ForLoop(binder, range, loop_body) = &body[1].kind else {
        panic!("expected range for-loop in start body");
    };

    assert_eq!(binder.id.name_str(&string_table), Some("i"));
    assert_eq!(range.end_kind, RangeEndKind::Inclusive);
    assert!(matches!(range.start.kind, ExpressionKind::Int(1)));
    assert!(matches!(range.end.kind, ExpressionKind::Int(5)));
    assert!(matches!(
        range.step.as_ref().map(|expr| &expr.kind),
        Some(ExpressionKind::Int(2))
    ));
    assert_eq!(loop_body.len(), 1);
}

#[test]
fn rejects_keyword_shadow_loop_binder_names() {
    let error = parse_single_file_ast_error("sum ~= 0\nloop _if in 0 to 3:\n    sum = sum + 1\n;\n");
    assert!(
        error
            .msg
            .contains("Identifier '_if' is reserved because it visually shadows language keyword 'if'"),
        "{}",
        error.msg
    );
}
