//! Choice expression parsing tests.
//!
//! WHAT: validates `Choice::Variant` expression resolution and diagnostics.
//! WHY: alpha choices are unit-variant-only and must fail fast for unknown/deferred forms.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::tests::test_support::{
    function_body_by_name, parse_single_file_ast, parse_single_file_ast_error, start_function_body,
};

#[test]
fn resolves_choice_variant_expressions_with_choice_types() {
    let (ast, string_table) = parse_single_file_ast(
        "#Status :: Ready, Busy;\n\
         echo_status |status Status| -> Status:\n\
             return status\n\
         ;\n\
         make_status || -> Status:\n\
             selected = Status::Busy\n\
             return echo_status(selected)\n\
         ;\n\
         current Status = Status::Ready\n\
         next = make_status()\n",
    );

    let start_body = start_function_body(&ast, &string_table);
    let current_declaration = start_body
        .iter()
        .find_map(|node| match &node.kind {
            NodeKind::VariableDeclaration(declaration)
                if declaration.id.name_str(&string_table) == Some("current") =>
            {
                Some(declaration)
            }
            _ => None,
        })
        .expect("expected 'current' declaration in start function");

    assert!(
        matches!(current_declaration.value.kind, ExpressionKind::Int(0)),
        "expected Status::Ready to lower to deterministic variant tag 0"
    );
    assert!(
        matches!(
            &current_declaration.value.data_type,
            DataType::Choices(variants) if variants.len() == 2
        ),
        "choice literal should keep choice type identity"
    );

    let make_status_body = function_body_by_name(&ast, &string_table, "make_status");
    let selected_declaration = make_status_body
        .iter()
        .find_map(|node| match &node.kind {
            NodeKind::VariableDeclaration(declaration)
                if declaration.id.name_str(&string_table) == Some("selected") =>
            {
                Some(declaration)
            }
            _ => None,
        })
        .expect("expected 'selected' declaration in make_status");

    assert!(
        matches!(selected_declaration.value.kind, ExpressionKind::Int(1)),
        "expected Status::Busy to lower to deterministic variant tag 1"
    );
    assert!(
        matches!(
            &selected_declaration.value.data_type,
            DataType::Choices(variants) if variants.len() == 2
        ),
        "choice literal should preserve declaration-backed choice type"
    );
}

#[test]
fn reports_unknown_choice_variant_with_targeted_diagnostic() {
    let error = parse_single_file_ast_error(
        "#Status :: Ready, Busy;\n\
         value = Status::Unknown\n",
    );

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error.msg.contains("Unknown variant 'Status::Unknown'"),
        "{}",
        error.msg
    );
}

#[test]
fn reports_missing_variant_name_after_choice_separator() {
    let error = parse_single_file_ast_error(
        "#Status :: Ready, Busy;\n\
         value = Status::\n",
    );

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error
            .msg
            .contains("Expected a variant name after 'Status::'"),
        "{}",
        error.msg
    );
}
