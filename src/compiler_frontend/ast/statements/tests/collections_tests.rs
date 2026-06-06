//! Collection literal parsing regression tests.
//!
//! WHAT: validates collection item parsing and unterminated-literal diagnostics.
//! WHY: collections use expression recursion and can silently drift without focused coverage.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, FallibleHandling,
};
use crate::compiler_frontend::ast::statements::value_production::types::ValueBlock;
use crate::compiler_frontend::builtins::CollectionBuiltinOp;
use crate::compiler_frontend::compiler_messages::{
    DiagnosticPayload, InvalidAssignmentTargetReason, InvalidBuiltinCallReason,
    InvalidCollectionTypeReason, InvalidFieldAccessReason, InvalidReceiverCallReason,
    InvalidResultHandlingReason, TypeMismatchContext,
};
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::tests::ast_fixture_support::{
    function_body_by_name, start_function_body,
};
use crate::compiler_frontend::tests::parse_support::{
    parse_single_file_ast, parse_single_file_ast_diagnostic,
};

fn runtime_collection_builtin_op(expression: &Expression) -> CollectionBuiltinOp {
    let ExpressionKind::Runtime(nodes) = &expression.kind else {
        panic!("expected runtime expression");
    };
    assert_eq!(nodes.len(), 1, "expected single-node runtime expression");
    let NodeKind::CollectionBuiltinCall { op, .. } = &nodes[0].kind else {
        panic!("expected collection builtin call node");
    };

    *op
}

fn handled_collection_builtin_op(expression: &Expression) -> CollectionBuiltinOp {
    let handled_expression = match &expression.kind {
        ExpressionKind::HandledFallibleExpression { value, .. } => value.as_ref(),
        ExpressionKind::ValueBlock { block } => {
            let ValueBlock::Catch(value_catch) = block.as_ref() else {
                panic!("expected catch value block");
            };
            let ExpressionKind::HandledFallibleExpression { value, .. } =
                &value_catch.handled_value.kind
            else {
                panic!("expected handled fallible expression inside catch value block");
            };
            value.as_ref()
        }
        _ => panic!("expected handled fallible expression"),
    };

    runtime_collection_builtin_op(handled_expression)
}

// --------------------------
//  Collection literals
// --------------------------

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
fn infers_collection_element_type_from_non_empty_literal() {
    let (ast, string_table) = parse_single_file_ast("values ~= {1, 2, 3}\n");
    let body = start_function_body(&ast, &string_table);

    let NodeKind::VariableDeclaration(values_decl) = &body[0].kind else {
        panic!("expected values declaration");
    };

    assert_eq!(
        values_decl
            .value
            .diagnostic_type
            .display_with_table(&string_table),
        "{Int}"
    );
}

#[test]
fn parses_empty_collection_with_explicit_element_type() {
    let (ast, string_table) =
        parse_single_file_ast("Reading = |\n    value Float,\n|\n\nreadings ~{Reading} = {}\n");
    let body = start_function_body(&ast, &string_table);

    let NodeKind::VariableDeclaration(readings_decl) = &body[0].kind else {
        panic!("expected readings declaration");
    };

    assert_eq!(
        readings_decl
            .value
            .diagnostic_type
            .display_with_table(&string_table),
        "{Reading}"
    );
}

#[test]
fn rejects_empty_collection_without_explicit_element_type() {
    let diagnostic = parse_single_file_ast_diagnostic("values ~= {}\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::EmptyCollectionTypeAmbiguity
    ));
}

#[test]
fn rejects_mixed_item_types_in_inferred_collection() {
    let diagnostic = parse_single_file_ast_diagnostic("values ~= {1, \"bad\"}\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::TypeMismatch {
            context: TypeMismatchContext::CollectionElement,
            ..
        }
    ));
}

#[test]
fn rejects_item_that_does_not_match_explicit_collection_type() {
    let diagnostic = parse_single_file_ast_diagnostic("values ~{Int} = {1, \"bad\"}\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::TypeMismatch {
            context: TypeMismatchContext::CollectionElement,
            ..
        }
    ));
}

#[test]
fn parses_push_after_explicit_empty_collection() {
    let (ast, string_table) = parse_single_file_ast(
        "values ~{Int} = {}\n~values.push(1) catch:
;\n",
    );
    let body = start_function_body(&ast, &string_table);

    let NodeKind::Rvalue(push_expr) = &body[1].kind else {
        panic!("expected push statement");
    };

    assert_eq!(
        handled_collection_builtin_op(push_expr),
        CollectionBuiltinOp::Push
    );
}

#[test]
fn rejects_empty_collection_even_when_later_push_would_reveal_type() {
    let diagnostic = parse_single_file_ast_diagnostic("values ~= {}\n~values.push(1)\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::EmptyCollectionTypeAmbiguity
    ));
}

#[test]
fn rejects_missing_collection_item_after_comma() {
    let diagnostic = parse_single_file_ast_diagnostic("values ~= {1, , 2}\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::MissingCollectionItem
    ));
}

