//! Branching and match parsing regression tests.
//!
//! WHAT: validates `if`/`else` and `match`-style AST construction.
//! WHY: control-flow lowering relies on branch bodies and match arms staying structurally correct.

use super::*;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::test_support::{
    parse_single_file_ast, parse_single_file_ast_error, start_function_body,
};
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::datatypes::DataType;

#[test]
fn parses_if_else_statements() {
    let (ast, string_table) =
        parse_single_file_ast("flag = true\nif flag:\n    io(\"yes\")\nelse\n    io(\"no\")\n;\n");

    let body = start_function_body(&ast, &string_table);

    let NodeKind::If(condition, then_block, else_block) = &body[1].kind else {
        panic!("expected if statement in start body");
    };

    assert_eq!(condition.data_type, DataType::Bool);
    assert_eq!(then_block.len(), 1);
    assert_eq!(
        else_block.as_ref().map(Vec::len),
        Some(1),
        "else block should contain one host call"
    );
}

#[test]
fn parses_match_statements_with_else_arm() {
    let (ast, string_table) = parse_single_file_ast(
        "value = 42\nif value is:\n    case 0 => io(\"zero\");\n    case 42 => io(\"forty-two\");\n    else => io(\"other\");\n;\n",
    );

    let body = start_function_body(&ast, &string_table);

    let NodeKind::Match(subject, arms, else_block) = &body[1].kind else {
        panic!("expected match statement in start body");
    };

    assert_eq!(subject.data_type, DataType::Int);
    assert_eq!(arms.len(), 2);
    assert!(matches!(arms[0].condition.kind, ExpressionKind::Int(0)));
    assert!(matches!(arms[1].condition.kind, ExpressionKind::Int(42)));
    assert_eq!(
        else_block.as_ref().map(Vec::len),
        Some(1),
        "match should keep the default arm body"
    );
}

#[test]
fn parses_choice_match_arms_with_bare_and_qualified_variants() {
    let (ast, string_table) = parse_single_file_ast(
        "#Status :: Ready, Busy;\n\
         current Status = Status::Ready\n\
         if current is:\n\
             case Ready => io(\"ready\");\n\
             case Status::Busy => io(\"busy\");\n\
             else => io(\"other\");\n\
         ;\n",
    );

    let body = start_function_body(&ast, &string_table);
    let NodeKind::Match(subject, arms, else_block) = &body[1].kind else {
        panic!("expected match statement in start body");
    };

    assert!(
        matches!(subject.data_type, DataType::Choices(_)),
        "choice match subject should preserve choice type identity"
    );
    assert_eq!(arms.len(), 2);
    assert!(matches!(arms[0].condition.kind, ExpressionKind::Int(0)));
    assert!(matches!(arms[1].condition.kind, ExpressionKind::Int(1)));
    assert!(
        else_block.is_some(),
        "choice match should keep explicit else default"
    );
}

#[test]
fn rejects_legacy_colon_match_arm_syntax() {
    let error = parse_single_file_ast_error(
        "value = 1\nif value is:\n    1: io(\"one\");\n    else => io(\"other\");\n;\n",
    );

    assert_eq!(error.error_type, ErrorType::Syntax);
    assert!(
        error
            .msg
            .contains("Legacy match arm syntax is no longer supported"),
        "{}",
        error.msg
    );
}

#[test]
fn rejects_non_exhaustive_choice_match_without_else() {
    let error = parse_single_file_ast_error(
        "#Status :: Ready, Busy;\n\
         current Status = Status::Ready\n\
         if current is:\n\
             case Ready => io(\"ready\");\n\
         ;\n",
    );

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error.msg.contains("Non-exhaustive choice match"),
        "{}",
        error.msg
    );
    assert!(error.msg.contains("Busy"), "{}", error.msg);
}

#[test]
fn rejects_deferred_relational_match_patterns() {
    let error = parse_single_file_ast_error(
        "value = 1\nif value is:\n    case < 0 => io(\"neg\");\n    else => io(\"other\");\n;\n",
    );

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error.msg.contains("Relational match patterns"),
        "{}",
        error.msg
    );
}
