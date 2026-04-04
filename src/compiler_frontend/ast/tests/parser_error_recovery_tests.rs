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
fn reports_wildcard_match_arms_as_deferred_rule_errors() {
    let error =
        parse_single_file_ast_error("value = 1\nif value is:\n    case _ => io(\"one\");\n;\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error
            .msg
            .contains("Wildcard patterns ('case _ =>') are deferred")
    );
}

#[test]
fn reports_unterminated_match_scope_at_end_of_file() {
    let error =
        parse_single_file_ast_error("value = 1\nif value is:\n    case 1 => io(\"one\");\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error
            .msg
            .contains("Unexpected end of file in match statement")
    );
}

#[test]
fn reports_case_outside_match_scope() {
    let error = parse_single_file_ast_error("case 1 => io(\"one\");\n");

    assert_eq!(error.error_type, ErrorType::Syntax);
    assert!(
        error.msg.contains("Unexpected 'case' in function body"),
        "{}",
        error.msg
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

#[test]
fn reports_reserved_must_keyword_in_function_body() {
    let error = parse_single_file_ast_error("must = 1\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(error.msg.contains("'must' is reserved for traits"));
    assert!(error.msg.contains("not implemented yet in Alpha"));
}

#[test]
fn reports_reserved_this_keyword_in_declaration_type_position() {
    let error = parse_single_file_ast_error("value This = 1\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(error.msg.contains("'This' is reserved for traits"));
    assert!(error.msg.contains("not implemented yet in Alpha"));
}
