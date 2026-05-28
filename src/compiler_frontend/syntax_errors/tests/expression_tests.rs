use crate::compiler_frontend::compiler_messages::{CommonSyntaxMistakeReason, DiagnosticPayload};
use crate::compiler_frontend::tests::test_support::parse_single_file_ast_diagnostic;

fn assert_common_syntax_mistake(source: &str, expected_reason: CommonSyntaxMistakeReason) {
    let diagnostic = parse_single_file_ast_diagnostic(source);

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::CommonSyntaxMistake { reason } if reason == expected_reason
    ));
}

#[test]
fn detects_double_equal_as_equality_mistake() {
    assert_common_syntax_mistake(
        "value = 1 == 2\n",
        CommonSyntaxMistakeReason::EqualityOperator,
    );
}

#[test]
fn detects_bang_equal_as_inequality_mistake() {
    assert_common_syntax_mistake(
        "value = 1 != 2\n",
        CommonSyntaxMistakeReason::InequalityOperator,
    );
}

#[test]
fn detects_and_and_as_conjunction_mistake() {
    assert_common_syntax_mistake(
        "value = true && false\n",
        CommonSyntaxMistakeReason::LogicalAndOperator,
    );
}

#[test]
fn detects_or_or_as_disjunction_mistake() {
    assert_common_syntax_mistake(
        "value = true || false\n",
        CommonSyntaxMistakeReason::LogicalOrOperator,
    );
}

#[test]
fn detects_bang_as_boolean_negation_mistake() {
    assert_common_syntax_mistake(
        "value = !true\n",
        CommonSyntaxMistakeReason::BooleanBangNegation,
    );
}
