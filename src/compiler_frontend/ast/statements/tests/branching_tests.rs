//! Branching and match parsing regression tests.
//!
//! WHAT: validates `if`/`else` and `match`-style AST construction.
//! WHY: control-flow lowering relies on branch bodies and match arms staying structurally correct.

use super::*;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator,
};
use crate::compiler_frontend::compiler_errors::{ErrorMetaDataKey, ErrorType};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::tests::test_support::{
    parse_single_file_ast, parse_single_file_ast_error, start_function_body,
};

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

fn runtime_operator_sequence(expression: &Expression) -> Vec<Operator> {
    fn collect_operators_from_runtime_nodes(nodes: &[AstNode], out: &mut Vec<Operator>) {
        for node in nodes {
            match &node.kind {
                NodeKind::Operator(operator) => out.push(operator.to_owned()),
                NodeKind::Rvalue(Expression {
                    kind: ExpressionKind::Runtime(inner_nodes),
                    ..
                }) => collect_operators_from_runtime_nodes(inner_nodes, out),
                _ => {}
            }
        }
    }

    match &expression.kind {
        ExpressionKind::Runtime(nodes) => {
            let mut operators = Vec::new();
            collect_operators_from_runtime_nodes(nodes, &mut operators);
            operators
        }
        _ => vec![],
    }
}

#[test]
fn parses_nested_if_else_statements() {
    let (ast, string_table) = parse_single_file_ast(
        "outer = true\ninner = false\nif outer:\n    if inner:\n        io(\"inner\")\n    else\n        io(\"not inner\")\n    ;\nelse\n    io(\"outer false\")\n;\n",
    );

    let body = start_function_body(&ast, &string_table);
    let NodeKind::If(_, then_block, else_block) = &body[2].kind else {
        panic!("expected top-level if statement in start body");
    };
    let NodeKind::If(_, nested_then, nested_else) = &then_block[0].kind else {
        panic!("expected nested if statement in top-level then block");
    };

    assert_eq!(nested_then.len(), 1);
    assert_eq!(nested_else.as_ref().map(Vec::len), Some(1));
    assert_eq!(else_block.as_ref().map(Vec::len), Some(1));
}

#[test]
fn rejects_non_boolean_if_condition_with_type_error_metadata() {
    let error = parse_single_file_ast_error("if 1:\n    io(\"bad\")\n;\n");

    assert_eq!(error.error_type, ErrorType::Type);
    assert!(
        error
            .msg
            .contains("If statement condition requires a Bool condition"),
        "{}",
        error.msg
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::ExpectedType)
            .map(String::as_str),
        Some("Bool")
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::FoundType)
            .map(String::as_str),
        Some("Int")
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::PrimarySuggestion)
            .map(String::as_str),
        Some("Use a boolean expression in the if condition (for example 'value is 0' or 'flag')")
    );
}

#[test]
fn rejects_string_if_condition_with_type_error_metadata() {
    let error = parse_single_file_ast_error("if \"text\":\n    io(\"bad\")\n;\n");

    assert_eq!(error.error_type, ErrorType::Type);
    assert!(
        error
            .msg
            .contains("If statement condition requires a Bool condition"),
        "{}",
        error.msg
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::ExpectedType)
            .map(String::as_str),
        Some("Bool")
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::FoundType)
            .map(String::as_str),
        Some("String")
    );
}

#[test]
fn precedence_not_binds_tighter_than_and_in_if_conditions() {
    let (ast, string_table) =
        parse_single_file_ast("a = true\nb = false\nif not a and b:\n    io(\"x\")\n;\n");
    let body = start_function_body(&ast, &string_table);

    let NodeKind::If(condition, _, _) = &body[2].kind else {
        panic!("expected if statement in start body");
    };

    assert_eq!(
        runtime_operator_sequence(condition),
        vec![Operator::Not, Operator::And]
    );
}

#[test]
fn precedence_and_binds_tighter_than_or_in_if_conditions() {
    let (ast, string_table) = parse_single_file_ast(
        "a = true\nb = false\nc = false\nif a or b and c:\n    io(\"x\")\n;\n",
    );
    let body = start_function_body(&ast, &string_table);

    let NodeKind::If(condition, _, _) = &body[3].kind else {
        panic!("expected if statement in start body");
    };

    assert_eq!(
        runtime_operator_sequence(condition),
        vec![Operator::And, Operator::Or]
    );
}

#[test]
fn parenthesized_grouping_overrides_default_logical_precedence() {
    let (ast, string_table) = parse_single_file_ast(
        "a = true\nb = false\nc = false\nif (a or b) and c:\n    io(\"x\")\n;\n",
    );
    let body = start_function_body(&ast, &string_table);

    let NodeKind::If(condition, _, _) = &body[3].kind else {
        panic!("expected if statement in start body");
    };

    assert_eq!(
        runtime_operator_sequence(condition),
        vec![Operator::Or, Operator::And]
    );
}

#[test]
fn comparisons_bind_tighter_than_and_in_if_conditions() {
    let (ast, string_table) = parse_single_file_ast(
        "a = 1\nb = 2\nc = 3\nd = 4\nif a < b and c < d:\n    io(\"x\")\n;\n",
    );
    let body = start_function_body(&ast, &string_table);

    let NodeKind::If(condition, _, _) = &body[4].kind else {
        panic!("expected if statement in start body");
    };

    assert_eq!(
        runtime_operator_sequence(condition),
        vec![Operator::LessThan, Operator::LessThan, Operator::And]
    );
}

#[test]
fn parenthesized_comparison_can_be_negated_in_if_conditions() {
    let (ast, string_table) =
        parse_single_file_ast("a = 1\nb = 2\nif not (a < b):\n    io(\"x\")\n;\n");
    let body = start_function_body(&ast, &string_table);

    let NodeKind::If(condition, _, _) = &body[2].kind else {
        panic!("expected if statement in start body");
    };

    assert_eq!(
        runtime_operator_sequence(condition),
        vec![Operator::LessThan, Operator::Not]
    );
}

#[test]
fn equality_and_or_precedence_stays_deterministic_in_if_conditions() {
    let (ast, string_table) = parse_single_file_ast(
        "a = 1\nb = 1\nc = 2\nd = 2\nif a is b or c is d:\n    io(\"x\")\n;\n",
    );
    let body = start_function_body(&ast, &string_table);

    let NodeKind::If(condition, _, _) = &body[4].kind else {
        panic!("expected if statement in start body");
    };

    assert_eq!(
        runtime_operator_sequence(condition),
        vec![Operator::Equality, Operator::Equality, Operator::Or]
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
        matches!(subject.data_type, DataType::Choices { .. }),
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
fn rejects_choice_match_arm_qualifier_for_other_choice() {
    let error = parse_single_file_ast_error(
        "#Status :: Ready, Busy;\n\
         #OtherStatus :: Busy;\n\
         current Status = Status::Ready\n\
         if current is:\n\
             case OtherStatus::Busy => io(\"busy\");\n\
         ;\n",
    );

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error
            .msg
            .contains("does not match the scrutinee choice 'Status'"),
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