// --------------------------
//  Fallible handling on collections
// --------------------------

#[test]
fn parses_collection_get_with_fallback_handler_and_propagation() {
    let (ast, string_table) = parse_single_file_ast(
        "read_or_default |values {Int}, idx Int| -> Int:\n    return values.get(idx) catch:\n        then 0\n    ;\n;\n\nread_with_handler |values {Int}, idx Int| -> Int:\n    return values.get(idx) catch |err|:\n        io(err.message)\n        then 0\n    ;\n;\n\nforward_read |values {Int}, idx Int| -> Int, Error!:\n    return values.get(idx)!\n;\n",
    );

    let fallback_body = function_body_by_name(&ast, &string_table, "read_or_default");
    let NodeKind::Return(values) = &fallback_body[0].kind else {
        panic!("expected return statement in fallback function");
    };
    assert!(matches!(values[0].kind, ExpressionKind::ValueBlock { .. }));

    let handler_body = function_body_by_name(&ast, &string_table, "read_with_handler");
    let NodeKind::Return(values) = &handler_body[0].kind else {
        panic!("expected return statement in handler function");
    };
    assert!(matches!(values[0].kind, ExpressionKind::ValueBlock { .. }));

    let propagation_body = function_body_by_name(&ast, &string_table, "forward_read");
    let NodeKind::Return(values) = &propagation_body[0].kind else {
        panic!("expected return statement in propagation function");
    };
    let ExpressionKind::HandledFallibleExpression { handling, .. } = &values[0].kind else {
        panic!("expected handled fallible expression for propagation");
    };
    assert!(matches!(handling, FallibleHandling::Propagate));
}

// --------------------------
//  Collection mutators and accessors
// --------------------------

#[test]
fn parses_collection_mutators_and_length_calls() {
    let (ast, string_table) = parse_single_file_ast(
        "values ~= {1, 2, 3}\n~values.set(1, 9) catch:\n;\n~values.push(4) catch:
;\nremoved = ~values.remove(2) catch:\n    then 0\n;\nsize = values.length()\n",
    );
    let body = start_function_body(&ast, &string_table);

    let NodeKind::Rvalue(set_expr) = &body[1].kind else {
        panic!("expected set(...) statement");
    };
    assert_eq!(
        handled_collection_builtin_op(set_expr),
        CollectionBuiltinOp::Set
    );

    let NodeKind::Rvalue(push_expr) = &body[2].kind else {
        panic!("expected push(...) statement");
    };
    assert_eq!(
        handled_collection_builtin_op(push_expr),
        CollectionBuiltinOp::Push
    );

    let NodeKind::VariableDeclaration(removed_decl) = &body[3].kind else {
        panic!("expected removed declaration");
    };
    assert_eq!(
        handled_collection_builtin_op(&removed_decl.value),
        CollectionBuiltinOp::Remove
    );

    let NodeKind::VariableDeclaration(size_decl) = &body[4].kind else {
        panic!("expected length declaration");
    };
    assert_eq!(size_decl.value.type_id, builtin_type_ids::INT);
    assert_eq!(
        runtime_collection_builtin_op(&size_decl.value),
        CollectionBuiltinOp::Length
    );
}

#[test]
fn parses_collection_mutators_with_explicit_receiver_tilde_prefix() {
    let (ast, string_table) = parse_single_file_ast(
        "values ~= {1, 2, 3}\n~values.push(4) catch:
;\n~values.set(1, 9) catch:\n;\nremoved = ~values.remove(2) catch:\n    then 0\n;\nsize = values.length()\n",
    );
    let body = start_function_body(&ast, &string_table);

    let NodeKind::Rvalue(push_expr) = &body[1].kind else {
        panic!("expected push(...) statement");
    };
    assert_eq!(
        handled_collection_builtin_op(push_expr),
        CollectionBuiltinOp::Push
    );

    let NodeKind::Rvalue(set_expr) = &body[2].kind else {
        panic!("expected set(...) statement");
    };
    assert_eq!(
        handled_collection_builtin_op(set_expr),
        CollectionBuiltinOp::Set
    );

    let NodeKind::VariableDeclaration(removed_decl) = &body[3].kind else {
        panic!("expected removed declaration");
    };
    assert_eq!(
        handled_collection_builtin_op(&removed_decl.value),
        CollectionBuiltinOp::Remove
    );
}

#[test]
fn rejects_unhandled_collection_push_result() {
    let diagnostic = parse_single_file_ast_diagnostic("values ~= {1, 2, 3}\n~values.push(4)\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidBuiltinCall {
            reason: InvalidBuiltinCallReason::MustHandleFallibleResult,
            ..
        }
    ));
}

#[test]
fn accepts_collection_push_postfix_propagation() {
    parse_single_file_ast(
        "append || -> Error!:
    values ~= {1, 2, 3}
    ~values.push(4)!
;
",
    );
}

#[test]
fn rejects_collection_push_fallback_value() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "values ~= {1, 2, 3}
~values.push(4) catch:
    then 0
