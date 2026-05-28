use crate::compiler_frontend::compiler_messages::{CommonSyntaxMistakeReason, DiagnosticPayload};
use crate::compiler_frontend::tests::test_support::parse_single_file_ast_diagnostic;

fn assert_common_syntax_mistake(
    source: &str,
    expected: impl FnOnce(CommonSyntaxMistakeReason) -> bool,
) {
    let diagnostic = parse_single_file_ast_diagnostic(source);

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::CommonSyntaxMistake { reason } if expected(reason.clone())
    ));
}

#[test]
fn detects_int_divide_at_statement_start_as_comment_mistake() {
    assert_common_syntax_mistake("// comment\n", |reason| {
        reason == CommonSyntaxMistakeReason::StatementLineComment
    });
}

#[test]
fn detects_fn_keyword_as_function_declaration_mistake() {
    assert_common_syntax_mistake("fn hello():\n", |reason| {
        matches!(reason, CommonSyntaxMistakeReason::FunctionKeyword { .. })
    });
}

#[test]
fn detects_let_keyword_as_declaration_mistake() {
    assert_common_syntax_mistake("let x = 1\n", |reason| {
        reason == CommonSyntaxMistakeReason::LetOrVarKeyword
    });
}

#[test]
fn detects_match_keyword_as_pattern_matching_mistake() {
    assert_common_syntax_mistake("match x:\n", |reason| {
        reason == CommonSyntaxMistakeReason::MatchKeyword
    });
}

#[test]
fn detects_struct_keyword_as_struct_declaration_mistake() {
    assert_common_syntax_mistake("struct Name { }\n", |reason| {
        matches!(reason, CommonSyntaxMistakeReason::StructKeyword { .. })
    });
}
