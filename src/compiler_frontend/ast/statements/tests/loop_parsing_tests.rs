//! Loop parsing regression tests.
//!
//! WHAT: validates conditional/range/collection loop AST shapes and loop-header diagnostics.
//! WHY: loop lowering depends on parser output staying stable across the new loop header syntax.

use super::*;
use crate::compiler_frontend::ast::Ast;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey, ErrorType};
use crate::compiler_frontend::tests::test_support::{
    function_body_by_name, parse_single_file_ast, parse_single_file_ast_error,
};

fn loop_fixture_source(loop_body_source: &str) -> String {
    let indented_body = loop_body_source
        .lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!("loop_test ||:\n{indented_body}\n;\n\nloop_test()\n")
}

fn parse_loop_fixture(loop_body_source: &str) -> (Ast, StringTable) {
    parse_single_file_ast(&loop_fixture_source(loop_body_source))
}

fn parse_loop_fixture_error(loop_body_source: &str) -> CompilerError {
    parse_single_file_ast_error(&loop_fixture_source(loop_body_source))
}

fn loop_function_body<'a>(ast: &'a Ast, string_table: &StringTable) -> &'a [AstNode] {
    function_body_by_name(ast, string_table, "loop_test")
}

#[test]
fn parses_conditional_loop_without_bindings() {
    let (ast, string_table) =
        parse_loop_fixture("counter ~= 0\nloop counter < 3:\n    counter = counter + 1\n;");

    let body = loop_function_body(&ast, &string_table);

    let NodeKind::WhileLoop(condition, loop_body) = &body[1].kind else {
        panic!("expected conditional loop in function body");
    };

    assert!(matches!(condition.data_type, DataType::Bool));
    assert_eq!(loop_body.len(), 1);
    assert!(matches!(loop_body[0].kind, NodeKind::Assignment { .. }));
}

