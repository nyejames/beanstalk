//! Collection literal parsing regression tests.
//!
//! WHAT: validates collection item parsing and unterminated-literal diagnostics.
//! WHY: collections use expression recursion and can silently drift without focused coverage.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::{ExpressionKind, ResultCallHandling};
use crate::compiler_frontend::ast::test_support::{
    function_body_by_name, parse_single_file_ast, parse_single_file_ast_error, start_function_body,
};
use crate::compiler_frontend::builtins::BuiltinMethodKind;

fn runtime_method_builtin_kind(
    expression: &crate::compiler_frontend::ast::expressions::expression::Expression,
) -> BuiltinMethodKind {
    let ExpressionKind::Runtime(nodes) = &expression.kind else {
        panic!("expected runtime expression");
    };
    assert_eq!(nodes.len(), 1, "expected single-node runtime expression");
    let NodeKind::MethodCall {
        builtin: Some(kind),
        ..
    } = &nodes[0].kind
    else {
        panic!("expected collection builtin method call node");
    };

    *kind
}

fn declaration_runtime_method_builtin_kind(
    node: &NodeKind,
) -> crate::compiler_frontend::builtins::BuiltinMethodKind {
    let NodeKind::VariableDeclaration(declaration) = node else {
        panic!("expected variable declaration node");
    };
    runtime_method_builtin_kind(&declaration.value)
}

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

#[test]
fn parses_collection_get_with_fallback_handler_and_propagation() {
    let (ast, string_table) = parse_single_file_ast(
        "read_or_default |values {Int}, idx Int| -> Int:\n    return values.get(idx) ! 0\n;\n\nread_with_handler |values {Int}, idx Int| -> Int:\n    return values.get(idx) err! 0:\n        io(err.message)\n    ;\n;\n\nforward_read |values {Int}, idx Int| -> Int, Error!:\n    return values.get(idx)!\n;\n",
    );

    let fallback_body = function_body_by_name(&ast, &string_table, "read_or_default");
    let NodeKind::Return(values) = &fallback_body[0].kind else {
        panic!("expected return statement in fallback function");
    };
    let ExpressionKind::HandledResult { handling, .. } = &values[0].kind else {
        panic!("expected handled result expression for fallback");
    };
    assert!(matches!(handling, ResultCallHandling::Fallback(_)));

    let handler_body = function_body_by_name(&ast, &string_table, "read_with_handler");
    let NodeKind::Return(values) = &handler_body[0].kind else {
        panic!("expected return statement in handler function");
    };
    let ExpressionKind::HandledResult { handling, .. } = &values[0].kind else {
        panic!("expected handled result expression for named handler");
    };
    assert!(matches!(handling, ResultCallHandling::Handler { .. }));

    let propagation_body = function_body_by_name(&ast, &string_table, "forward_read");
    let NodeKind::Return(values) = &propagation_body[0].kind else {
        panic!("expected return statement in propagation function");
    };
    let ExpressionKind::HandledResult { handling, .. } = &values[0].kind else {
        panic!("expected handled result expression for propagation");
    };
    assert!(matches!(handling, ResultCallHandling::Propagate));
}

#[test]
fn parses_collection_mutators_and_length_calls() {
    let (ast, string_table) = parse_single_file_ast(
        "values ~= {1, 2, 3}\nvalues.set(1, 9)\nvalues.get(0) = 7\nvalues.push(4)\nvalues.remove(2)\nsize = values.length()\n",
    );
    let body = start_function_body(&ast, &string_table);

    let NodeKind::Rvalue(set_expr) = &body[1].kind else {
        panic!("expected set(...) statement");
    };
    assert_eq!(
        runtime_method_builtin_kind(set_expr),
        BuiltinMethodKind::CollectionSet
    );

    let NodeKind::Assignment { target, .. } = &body[2].kind else {
        panic!("expected indexed assignment statement");
    };
    let NodeKind::MethodCall {
        builtin: Some(BuiltinMethodKind::CollectionGet),
        ..
    } = &target.kind
    else {
        panic!("expected assignment target to be collection get(...)");
    };

    let NodeKind::Rvalue(push_expr) = &body[3].kind else {
        panic!("expected push(...) statement");
    };
    assert_eq!(
        runtime_method_builtin_kind(push_expr),
        BuiltinMethodKind::CollectionPush
    );

    let NodeKind::Rvalue(remove_expr) = &body[4].kind else {
        panic!("expected remove(...) statement");
    };
    assert_eq!(
        runtime_method_builtin_kind(remove_expr),
        BuiltinMethodKind::CollectionRemove
    );

    let NodeKind::VariableDeclaration(size_decl) = &body[5].kind else {
        panic!("expected length declaration");
    };
    assert_eq!(
        size_decl.value.data_type,
        crate::compiler_frontend::datatypes::DataType::Int
    );
    assert_eq!(
        runtime_method_builtin_kind(&size_decl.value),
        BuiltinMethodKind::CollectionLength
    );
}

