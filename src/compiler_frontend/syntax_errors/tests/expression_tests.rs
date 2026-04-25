use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::tests::test_support::parse_single_file_ast_error;

#[test]
fn detects_double_equal_as_equality_mistake() {
    let err = parse_single_file_ast_error("value = 1 == 2\n");
    assert_eq!(err.error_type, ErrorType::Syntax);
    assert!(err.msg.contains("is"), "msg: {}", err.msg);
    assert!(err.msg.contains("=="), "msg: {}", err.msg);
}

#[test]
fn detects_bang_equal_as_inequality_mistake() {
    let err = parse_single_file_ast_error("value = 1 != 2\n");
    assert_eq!(err.error_type, ErrorType::Syntax);
    assert!(err.msg.contains("is not"), "msg: {}", err.msg);
    assert!(err.msg.contains("!="), "msg: {}", err.msg);
}

#[test]
fn detects_and_and_as_conjunction_mistake() {
    let err = parse_single_file_ast_error("value = true && false\n");
    assert_eq!(err.error_type, ErrorType::Syntax);
    assert!(err.msg.contains("and"), "msg: {}", err.msg);
    assert!(err.msg.contains("&&"), "msg: {}", err.msg);
}

#[test]
fn detects_or_or_as_disjunction_mistake() {
    let err = parse_single_file_ast_error("value = true || false\n");
    assert_eq!(err.error_type, ErrorType::Syntax);
    assert!(err.msg.contains("or"), "msg: {}", err.msg);
    assert!(err.msg.contains("||"), "msg: {}", err.msg);
}

#[test]
fn detects_bang_as_boolean_negation_mistake() {
    let err = parse_single_file_ast_error("value = !true\n");
    assert_eq!(err.error_type, ErrorType::Syntax);
    assert!(err.msg.contains("not"), "msg: {}", err.msg);
}
