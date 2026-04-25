use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::tests::test_support::parse_single_file_ast_error;

#[test]
fn detects_int_divide_at_statement_start_as_comment_mistake() {
    let err = parse_single_file_ast_error("// comment\n");
    assert_eq!(err.error_type, ErrorType::Syntax);
    assert!(err.msg.contains("//"), "msg: {}", err.msg);
    assert!(err.msg.contains("--"), "msg: {}", err.msg);
}

#[test]
fn detects_fn_keyword_as_function_declaration_mistake() {
    let err = parse_single_file_ast_error("fn hello():\n");
    assert_eq!(err.error_type, ErrorType::Syntax);
    assert!(err.msg.contains("fn"), "msg: {}", err.msg);
    assert!(err.msg.contains("keyword prefix"), "msg: {}", err.msg);
}

#[test]
fn detects_let_keyword_as_declaration_mistake() {
    let err = parse_single_file_ast_error("let x = 1\n");
    assert_eq!(err.error_type, ErrorType::Syntax);
    assert!(err.msg.contains("let"), "msg: {}", err.msg);
}

#[test]
fn detects_match_keyword_as_pattern_matching_mistake() {
    let err = parse_single_file_ast_error("match x:\n");
    assert_eq!(err.error_type, ErrorType::Syntax);
    assert!(err.msg.contains("match"), "msg: {}", err.msg);
    assert!(err.msg.contains("if value is:"), "msg: {}", err.msg);
}

#[test]
fn detects_struct_keyword_as_struct_declaration_mistake() {
    let err = parse_single_file_ast_error("struct Name { }\n");
    assert_eq!(err.error_type, ErrorType::Syntax);
    assert!(err.msg.contains("struct"), "msg: {}", err.msg);
    assert!(err.msg.contains("="), "msg: {}", err.msg);
}
