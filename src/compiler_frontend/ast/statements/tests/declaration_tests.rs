//! Variable declaration parsing regression tests.
//!
//! WHAT: validates mutability, explicit types, and named-type annotations in declarations.
//! WHY: declaration parsing is the entrypoint for most AST values and must preserve type intent.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::test_support::{
    parse_single_file_ast, parse_single_file_ast_error, start_function_body,
};
use crate::compiler_frontend::datatypes::{DataType, Ownership};

#[test]
fn parses_mutable_and_explicitly_typed_declarations() {
    let (ast, string_table) = parse_single_file_ast("count ~= 1\nname String = \"Ada\"\n");

    let body = start_function_body(&ast, &string_table);

    let NodeKind::VariableDeclaration(count_decl) = &body[0].kind else {
        panic!("expected mutable declaration");
    };
    assert_eq!(count_decl.value.data_type, DataType::Int);
    assert_eq!(count_decl.value.ownership, Ownership::MutableOwned);

    let NodeKind::VariableDeclaration(name_decl) = &body[1].kind else {
        panic!("expected explicit string declaration");
    };
    assert_eq!(name_decl.value.data_type, DataType::StringSlice);
    assert!(matches!(
        name_decl.value.kind,
        ExpressionKind::StringSlice(..)
    ));
}

#[test]
fn resolves_named_type_annotations_against_prior_structs() {
    let (ast, string_table) =
        parse_single_file_ast("Point = |\n    x Int,\n|\n\norigin Point = Point(0)\n");

    let body = start_function_body(&ast, &string_table);

    let NodeKind::VariableDeclaration(origin_decl) = &body[0].kind else {
        panic!("expected typed declaration");
    };
    assert!(matches!(
        origin_decl.value.data_type,
        DataType::Struct {
            ownership: Ownership::MutableOwned,
            const_record: false,
            ..
        }
    ));
    assert!(matches!(
        origin_decl.value.kind,
        ExpressionKind::StructInstance(..)
    ));
}

#[test]
fn rejects_user_declarations_named_error() {
    let error = parse_single_file_ast_error("Error = 1\n");
    assert!(error.msg.contains("reserved"), "{}", error.msg);
    assert!(error.msg.contains("Error"), "{}", error.msg);
}

#[test]
fn rejects_struct_redefinition_of_reserved_error_symbol() {
    let error = parse_single_file_ast_error("Error = |\n    message String,\n|\n");
    assert!(error.msg.contains("reserved"), "{}", error.msg);
    assert!(error.msg.contains("Error"), "{}", error.msg);
}

#[test]
fn rejects_keyword_shadow_variable_declarations() {
    let error = parse_single_file_ast_error("_true = 1\n");
    assert!(
        error.msg.contains(
            "Identifier '_true' is reserved because it visually shadows language keyword 'true'"
        ),
        "{}",
        error.msg
    );
}
