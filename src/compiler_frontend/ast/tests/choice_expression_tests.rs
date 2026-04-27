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

    let (nominal_path, variant, tag) = match &current_declaration.value.kind {
        ExpressionKind::ChoiceConstruct {
            nominal_path,
            variant,
            tag,
            ..
        } => (nominal_path, *variant, *tag),
        other => panic!("expected ChoiceConstruct, got {:?}", other),
    };
    assert_eq!(tag, 0, "expected Status::Ready to have tag 0");
    assert_eq!(
        nominal_path.name_str(&string_table),
        Some("Status"),
        "expected nominal path to be Status"
    );
    assert_eq!(
        string_table.resolve(variant),
        "Ready",
        "expected variant name to be Ready"
    );
    assert!(
        matches!(
            &current_declaration.value.data_type,
            DataType::Choices {
                nominal_path,
                variants,
            } if nominal_path.name_str(&string_table) == Some("Status") && variants.len() == 2
        ),
        "choice literal should keep declaration-backed choice identity"
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

    let (nominal_path, variant, tag) = match &selected_declaration.value.kind {
        ExpressionKind::ChoiceConstruct {
            nominal_path,
            variant,
            tag,
            ..
        } => (nominal_path, *variant, *tag),
        other => panic!("expected ChoiceConstruct, got {:?}", other),
    };
    assert_eq!(tag, 1, "expected Status::Busy to have tag 1");
    assert_eq!(
        nominal_path.name_str(&string_table),
        Some("Status"),
        "expected nominal path to be Status"
    );
    assert_eq!(
        string_table.resolve(variant),
        "Busy",
        "expected variant name to be Busy"
    );
    assert!(
        matches!(
            &selected_declaration.value.data_type,
            DataType::Choices {
                nominal_path,
                variants,
            } if nominal_path.name_str(&string_table) == Some("Status") && variants.len() == 2
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
