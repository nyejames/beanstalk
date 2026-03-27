//! Variable declaration parsing regression tests.
//!
//! WHAT: validates mutability, explicit types, and named-type annotations in declarations.
//! WHY: declaration parsing is the entrypoint for most AST values and must preserve type intent.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::test_support::{parse_single_file_ast, start_function_body};
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
        DataType::Struct(_, Ownership::MutableOwned)
    ));
    assert!(matches!(
        origin_decl.value.kind,
        ExpressionKind::StructInstance(..)
    ));
}
