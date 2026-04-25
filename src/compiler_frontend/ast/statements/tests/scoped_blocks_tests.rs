use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::tests::test_support::{
    parse_single_file_ast, parse_single_file_ast_error, start_function_body,
};

#[test]
fn parses_keyword_scoped_block_as_own_ast_node() {
    let (ast, string_table) = parse_single_file_ast("block:\n    value = \"inside\"\n;\n");
    let body = start_function_body(&ast, &string_table);

    let NodeKind::ScopedBlock { body: block_body } = &body[0].kind else {
        panic!("expected scoped block node");
    };

    assert_eq!(block_body.len(), 1);
    assert!(matches!(
        block_body[0].kind,
        NodeKind::VariableDeclaration(_)
    ));
}

#[test]
fn rejects_block_keyword_as_declaration_name() {
    let error = parse_single_file_ast_error("block = 1\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error
            .msg
            .contains("'block' is a reserved keyword and cannot be used as a declaration name"),
        "{}",
        error.msg
    );
}

#[cfg(feature = "checked_blocks")]
#[test]
fn checked_block_feature_still_reports_unimplemented() {
    let error = parse_single_file_ast_error("checked:\n    value = 1\n;\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(error.msg.contains("not implemented yet"), "{}", error.msg);
    assert!(
        !error.msg.contains("behind the `checked_blocks` feature"),
        "{}",
        error.msg
    );
}

#[cfg(feature = "async_blocks")]
#[test]
fn async_block_feature_still_reports_unimplemented() {
    let error = parse_single_file_ast_error("async:\n    value = 1\n;\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(error.msg.contains("not implemented yet"), "{}", error.msg);
    assert!(
        !error.msg.contains("behind the `async_blocks` feature"),
        "{}",
        error.msg
    );
}
