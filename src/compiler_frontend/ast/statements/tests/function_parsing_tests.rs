//! Function-signature and call parsing regression tests.
//!
//! WHAT: validates the AST shapes produced for function declarations and call sites.
//! WHY: statement parsing should preserve signature metadata and host/user call dispatch.

use super::*;
use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::{ExpressionKind, ResultCallHandling};
use crate::compiler_frontend::ast::statements::functions::ReturnSlot;
use crate::compiler_frontend::ast::test_support::{
    function_body_by_name, function_signature_by_name, parse_single_file_ast,
    parse_single_file_ast_error, start_function_body,
};
use crate::compiler_frontend::datatypes::{DataType, Ownership};

#[test]
fn parses_function_parameters_and_return_types() {
    let (ast, string_table) =
        parse_single_file_ast("add |left Int, right Int| -> Int:\n    return left + right\n;\n");

    let signature = function_signature_by_name(&ast, &string_table, "add");

    assert_eq!(signature.parameters.len(), 2);
    assert_eq!(
        signature.parameters[0].id.name_str(&string_table),
        Some("left")
    );
    assert_eq!(
        signature.parameters[1].id.name_str(&string_table),
        Some("right")
    );
    assert_eq!(
        signature.returns,
        vec![ReturnSlot::success(FunctionReturn::Value(DataType::Int))]
    );
}

#[test]
fn parses_alias_return_candidates_in_function_signatures() {
    let (ast, string_table) = parse_single_file_ast(
        "choose |first String, fallback String| -> first or fallback:\n    return first\n;\n",
    );

    let signature = function_signature_by_name(&ast, &string_table, "choose");

    assert_eq!(signature.returns.len(), 1);
    assert_eq!(
        signature.returns[0],
        ReturnSlot::success(FunctionReturn::AliasCandidates {
            parameter_indices: vec![0, 1],
            data_type: DataType::StringSlice,
        })
    );
}

#[test]
fn start_function_distinguishes_user_and_host_calls() {
    let (ast, string_table) = parse_single_file_ast(
        "identity |value Int| -> Int:\n    return value\n;\n\nresult = identity(1)\nio(result)\n",
    );

    let body = start_function_body(&ast, &string_table);

    let NodeKind::VariableDeclaration(result_decl) = &body[0].kind else {
        panic!("expected variable declaration for function result");
    };
    assert!(matches!(
        result_decl.value.kind,
        ExpressionKind::FunctionCall(..)
    ));

    assert!(
        body.iter()
            .any(|node| matches!(node.kind, NodeKind::HostFunctionCall { .. })),
        "expected io(...) to remain a host-function statement"
    );

    let function_body = function_body_by_name(&ast, &string_table, "identity");
    assert!(
        function_body
            .iter()
            .any(|node| matches!(node.kind, NodeKind::Return(..))),
        "explicit function body should preserve return statements"
    );
}