#[test]
fn parses_range_loop_with_pipe_binding() {
    let (ast, string_table) =
        parse_loop_fixture("sum ~= 0\nloop 1 upto 5 by 2 |i|:\n    sum = sum + i\n;");

    let body = loop_function_body(&ast, &string_table);

    let NodeKind::RangeLoop {
        bindings,
        range,
        body: loop_body,
    } = &body[1].kind
    else {
        panic!("expected range loop in function body");
    };

    assert_eq!(
        bindings
            .item
            .as_ref()
            .and_then(|binding| binding.id.name_str(&string_table)),
        Some("i")
    );
    assert!(bindings.index.is_none());
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
fn parses_range_loop_with_value_and_index_bindings() {
    let (ast, string_table) =
        parse_loop_fixture("sum ~= 0\nloop 0 to 4 |value, index|:\n    sum = sum + value\n;");

    let body = loop_function_body(&ast, &string_table);

    let NodeKind::RangeLoop { bindings, .. } = &body[1].kind else {
        panic!("expected range loop in function body");
    };

    assert_eq!(
        bindings
            .item
            .as_ref()
            .and_then(|binding| binding.id.name_str(&string_table)),
        Some("value")
    );
    assert_eq!(
        bindings
            .index
            .as_ref()
            .and_then(|binding| binding.id.name_str(&string_table)),
        Some("index")
    );
}

#[test]
fn parses_collection_loop_with_pipe_item_binding() {
    let (ast, string_table) =
        parse_loop_fixture("items = {1, 2, 3}\nloop items |item|:\n    io(item)\n;");

    let body = loop_function_body(&ast, &string_table);

    let NodeKind::CollectionLoop {
        bindings,
        iterable,
        body: loop_body,
    } = &body[1].kind
    else {
        panic!("expected collection loop in function body");
    };

    assert_eq!(
        bindings
            .item
            .as_ref()
            .and_then(|binding| binding.id.name_str(&string_table)),
        Some("item")
    );
    assert!(bindings.index.is_none());
    assert!(matches!(iterable.kind, ExpressionKind::Reference(_)));
    assert!(matches!(
        iterable.data_type,
        DataType::Collection(_, _) | DataType::Reference(_)
    ));
    assert_eq!(loop_body.len(), 1);
}

#[test]
fn parses_collection_loop_with_item_and_index_pipe_bindings() {
    let (ast, string_table) =
        parse_loop_fixture("items = {1, 2, 3}\nloop items |item, index|:\n    io(item)\n;");

    let body = loop_function_body(&ast, &string_table);

    let NodeKind::CollectionLoop { bindings, .. } = &body[1].kind else {
        panic!("expected collection loop in function body");
    };

    assert_eq!(
        bindings
            .item
            .as_ref()
            .and_then(|binding| binding.id.name_str(&string_table)),
        Some("item")
    );
    assert_eq!(
        bindings
            .index
            .as_ref()
            .and_then(|binding| binding.id.name_str(&string_table)),
        Some("index")
    );
}

#[test]
fn range_index_binding_has_int_type() {
    let (ast, string_table) =
        parse_loop_fixture("sum ~= 0\nloop 0 to 4 |value, index|:\n    sum = sum + value\n;");
    let body = loop_function_body(&ast, &string_table);

    let NodeKind::RangeLoop { bindings, .. } = &body[1].kind else {
        panic!("expected range loop in function body");
    };

    assert!(matches!(
        bindings
            .index
            .as_ref()
            .map(|binding| &binding.value.data_type),
        Some(DataType::Int)
    ));
}

#[test]
fn collection_index_binding_has_int_type() {
    let (ast, string_table) =
        parse_loop_fixture("items = {1, 2, 3}\nloop items |item, index|:\n    io(item)\n;");
    let body = loop_function_body(&ast, &string_table);

    let NodeKind::CollectionLoop { bindings, .. } = &body[1].kind else {
        panic!("expected collection loop in function body");
    };

    assert!(matches!(
        bindings
            .index
            .as_ref()
            .map(|binding| &binding.value.data_type),
        Some(DataType::Int)
    ));
}

#[test]
fn rejects_old_in_loop_syntax_with_migration_error() {
    let error = parse_loop_fixture_error("loop i in 0 to 3:\n    io(i)\n;");
    assert!(
        error
            .msg
            .contains("Old loop syntax 'loop <binder> in ...' was removed"),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_collection_loop_with_bare_single_binding() {
    let error = parse_loop_fixture_error("items = {1, 2, 3}\nloop items item:\n    io(item)\n;");
    assert!(
        error.msg.contains("Loop bindings must use `|...|`"),
        "{}",
        error.msg
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::PrimarySuggestion)
            .map(String::as_str),
        Some("Use syntax like `loop items |item|:` or `loop 0 to 10 |i|:`")
    );
}

#[test]
fn rejects_collection_loop_with_bare_dual_bindings() {
    let error =
        parse_loop_fixture_error("items = {1, 2, 3}\nloop items item, index:\n    io(item)\n;");
    assert!(
        error
            .msg
            .contains("Loop bindings must use `|item, index|` form."),
        "{}",
        error.msg
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::PrimarySuggestion)
            .map(String::as_str),
        Some("Write `loop items |item, index|:` instead of bare trailing names.")
    );
}

#[test]
fn rejects_range_loop_with_bare_single_binding() {
    let error = parse_loop_fixture_error("loop 0 to 10 i:\n    io(i)\n;");
    assert!(
        error.msg.contains("Loop bindings must use `|...|`"),
        "{}",
        error.msg
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::PrimarySuggestion)
            .map(String::as_str),
        Some("Use syntax like `loop items |item|:` or `loop 0 to 10 |i|:`")
    );
}

#[test]
fn rejects_range_loop_with_bare_dual_bindings() {
    let error = parse_loop_fixture_error("loop 0 to 10 i, index:\n    io(i)\n;");
    assert!(
        error
            .msg
            .contains("Loop bindings must use `|item, index|` form."),
        "{}",
        error.msg
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::PrimarySuggestion)
            .map(String::as_str),
        Some("Write `loop items |item, index|:` instead of bare trailing names.")
    );
}

#[test]
fn parses_collection_loop_without_bindings() {
    let (ast, string_table) =
        parse_loop_fixture("count ~= 0\nitems = {1, 2, 3}\nloop items:\n    count = count + 1\n;");
    let body = loop_function_body(&ast, &string_table);

    let NodeKind::CollectionLoop { bindings, .. } = &body[2].kind else {
        panic!("expected collection loop in function body");
    };

    assert!(bindings.item.is_none());
    assert!(bindings.index.is_none());
}

#[test]
fn parses_range_loop_without_bindings() {
    let (ast, string_table) = parse_loop_fixture("loop 0 to 10:\n    io(1)\n;");
    let body = loop_function_body(&ast, &string_table);

    let NodeKind::RangeLoop { bindings, .. } = &body[0].kind else {
        panic!("expected range loop in function body");
    };

    assert!(bindings.item.is_none());
    assert!(bindings.index.is_none());
}

