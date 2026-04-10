use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::test_support::{
    parse_single_file_ast, parse_single_file_ast_error, start_function_body,
};
use crate::compiler_frontend::compiler_errors::{ErrorMetaDataKey, ErrorType};
use crate::compiler_frontend::datatypes::DataType;

#[test]
fn rejects_assignment_value_type_mismatch_with_specific_details() {
    let error = parse_single_file_ast_error("value ~= 1\nvalue = true\n");

    assert_eq!(error.error_type, ErrorType::Type);
    assert!(
        error
            .msg
            .contains("Assignment to 'value' has incorrect value type"),
        "{}",
        error.msg
    );
    assert!(
        error.msg.contains("Expected 'Int', but found 'Bool'"),
        "{}",
        error.msg
    );
    assert!(error.msg.contains("Offending value: true"), "{}", error.msg);
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::ExpectedType)
            .map(String::as_str),
        Some("Int")
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::FoundType)
            .map(String::as_str),
        Some("Bool")
    );
}

#[test]
fn allows_int_to_float_assignment_via_contextual_coercion() {
    let (ast, string_table) = parse_single_file_ast("total ~= 1.5\ntotal = 2\n");
    let body = start_function_body(&ast, &string_table);

    let NodeKind::Assignment { value, .. } = &body[1].kind else {
        panic!("expected second statement to be an assignment");
    };

    assert_eq!(value.data_type, DataType::Float);
}