#[test]
fn rejects_immutable_collection_argument_for_mutable_parameter() {
    let error = parse_single_file_ast_error(
        "touch |items ~{Int}| -> Int:\n    return items.length()\n;\n\nvalues = {1, 2, 3}\ncount = touch(values)\n",
    );

    assert!(
        error
            .msg
            .contains("Argument for parameter 'items' in Function 'touch' has incorrect type"),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_collection_element_type_mismatch_with_mutability_relaxation() {
    let error = parse_single_file_ast_error(
        "sum |items {Int}| -> Int:\n    return items.length()\n;\n\nvalues ~= {true, false}\ncount = sum(values)\n",
    );

    assert!(
        error
            .msg
            .contains("Argument for parameter 'items' in Function 'sum' has incorrect type"),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_struct_constructor_argument_type_with_field_wording() {
    let error = parse_single_file_ast_error(
        "Point = |\n    x Int,\n    y Int,\n|\n\npoint = Point(x = 1, y = \"oops\")\n",
    );

    assert!(
        error
            .msg
            .contains("Argument for field 'y' in Struct constructor 'Point' has incorrect type"),
        "{}",
        error.msg
    );
    assert!(
        error.msg.contains("Expected 'Int', but found 'String'"),
        "{}",
        error.msg
    );
    assert!(
        error.msg.contains("Offending value: \"oops\""),
        "{}",
        error.msg
    );
}

#[test]
fn resolves_named_struct_type_in_function_parameters() {
    let (ast, string_table) = parse_single_file_ast(
        "Point = |\n    x Int,\n|\n\nshow |value Point|:\n    io(value.x)\n;\n",
    );

    let signature = function_signature_by_name(&ast, &string_table, "show");
    assert!(matches!(
        signature.parameters[0].value.data_type,
        DataType::Struct {
            ownership: Ownership::MutableOwned,
            const_record: false,
            ..
        }
    ));
}

#[test]
fn resolves_named_struct_type_in_function_returns() {
    let (ast, string_table) = parse_single_file_ast(
        "Point = |\n    x Int,\n|\n\nclone |value Point| -> Point:\n    return value\n;\n",
    );

    let signature = function_signature_by_name(&ast, &string_table, "clone");
    assert!(matches!(
        signature.returns[0].value,
        FunctionReturn::Value(DataType::Struct {
            ownership: Ownership::MutableOwned,
            const_record: false,
            ..
        })
    ));
}

#[test]
fn rejects_unknown_named_type_in_function_signatures() {
    let error = parse_single_file_ast_error("use_missing |value Missing|:\n    return value\n;\n");

    assert!(error.msg.contains("Unknown type 'Missing'"));
}

#[test]
fn rejects_unknown_named_return_type_in_function_signatures() {
    let error = parse_single_file_ast_error("clone |value Int| -> Missing:\n    return value\n;\n");

    assert!(error.msg.contains("Unknown type 'Missing'"));
}

#[test]
fn rejects_receiver_parameter_when_not_first() {
    let error = parse_single_file_ast_error(
        "Point = |\n    x Int,\n|\n\nreset |value Int, this Point|:\n    return value\n;\n",
    );

    assert!(error.msg.contains("not the first parameter"));
}

#[test]
fn allows_builtin_scalar_receiver_parameter() {
    let (ast, string_table) =
        parse_single_file_ast("reset |this Int| -> Int:\n    return this\n;\n");

    let signature = function_signature_by_name(&ast, &string_table, "reset");
    assert_eq!(signature.parameters[0].value.data_type, DataType::Int);
}

#[test]
fn rejects_multiple_this_parameters_in_one_signature() {
    let error = parse_single_file_ast_error(
        "Point = |\n    x Int = 0,\n|\n\nlength |this Point, this Point| -> Int:\n    return this.x\n;\n",
    );

    assert!(
        error.msg.contains("declares 'this' more than once"),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_field_method_name_collisions() {
    let error = parse_single_file_ast_error(
        "Point = |\n    reset Int = 0,\n|\n\nreset |this Point| -> Int:\n    return this.reset\n;\n",
    );

    assert!(error.msg.contains("declares both a field and method named"));
}

#[test]
fn rejects_free_function_call_syntax_for_receiver_methods() {
    let error = parse_single_file_ast_error(
        "Point = |\n    x Int = 0,\n|\n\nreset |this Point|:\n    return\n;\n\npoint ~= Point()\nreset(point)\n",
    );

    assert!(error.msg.contains("cannot be called as a free function"));
}

#[test]
fn rejects_free_function_call_syntax_for_builtin_receiver_methods() {
    let error = parse_single_file_ast_error(
        "double |this Int| -> Int:\n    return this + this\n;\n\ndouble(21)\n",
    );

    assert!(error.msg.contains("cannot be called as a free function"));
    assert!(error.msg.contains("for 'Int'"), "{}", error.msg);
}

#[test]
fn rejects_const_record_method_calls() {
    let error = parse_single_file_ast_error(
        "Point = |\n    x Int = 0,\n|\n\nlength |this Point| -> Int:\n    return this.x\n;\n\n#origin = Point()\norigin.length()\n",
    );

    assert!(
        error
            .msg
            .contains("data-only and do not support runtime method calls"),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_mutable_receiver_methods_on_temporaries() {
    let error = parse_single_file_ast_error(
        "Point = |\n    x Int = 0,\n|\n\nreset |this ~Point|:\n    this.x = 0\n;\n\nmake || -> Point:\n    return Point()\n;\n\nmake().reset()\n",
    );

    assert!(
        error.msg.contains("requires a mutable place receiver"),
        "{}",
        error.msg
    );
}

#[test]
fn parses_mutable_receiver_methods_with_explicit_receiver_tilde() {
    let (ast, string_table) = parse_single_file_ast(
        "Point = |\n    x Int = 0,\n|\n\nreset |this ~Point|:\n    this.x = 0\n;\n\npoint ~= Point()\n~point.reset()\n",
    );

    let body = start_function_body(&ast, &string_table);
    let NodeKind::Rvalue(call_expr) = &body[1].kind else {
        panic!("expected receiver method statement");
    };
    let ExpressionKind::Runtime(nodes) = &call_expr.kind else {
        panic!("expected runtime receiver call expression");
    };
    assert!(
        matches!(nodes[0].kind, NodeKind::MethodCall { builtin: None, .. }),
        "expected user-defined receiver method call node"
    );
}

#[test]
fn rejects_mutable_receiver_methods_without_explicit_receiver_tilde() {
    let error = parse_single_file_ast_error(
        "Point = |\n    x Int = 0,\n|\n\nreset |this ~Point|:\n    this.x = 0\n;\n\npoint ~= Point()\npoint.reset()\n",
    );

    assert!(
        error
            .msg
            .contains("expects mutable access at the receiver call site"),
        "{}",
        error.msg
    );
    assert!(error.msg.contains("~point"), "{}", error.msg);
}

#[test]
fn rejects_explicit_receiver_tilde_for_shared_receiver_methods() {
    let error = parse_single_file_ast_error(
        "Point = |\n    x Int = 0,\n|\n\nlength |this Point| -> Int:\n    return this.x\n;\n\npoint ~= Point()\nvalue = ~point.length()\n",
    );

    assert!(
        error
            .msg
            .contains("does not accept explicit mutable access marker '~'"),
        "{}",
        error.msg
    );
}

#[test]
fn parses_result_propagation_call_in_expression_position() {
    let (ast, string_table) = parse_single_file_ast(
        "can_error |value String| -> String, Error!:\n    return value\n;\n\nforward |value String| -> String, Error!:\n    return can_error(value)!\n;\n",
    );

    let body = function_body_by_name(&ast, &string_table, "forward");
    let NodeKind::Return(values) = &body[0].kind else {
        panic!("expected return statement in forward()");
    };

    assert_eq!(values.len(), 1);
    assert!(matches!(
        values[0].kind,
        ExpressionKind::ResultHandledFunctionCall {
            handling: ResultCallHandling::Propagate,
            ..
        }
    ));
}

#[test]
fn rejects_result_propagation_when_error_slot_types_do_not_match() {
    let error = parse_single_file_ast_error(
        "ErrA = |\n    message String,\n|\n\nErrB = |\n    message String,\n|\n\ninner || -> Int, ErrA!:\n    return! ErrA(\"failed\")\n;\n\nouter || -> Int, ErrB!:\n    value = inner()!\n    return value\n;\n",
    );

    assert!(
        error.msg.contains("Mismatched propagated error type"),
        "{}",
        error.msg
    );
    assert!(
        error.msg.contains("Expected 'ErrB', but found 'ErrA'"),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_return_value_type_with_expected_found_and_value_details() {
    let error = parse_single_file_ast_error("render || -> String:\n    return 1\n;\n\nrender()\n");

    assert!(
        error.msg.contains("Return value 1 has incorrect type"),
        "{}",
        error.msg
    );
    assert!(
        error.msg.contains("Expected 'String', but found 'Int'"),
        "{}",
        error.msg
    );
    assert!(error.msg.contains("Offending value: 1"), "{}", error.msg);
}

#[test]
fn parses_result_fallback_call_in_expression_position() {
    let (ast, string_table) = parse_single_file_ast(
        "can_error |value String| -> String, Error!:\n    return value\n;\n\nrecover |value String| -> String:\n    return can_error(value) ! \"fallback\"\n;\n",
    );

    let body = function_body_by_name(&ast, &string_table, "recover");
    let NodeKind::Return(values) = &body[0].kind else {
        panic!("expected return statement in recover()");
    };

    assert_eq!(values.len(), 1);
    let ExpressionKind::ResultHandledFunctionCall { handling, .. } = &values[0].kind else {
        panic!("expected handled call expression in recover return");
    };

    let ResultCallHandling::Fallback(fallback_values) = handling else {
        panic!("expected fallback handling");
    };
    assert_eq!(fallback_values.len(), 1);
    assert!(matches!(
        fallback_values[0].kind,
        ExpressionKind::StringSlice(_)
    ));
}

#[test]
fn rejects_bare_named_error_handler_without_scope() {
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
fn parses_named_handler_with_fallback_scope_in_declaration_rhs() {
    let (ast, string_table) = parse_single_file_ast(
        "can_error |value String| -> String, Error!:\n    return value\n;\n\nrecover |value String| -> String:\n    output = can_error(value) err! \"fallback\":\n        io(err.message)\n    ;\n    return output\n;\n",
    );

    let body = function_body_by_name(&ast, &string_table, "recover");
    let NodeKind::VariableDeclaration(output_decl) = &body[0].kind else {
        panic!("expected declaration statement in recover()");
    };

    let ExpressionKind::ResultHandledFunctionCall { handling, .. } = &output_decl.value.kind else {
        panic!("expected handled call expression in recover declaration");
    };

    let ResultCallHandling::Handler {
        error_name: _,
        error_binding: _,
        fallback,
        body,
    } = handling
    else {
        panic!("expected named-handler call handling");
    };

    let Some(fallback_values) = fallback else {
        panic!("expected handler fallback values");
    };
    assert_eq!(fallback_values.len(), 1);
    assert_eq!(body.len(), 1);
}

#[test]
fn rejects_fallthrough_named_handler_without_fallback_when_values_are_required() {
    let error = parse_single_file_ast_error(
        "can_error |value String| -> String, Error!:\n    return value\n;\n\nrecover |value String| -> String:\n    return can_error(value) err!:\n        io(err.message)\n    ;\n;\n",
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
fn parses_standalone_result_propagation_statement() {
    let (ast, string_table) = parse_single_file_ast(
        "can_error || -> Error!:\n    return\n;\n\nrun || -> Error!:\n    can_error()!\n;\n",
    );

    let body = function_body_by_name(&ast, &string_table, "run");
    let NodeKind::Rvalue(expression) = &body[0].kind else {
        panic!("expected standalone handled call to parse as rvalue statement");
    };
    assert!(matches!(
        expression.kind,
        ExpressionKind::ResultHandledFunctionCall {
            handling: ResultCallHandling::Propagate,
            ..
        }
    ));
}
