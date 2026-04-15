//! Result-handling parsing and validation regression tests.
//!
//! WHAT: exercises shared result-handling helpers across call/expression paths.
//! WHY: result handling spans dense syntax plus control-flow constraints, so focused tests prevent
//! parser and validation drift during refactors.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::{ExpressionKind, ResultCallHandling};
use crate::compiler_frontend::tests::test_support::{
    function_body_by_name, parse_single_file_ast, parse_single_file_ast_error,
};

#[test]
fn parses_named_handler_with_fallback() {
    let (ast, string_table) = parse_single_file_ast(
        "can_error |value String| -> String, Error!:\n    return! Error(\"Other\", \"test.synthetic\", \"boom\")\n;\n\nrecover |value String| -> String:\n    output = can_error(value) err! \"fallback\":\n        io(err.message)\n    ;\n    return output\n;\n",
    );

    let body = function_body_by_name(&ast, &string_table, "recover");
    let NodeKind::VariableDeclaration(output_decl) = &body[0].kind else {
        panic!("expected declaration statement in recover()")
    };

    let ExpressionKind::ResultHandledFunctionCall { handling, .. } = &output_decl.value.kind else {
        panic!("expected handled call expression in recover declaration")
    };

    let ResultCallHandling::Handler {
        fallback,
        body: handler_body,
        ..
    } = handling
    else {
        panic!("expected named handler handling")
    };

    assert!(
        fallback.is_some(),
        "expected fallback values on named handler"
    );
    assert_eq!(handler_body.len(), 1);
}

#[test]
fn parses_named_handler_without_fallback_when_handler_guarantees_return() {
    let (ast, string_table) = parse_single_file_ast(
        "can_error |value String| -> String, Error!:\n    return! Error(\"Other\", \"test.synthetic\", \"boom\")\n;\n\nrecover |value String| -> String:\n    output = can_error(value) err!:\n        return \"recovered\"\n    ;\n    return output\n;\n",
    );

    let body = function_body_by_name(&ast, &string_table, "recover");
    let NodeKind::VariableDeclaration(output_decl) = &body[0].kind else {
        panic!("expected declaration statement in recover()")
    };

    let ExpressionKind::ResultHandledFunctionCall { handling, .. } = &output_decl.value.kind else {
        panic!("expected handled call expression in recover declaration")
    };

    let ResultCallHandling::Handler {
        fallback,
        body: handler_body,
        ..
    } = handling
    else {
        panic!("expected named handler handling")
    };

    assert!(fallback.is_none(), "expected no fallback values");
    assert!(matches!(handler_body[0].kind, NodeKind::Return(_)));
}

#[test]
fn rejects_named_handler_without_fallback_when_handler_can_fall_through() {
    let error = parse_single_file_ast_error(
        "can_error |value String| -> String, Error!:\n    return! Error(\"Other\", \"test.synthetic\", \"boom\")\n;\n\nrecover |value String, route Bool| -> String:\n    return can_error(value) err!:\n        if route:\n            io(err.message)\n        else\n            io(err.code)\n        ;\n    ;\n;\n",
    );

    assert!(
        error
            .msg
            .contains("Named handler without fallback can fall through"),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_named_handler_name_conflict_with_visible_declaration() {
    let error = parse_single_file_ast_error(
        "can_error |value String| -> String, Error!:\n    return! Error(\"Other\", \"test.synthetic\", \"boom\")\n;\n\nrecover |value String| -> String:\n    err = \"taken\"\n    output = can_error(value) err! \"fallback\":\n        io(err.message)\n    ;\n    return output\n;\n",
    );

    assert!(
        error
            .msg
            .contains("conflicts with an existing visible declaration"),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_fallback_arity_mismatch_for_multi_value_success_returns() {
    let error = parse_single_file_ast_error(
        "pair_error |value String| -> String, Int, Error!:\n    return! Error(\"Other\", \"test.synthetic\", \"boom\")\n;\n\nrecover |value String| -> String, Int:\n    first, count = pair_error(value) ! \"fallback\", 0, 1\n    return first, count\n;\n",
    );

    assert!(
        error
            .msg
            .contains("provide more entries than the success return arity"),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_invalid_bare_err_for_call_named_handler_shape() {
    let error = parse_single_file_ast_error(
        "can_error |value String| -> String, Error!:\n    return value\n;\n\nrecover |value String| -> String:\n    return can_error(value) err!\n;\n",
    );

    assert!(
        error.msg.contains("Bare 'err!' is invalid"),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_invalid_bare_err_for_expression_named_handler_shape() {
    let error = parse_single_file_ast_error(
        "read |values {Int}| -> Int:\n    return values.get(0) err!\n;\n",
    );

    assert!(
        error.msg.contains("Bare 'err!' is invalid"),
        "{}",
        error.msg
    );
}
