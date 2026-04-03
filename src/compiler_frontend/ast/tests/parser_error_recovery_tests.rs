//! Malformed-input parser regression tests.
//!
//! WHAT: asserts that common malformed inputs fail with stable diagnostic classes and message
//! fragments.
//! WHY: parser changes should not silently degrade recovery paths or produce vague errors.

use crate::compiler_frontend::ast::test_support::parse_single_file_ast_error;
use crate::compiler_frontend::compiler_errors::ErrorType;

#[test]
fn reports_missing_signature_colon() {
    let error = parse_single_file_ast_error("#f|| -> Int\n;\n");

    assert_eq!(error.error_type, ErrorType::Syntax);
    assert!(
        error
            .msg
            .contains("Function return declarations must end with ':'")
    );
}

#[test]
fn reports_stray_comma_in_function_body() {
    let error = parse_single_file_ast_error("#broken||:\n    value = 1\n    ,\n;\n");

    assert_eq!(error.error_type, ErrorType::Syntax);
    assert!(error.msg.contains("Unexpected ',' in function body"));
}

#[test]
fn reports_wildcard_match_arms_as_syntax_errors() {
    let error = parse_single_file_ast_error("value = 1\nif value is:\n    _: io(\"one\");\n;\n");

    assert_eq!(error.error_type, ErrorType::Syntax);
    assert!(error.msg.contains("Wildcard '_' arms are not supported"));
}

#[test]
fn reports_unterminated_match_scope_at_end_of_file() {
    let error = parse_single_file_ast_error("value = 1\nif value is:\n    1: io(\"one\");\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error
            .msg
            .contains("Unexpected end of file in match statement")
    );
}

#[test]
fn reports_multi_bind_malformed_comma_sequence() {
    let error = parse_single_file_ast_error(
        "pair || -> Int, Int:\n    return 1, 2\n;\n\na, , b = pair()\n",
    );

    assert_eq!(error.error_type, ErrorType::Syntax);
    assert!(error.msg.contains("Malformed multi-bind target list"));
}

#[test]
fn reports_multi_bind_mutable_target_without_explicit_type() {
    let error =
        parse_single_file_ast_error("pair || -> Int, Int:\n    return 1, 2\n;\n\na, b ~= pair()\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error.msg.contains("requires an explicit type annotation"),
        "{}",
        error.msg
    );
}