#[test]
fn parses_collection_mutators_with_explicit_receiver_tilde_prefix() {
    let (ast, string_table) = parse_single_file_ast(
        "values ~= {1, 2, 3}\n~values.push(4)\n~values.set(1, 9)\n~values.remove(2)\nsize = values.length()\n",
    );
    let body = start_function_body(&ast, &string_table);

    let NodeKind::Rvalue(push_expr) = &body[1].kind else {
        panic!("expected push(...) statement");
    };
    assert_eq!(
        runtime_method_builtin_kind(push_expr),
        BuiltinMethodKind::CollectionPush
    );

    let NodeKind::Rvalue(set_expr) = &body[2].kind else {
        panic!("expected set(...) statement");
    };
    assert_eq!(
        runtime_method_builtin_kind(set_expr),
        BuiltinMethodKind::CollectionSet
    );

    let NodeKind::Rvalue(remove_expr) = &body[3].kind else {
        panic!("expected remove(...) statement");
    };
    assert_eq!(
        runtime_method_builtin_kind(remove_expr),
        BuiltinMethodKind::CollectionRemove
    );
}

#[test]
fn rejects_mutating_collection_method_without_explicit_receiver_tilde() {
    let error = parse_single_file_ast_error("values ~= {1, 2, 3}\nvalues.push(4)\n");
    assert!(error.msg.contains("push"), "{}", error.msg);
    assert!(error.msg.contains("~"), "{}", error.msg);
}

#[test]
fn rejects_pull_method_and_guides_to_remove() {
    let error = parse_single_file_ast_error("values ~= {1, 2, 3}\nvalues.pull(1)\n");
    assert!(error.msg.contains("pull(...)"), "{}", error.msg);
    assert!(error.msg.contains("remove(index)"), "{}", error.msg);
}

#[test]
fn rejects_set_on_immutable_collection() {
    let error = parse_single_file_ast_error("values = {1, 2, 3}\nvalues.set(0, 9)\n");
    assert!(
        error.msg.contains("requires a mutable collection receiver"),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_get_index_assignment_on_immutable_collection() {
    let error = parse_single_file_ast_error("values = {1, 2, 3}\nvalues.get(0) = 9\n");
    assert!(
        error.msg.contains("Cannot mutate immutable variable"),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_unhandled_collection_get_result() {
    let error = parse_single_file_ast_error("values ~= {1, 2, 3}\nvalue = values.get(0)\n");
    assert!(
        error
            .msg
            .contains("must be explicitly handled with '!' syntax"),
        "{}",
        error.msg
    );
}

#[test]
fn parses_builtin_error_helper_methods() {
    let (ast, string_table) = parse_single_file_ast(
        "apply_helpers |err Error, location ErrorLocation, frame StackFrame| -> Error:\n    first = err.with_location(location)\n    second = first.push_trace(frame)\n    return second.bubble()\n;\n",
    );

    let body = function_body_by_name(&ast, &string_table, "apply_helpers");
    assert_eq!(
        declaration_runtime_method_builtin_kind(&body[0].kind),
        BuiltinMethodKind::ErrorWithLocation
    );
    assert_eq!(
        declaration_runtime_method_builtin_kind(&body[1].kind),
        BuiltinMethodKind::ErrorPushTrace
    );

    let NodeKind::Return(values) = &body[2].kind else {
        panic!("expected return statement");
    };
    assert_eq!(
        runtime_method_builtin_kind(&values[0]),
        BuiltinMethodKind::ErrorBubble
    );
}
