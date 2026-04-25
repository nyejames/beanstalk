use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::tests::test_support::parse_single_file_ast_error;

#[test]
fn detects_open_parenthesis_as_parameter_delimiter_mistake() {
    let err = parse_single_file_ast_error("bad |a Int, (b Int)| -> Int:\n    return a\n;\n");
    assert_eq!(err.error_type, ErrorType::Syntax);
    assert!(err.msg.contains("|"), "msg: {}", err.msg);
    assert!(err.msg.contains("()"), "msg: {}", err.msg);
}
