//! Function-signature and call parsing regression tests.
//!
//! WHAT: validates the AST shapes produced for function declarations and call sites.
//! WHY: statement parsing should preserve signature metadata and host/user call dispatch.

use super::*;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::test_support::{
    function_body_by_name, function_signature_by_name, parse_single_file_ast, start_function_body,
};
use crate::compiler_frontend::datatypes::DataType;

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
        vec![FunctionReturn::Value(DataType::Int)]
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
        FunctionReturn::AliasCandidates {
            parameter_indices: vec![0, 1],
            data_type: DataType::StringSlice,
        }
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
