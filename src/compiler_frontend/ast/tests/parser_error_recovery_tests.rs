//! Malformed-input parser regression tests.
//!
//! WHAT: asserts that common malformed inputs fail with stable diagnostic classes and message
//! fragments.
//! WHY: parser changes should not silently degrade recovery paths or produce vague errors.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::statements::branching::MatchPattern;
use crate::compiler_frontend::compiler_errors::{ErrorMetaDataKey, ErrorType};
use crate::compiler_frontend::tests::test_support::{
    parse_single_file_ast, parse_single_file_ast_error, start_function_body,
};

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
        parse_single_file_ast_error("value = 1\nif value is:\n    case _ => io(\"one\")\n;\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error
            .msg
            .contains("Wildcard patterns in 'case' arms are not supported")
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::CompilationStage)
            .map(String::as_str),
        Some("Match Statement Parsing")
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::PrimarySuggestion)
            .map(String::as_str),
        Some("Replace 'case _ =>' with 'else =>'.")
    );
    assert!(error.location.start_pos.char_column > 0);
}

#[test]
fn reports_labeled_scopes_as_deferred_rule_errors() {
    let error = parse_single_file_ast_error("label:\n    io(\"x\")\n;\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(error.msg.contains("Labeled scopes are deferred for Alpha."));
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::CompilationStage)
            .map(String::as_str),
        Some("Variable Declaration")
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::PrimarySuggestion)
            .map(String::as_str),
        Some("Remove the label and use supported control flow syntax.")
    );
    assert!(error.location.start_pos.char_column > 0);
}

#[test]
fn reports_unterminated_match_scope_at_end_of_file() {
    let error = parse_single_file_ast_error("value = 1\nif value is:\n    case 1 => io(\"one\")\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error
            .msg
            .contains("Unexpected end of file in match statement")
    );
}

#[test]
fn reports_case_outside_match_scope() {
    let error = parse_single_file_ast_error("case 1 => io(\"one\")\n");

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
fn reports_multi_bind_with_variable_rhs_rejected() {
    let error = parse_single_file_ast_error("value ~= 1\na, b = value\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error
            .msg
            .contains("Multi-bind is only supported for explicit multi-value surfaces"),
        "{}",
        error.msg
    );
}

#[test]
fn reports_multi_bind_with_literal_rhs_rejected() {
    let error = parse_single_file_ast_error("a, b = 1\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error
            .msg
            .contains("Multi-bind is only supported for explicit multi-value surfaces"),
        "{}",
        error.msg
    );
}

#[test]
fn reports_multi_bind_with_field_access_rhs_rejected() {
    let error = parse_single_file_ast_error(
        "Thing = |\n    x Int,\n    y Int,\n|\nthing ~= Thing(1, 2)\na, b = thing.x\n",
    );

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error
            .msg
            .contains("Multi-bind is only supported for explicit multi-value surfaces"),
        "{}",
        error.msg
    );
}

#[test]
fn reports_reserved_must_keyword_in_function_body() {
    let error = parse_single_file_ast_error("must = 1\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(error.msg.contains("Keyword 'must' is reserved for traits"));
    assert!(error.msg.contains("deferred for Alpha"));
    assert!(error.location.start_pos.char_column > 0);
}

#[test]
fn reports_reserved_this_keyword_in_function_body_statement_position() {
    let error = parse_single_file_ast_error("#f||:\n    This\n;\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(error.msg.contains("Keyword 'This' is reserved for traits"));
    assert!(error.msg.contains("deferred for Alpha"));
}

#[test]
fn reports_reserved_this_keyword_in_declaration_type_position() {
    let error = parse_single_file_ast_error("value This = 1\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(error.msg.contains("Keyword 'This' is reserved for traits"));
    assert!(error.msg.contains("deferred for Alpha"));
}

#[test]
fn reports_reserved_must_keyword_in_expression_position() {
    let error = parse_single_file_ast_error("value = must\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(error.msg.contains("Keyword 'must' is reserved for traits"));
    assert!(error.msg.contains("deferred for Alpha"));
}

#[test]
fn reports_reserved_this_keyword_in_expression_position() {
    let error = parse_single_file_ast_error("value = This\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(error.msg.contains("Keyword 'This' is reserved for traits"));
    assert!(error.msg.contains("deferred for Alpha"));
}

#[test]
fn reports_reserved_must_keyword_in_copy_place_position() {
    let error = parse_single_file_ast_error("value = copy must\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(error.msg.contains("Keyword 'must' is reserved for traits"));
    assert!(error.msg.contains("deferred for Alpha"));
}

#[test]
fn reports_reserved_must_keyword_in_signature_member_position() {
    let error = parse_single_file_ast_error("#sum |must Int| -> Int:\n    return 1\n;\n");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(error.msg.contains("Keyword 'must' is reserved for traits"));
    assert!(error.msg.contains("deferred for Alpha"));
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::CompilationStage)
            .map(String::as_str),
        Some("Struct/Parameter Parsing")
    );
}

#[test]
fn reports_reserved_must_keyword_in_postfix_member_position() {
    let error = parse_single_file_ast_error(
        "Point = |\n    value Int = 1,\n|\n\npoint ~= Point()\nvalue = point.must\n",
    );

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(error.msg.contains("Keyword 'must' is reserved for traits"));
    assert!(error.msg.contains("deferred for Alpha"));
}