#[test]
fn rejects_empty_loop_binding_list() {
    let error = parse_loop_fixture_error("items = {1, 2, 3}\nloop items ||:\n    io(items)\n;");
    assert!(error.msg.contains("Loop binding list cannot be empty"));
}

#[test]
fn rejects_more_than_two_loop_bindings() {
    let error = parse_loop_fixture_error("items = {1, 2, 3}\nloop items |a, b, c|:\n    io(a)\n;");
    assert!(
        error
            .msg
            .contains("Loop bindings support at most two names")
    );
}

#[test]
fn rejects_duplicate_loop_binding_names() {
    let error =
        parse_loop_fixture_error("items = {1, 2, 3}\nloop items |item, item|:\n    io(item)\n;");
    assert!(error.msg.contains("Duplicate loop binding name"));
}

#[test]
fn rejects_loop_binding_shadowing_existing_name() {
    let error = parse_loop_fixture_error(
        "items = {1, 2, 3}\nitem = 0\nloop items |item|:\n    io(item)\n;",
    );
    assert!(error.msg.contains("already declared in this scope"));
}

#[test]
fn rejects_keyword_shadow_loop_binding_names() {
    let error = parse_loop_fixture_error("items = {1, 2, 3}\nloop items |_if|:\n    io(items)\n;");
    assert!(
        error.msg.contains(
            "Identifier '_if' is reserved because it visually shadows language keyword 'if'"
        ),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_collection_loop_on_non_collection_expression() {
    let error = parse_loop_fixture_error("value = 3\nloop value |item|:\n    io(item)\n;");
    assert!(
        error
            .msg
            .contains("Collection loop source must be a collection"),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_non_boolean_conditional_loop_condition() {
    let error = parse_loop_fixture_error("loop 1 + 2:\n    io(1)\n;");
    assert_eq!(error.error_type, ErrorType::Type);
    assert!(
        error
            .msg
            .contains("Loop condition requires a Bool condition"),
        "{}",
        error.msg
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::ExpectedType)
            .map(String::as_str),
        Some("Bool")
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::FoundType)
            .map(String::as_str),
        Some("Int")
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::PrimarySuggestion)
            .map(String::as_str),
        Some("Use a boolean expression after 'loop', e.g. loop is_ready():")
    );
}

#[test]
fn rejects_range_loop_missing_end_bound() {
    let error = parse_loop_fixture_error("loop 0 to |i|:\n    io(i)\n;");
    assert!(error.msg.contains("Range loop is missing an end bound"));
}

#[test]
fn rejects_range_loop_by_without_step() {
    let error = parse_loop_fixture_error("loop 0 to 10 by |i|:\n    io(i)\n;");
    assert!(error.msg.contains("uses 'by' without a step value"));
}

#[test]
fn rejects_zero_step_literal() {
    let error = parse_loop_fixture_error("loop 0 to 10 by 0 |i|:\n    io(i)\n;");
    assert!(error.msg.contains("Range step cannot be zero"));
}

#[test]
fn rejects_float_range_without_by() {
    let error = parse_loop_fixture_error("loop 0.0 to 1.0 |t|:\n    io(t)\n;");
    assert!(
        error
            .msg
            .contains("Float ranges require an explicit 'by' step")
    );
}

#[test]
fn rejects_missing_comma_between_bare_dual_bindings() {
    let error =
        parse_loop_fixture_error("items = {1, 2, 3}\nloop items item index:\n    io(item)\n;");
    assert!(
        error
            .msg
            .contains("Loop bindings must use `|item, index|` form.")
    );
}

#[test]
fn rejects_missing_closing_pipe_in_loop_bindings() {
    let error = parse_loop_fixture_error("items = {1, 2, 3}\nloop items |item:\n    io(item)\n;");
    assert!(error.msg.contains("Missing closing pipe in loop bindings"));
}

#[test]
fn rejects_range_loop_with_bare_binding_after_complex_step_expression() {
    let error = parse_loop_fixture_error(
        "limit = 8\nstep = 2\nsum ~= 0\nloop 0 to limit by step i:\n    sum = sum + i\n;",
    );
    assert!(
        error.msg.contains("Loop bindings must use `|...|`"),
        "{}",
        error.msg
    );
}

#[test]
fn operator_tail_does_not_trigger_bare_loop_binding_diagnostic() {
    let error = parse_loop_fixture_error("a = 1\nb = 2\nloop a + b value:\n    io(1)\n;");
    assert!(
        !error.msg.contains("Loop bindings must use"),
        "{}",
        error.msg
    );
}