;
",
    );

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidResultHandling {
            reason: InvalidResultHandlingReason::FallbackValuesForErrorOnlyResult,
            ..
        }
    ));
}

#[test]
fn rejects_mutating_collection_method_without_explicit_receiver_tilde() {
    let diagnostic = parse_single_file_ast_diagnostic("values ~= {1, 2, 3}\nvalues.push(4)\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidReceiverCall {
            reason: InvalidReceiverCallReason::MissingMutableAccessMarker,
            ..
        }
    ));
}

#[test]
fn rejects_explicit_receiver_tilde_for_collection_get_and_length() {
    let get_diagnostic = parse_single_file_ast_diagnostic(
        "values ~= {1, 2, 3}\nvalue = ~values.get(0) catch:\n    then 0\n;\n",
    );
    assert!(matches!(
        get_diagnostic.payload,
        DiagnosticPayload::InvalidReceiverCall {
            reason: InvalidReceiverCallReason::UnneededMutableAccessMarker,
            ..
        }
    ));

    let length_diagnostic =
        parse_single_file_ast_diagnostic("values ~= {1, 2, 3}\nvalue = ~values.length()\n");
    assert!(matches!(
        length_diagnostic.payload,
        DiagnosticPayload::InvalidReceiverCall {
            reason: InvalidReceiverCallReason::UnneededMutableAccessMarker,
            ..
        }
    ));
}

#[test]
fn rejects_pull_method_as_unknown_collection_member() {
    let diagnostic = parse_single_file_ast_diagnostic("values ~= {1, 2, 3}\nvalues.pull(1)\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidFieldAccess {
            reason: InvalidFieldAccessReason::UnknownMember,
            ..
        }
    ));
}

#[test]
fn rejects_set_on_immutable_collection() {
    let diagnostic = parse_single_file_ast_diagnostic("values = {1, 2, 3}\nvalues.set(0, 9)\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidReceiverCall {
            reason: InvalidReceiverCallReason::MutableCollectionRequired,
            ..
        }
    ));
}

#[test]
fn rejects_get_index_assignment_as_removed_syntax() {
    let diagnostic = parse_single_file_ast_diagnostic("values = {1, 2, 3}\nvalues.get(0) = 9\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidAssignmentTarget {
            reason: InvalidAssignmentTargetReason::CollectionIndexedWriteRemoved,
            ..
        }
    ));
}

#[test]
fn rejects_unhandled_collection_get_result() {
    let diagnostic =
        parse_single_file_ast_diagnostic("values ~= {1, 2, 3}\nvalue = values.get(0)\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidBuiltinCall {
            reason: InvalidBuiltinCallReason::MustHandleFallibleResult,
            ..
        }
    ));
}

#[test]
fn rejects_unhandled_collection_set_result() {
    let diagnostic = parse_single_file_ast_diagnostic("values ~= {1, 2, 3}\n~values.set(0, 9)\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidBuiltinCall {
            reason: InvalidBuiltinCallReason::MustHandleFallibleResult,
            ..
        }
    ));
}

#[test]
fn rejects_collection_set_fallback_value() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "values ~= {1, 2, 3}\n~values.set(0, 9) catch:\n    then 0\n;\n",
    );

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidResultHandling {
            reason: InvalidResultHandlingReason::FallbackValuesForErrorOnlyResult,
            ..
        }
    ));
}

#[test]
fn rejects_discarded_collection_remove_success() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "drop_removed || -> Error!:\n    values ~= {1, 2, 3}\n    ~values.remove(0)!\n;\n",
    );

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidResultHandling {
            reason: InvalidResultHandlingReason::SuccessValueDiscarded,
            ..
        }
    ));
}

// --------------------------
//  Removed builtin error helpers
// --------------------------

#[test]
fn rejects_removed_builtin_error_helper_methods() {
    let diagnostic =
        parse_single_file_ast_diagnostic("err = Error(\"bad\")\nvalue = err.bubble()\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidFieldAccess {
            reason: InvalidFieldAccessReason::UnknownMember,
            ..
        }
    ));
}

// --------------------------
//  Fixed collection literals
// --------------------------

#[test]
fn fixed_collection_literal_within_capacity_is_accepted() {
    let (ast, string_table) = parse_single_file_ast("items {2 Int} = {1, 2}\n");
    let body = start_function_body(&ast, &string_table);

    let NodeKind::VariableDeclaration(decl) = &body[0].kind else {
        panic!("expected declaration");
    };

    assert_eq!(
        decl.value.diagnostic_type.display_with_table(&string_table),
        "{2 Int}"
    );
}

#[test]
fn fixed_collection_literal_exceeding_capacity_is_rejected() {
    let diagnostic = parse_single_file_ast_diagnostic("items {2 Int} = {1, 2, 3}\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidCollectionType {
            reason: InvalidCollectionTypeReason::InitializerExceedsFixedCapacity { .. },
            ..
        }
    ));
}
