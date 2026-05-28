//! Mutation expression parsing and validation regression tests.
//!
//! WHAT: validates mutable assignment, field mutation, collection mutation, and place-expression
//!       requirements.
//! WHY: mutation rules are tightly coupled to borrow checking; parser-level tests ensure the
//!      frontend produces the right AST shapes for later analysis.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::compiler_messages::{
    DiagnosticPayload, TypeMismatchContext, UnsupportedOperatorCategory,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::tests::test_support::{
    parse_single_file_ast, parse_single_file_ast_diagnostic, start_function_body,
};

#[test]
fn rejects_assignment_value_type_mismatch_with_specific_details() {
    assert_assignment_type_mismatch("value ~= 1\nvalue = true\n");
}

#[test]
fn allows_int_to_float_assignment_via_contextual_coercion() {
    let (ast, string_table) = parse_single_file_ast("total ~= 1.5\ntotal = 2\n");
    let body = start_function_body(&ast, &string_table);

    let NodeKind::Assignment { value, .. } = &body[1].kind else {
        panic!("expected second statement to be an assignment");
    };

    assert_eq!(value.diagnostic_type, DataType::Float);
}

#[test]
fn rejects_int_divide_assign_when_regular_division_returns_float() {
    assert_assignment_type_mismatch("value ~Int = 10\nvalue /= 4\n");
}

#[test]
fn allows_int_integer_divide_assign() {
    let (ast, string_table) = parse_single_file_ast("value ~Int = 10\nvalue //= 4\n");
    let body = start_function_body(&ast, &string_table);

    let NodeKind::Assignment { value, .. } = &body[1].kind else {
        panic!("expected second statement to be an assignment");
    };

    assert_eq!(value.diagnostic_type, DataType::Int);
}

#[test]
fn allows_float_divide_assign_int_rhs() {
    let (ast, string_table) = parse_single_file_ast("value ~Float = 10\nvalue /= 4\n");
    let body = start_function_body(&ast, &string_table);

    let NodeKind::Assignment { value, .. } = &body[1].kind else {
        panic!("expected second statement to be an assignment");
    };

    assert_eq!(value.diagnostic_type, DataType::Float);
}

#[test]
fn rejects_float_integer_divide_assign_rhs() {
    let diagnostic = parse_single_file_ast_diagnostic("value ~Float = 10\nvalue //= 4\n");

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::UnsupportedOperatorTypes {
            category: UnsupportedOperatorCategory::Arithmetic,
            ..
        }
    ));
}

fn assert_assignment_type_mismatch(source: &str) {
    let diagnostic = parse_single_file_ast_diagnostic(source);

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::TypeMismatch {
            context: TypeMismatchContext::Assignment,
            ..
        }
    ));
}
