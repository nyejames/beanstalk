//! Branching and match parsing regression tests.
//!
//! WHAT: validates `if`/`else` and `match`-style AST construction.
//! WHY: control-flow lowering relies on branch bodies and match arms staying structurally correct.

use super::*;
use crate::compiler_frontend::ast::ast_nodes::MatchExhaustiveness;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator,
};
use crate::compiler_frontend::ast::statements::match_patterns::{
    MatchPattern, RelationalPatternOp,
};
use crate::compiler_frontend::compiler_messages::{
    DiagnosticKind, DiagnosticPayload, InvalidControlFlowStatementReason, InvalidMatchArmReason,
    InvalidMatchPatternReason, NonExhaustiveMatchReason, RuleDiagnosticKind, SyntaxDiagnosticKind,
    TypeMismatchContext,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::tests::ast_fixture_support::start_function_body;
use crate::compiler_frontend::tests::parse_support::{
    parse_single_file_ast, parse_single_file_ast_diagnostic,
};

#[test]
fn parses_if_else_statements() {
    let (ast, string_table) =
        parse_single_file_ast("flag = true\nif flag:\n    io(\"yes\")\nelse\n    io(\"no\")\n;\n");

    let body = start_function_body(&ast, &string_table);

    let NodeKind::If(condition, then_block, else_block) = &body[1].kind else {
        panic!("expected if statement in start body");
    };

    assert_eq!(condition.diagnostic_type, DataType::Bool);
    assert_eq!(then_block.len(), 1);
    assert_eq!(
        else_block.as_ref().map(Vec::len),
        Some(1),
        "else block should contain one host call"
    );
}

#[test]
fn rejects_same_line_else_if_statement() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "if true:\n    io(\"selected\")\nelse if false:\n    io(\"unsupported\")\n;\n",
    );

    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::InvalidControlFlowStatement {
                reason: InvalidControlFlowStatementReason::ElseIfUnsupported,
            }
        ),
        "{:?}",
        diagnostic.payload
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

// --------------------------
//  If-condition type checks
// --------------------------

#[test]
fn rejects_non_boolean_if_condition_with_type_error_metadata() {
    let diagnostic = parse_single_file_ast_diagnostic("if 1:\n    io(\"bad\")\n;\n");

    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::TypeMismatch {
                context: TypeMismatchContext::Condition,
                ..
            }
        ),
        "{:?}",
        diagnostic.payload
    );
}

#[test]
fn rejects_string_if_condition_with_type_error_metadata() {
    let diagnostic = parse_single_file_ast_diagnostic("if \"text\":\n    io(\"bad\")\n;\n");

    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::TypeMismatch {
                context: TypeMismatchContext::Condition,
                ..
            }
        ),
        "{:?}",
        diagnostic.payload
    );
}

// --------------------------
//  Operator precedence in conditions
// --------------------------

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

// --------------------------
//  Match statements
// --------------------------

#[test]
fn parses_match_statements_with_else_arm() {
    let (ast, string_table) = parse_single_file_ast(
        "value = 42\nif value is:\n    0 => io(\"zero\")\n    42 => io(\"forty-two\")\n    else => io(\"other\")\n;\n",
    );

    let body = start_function_body(&ast, &string_table);

    let NodeKind::Match {
        scrutinee,
        arms,
        default: else_block,
        exhaustiveness,
    } = &body[1].kind
    else {
        panic!("expected match statement in start body");
    };

    assert_eq!(scrutinee.diagnostic_type, DataType::Int);
    assert_eq!(arms.len(), 2);
    assert!(matches!(
        arms[0].pattern,
        MatchPattern::Literal(Expression {
            kind: ExpressionKind::Int(0),
            ..
        })
    ));
    assert!(matches!(
        arms[1].pattern,
        MatchPattern::Literal(Expression {
            kind: ExpressionKind::Int(42),
            ..
        })
    ));
    assert_eq!(
        else_block.as_ref().map(Vec::len),
        Some(1),
        "match should keep the default arm body"
    );
    assert_eq!(*exhaustiveness, MatchExhaustiveness::HasDefault);
}

#[test]
fn parses_match_arm_with_boolean_guard() {
    let (ast, string_table) = parse_single_file_ast(
        "value = 42\nif value is:\n    42 if true => io(\"forty-two\")\n    else => io(\"other\")\n;\n",
    );

    let body = start_function_body(&ast, &string_table);

    let NodeKind::Match {
        arms,
        default: else_block,
        exhaustiveness,
        ..
    } = &body[1].kind
    else {
        panic!("expected match statement in start body");
    };

    assert_eq!(arms.len(), 1);
    assert!(
        matches!(
            arms[0].guard.as_ref().map(|guard| &guard.kind),
            Some(&ExpressionKind::Bool(true))
        ),
        "guard should parse as a Bool expression attached to the arm"
    );
    assert!(
        else_block.is_some(),
        "guarded literal match should still preserve explicit else body"
    );
    assert_eq!(*exhaustiveness, MatchExhaustiveness::HasDefault);
}

#[test]
fn rejects_non_boolean_match_guard_with_type_error_metadata() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "value = 1\nif value is:\n    1 if 7 => io(\"one\")\n    else => io(\"other\")\n;\n",
    );

    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::TypeMismatch {
                context: TypeMismatchContext::Condition,
                ..
            }
        ),
        "{:?}",
        diagnostic.payload
    );
}

#[test]
fn parses_choice_match_arms_with_bare_and_qualified_variants() {
    let (ast, string_table) = parse_single_file_ast(
        "Status :: Ready, Busy;\n\
         current Status = Status::Ready\n\
         if current is:\n\
             Ready => io(\"ready\")\n\
             Status::Busy => io(\"busy\")\n\
             else => io(\"other\")\n\
         ;\n",
    );

    let body = start_function_body(&ast, &string_table);
    let NodeKind::Match {
        scrutinee,
        arms,
        default: else_block,
        exhaustiveness,
    } = &body[1].kind
    else {
        panic!("expected match statement in start body");
    };

    assert!(
        matches!(scrutinee.diagnostic_type, DataType::Choices { .. }),
        "choice match scrutinee should preserve choice type identity"
    );
    assert_eq!(arms.len(), 2);
    assert!(
        matches!(arms[0].pattern, MatchPattern::ChoiceVariant { tag: 0, .. }),
        "expected first arm to match Ready (tag 0)"
    );
    assert!(
        matches!(arms[1].pattern, MatchPattern::ChoiceVariant { tag: 1, .. }),
        "expected second arm to match Busy (tag 1)"
    );
    assert!(
        else_block.is_some(),
        "choice match should keep explicit else default"
    );
    assert_eq!(*exhaustiveness, MatchExhaustiveness::HasDefault);
}

#[test]
fn parses_exhaustive_choice_match_without_else_marks_exhaustive_choice() {
    let (ast, string_table) = parse_single_file_ast(
        "Status :: Ready, Busy;\n\
         current Status = Status::Ready\n\
         if current is:\n\
             Ready => io(\"ready\")\n\
             Busy => io(\"busy\")\n\
         ;\n",
    );

    let body = start_function_body(&ast, &string_table);
    let NodeKind::Match {
        scrutinee: _,
        arms,
        default,
        exhaustiveness,
    } = &body[1].kind
    else {
        panic!("expected match statement in start body");
    };

    assert_eq!(arms.len(), 2);
    assert!(default.is_none());
    assert_eq!(*exhaustiveness, MatchExhaustiveness::ExhaustiveChoice);
}

#[test]
fn rejects_legacy_colon_match_arm_syntax() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "value = 1\nif value is:\n    1: io(\"one\")\n    else => io(\"other\")\n;\n",
    );

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Syntax(SyntaxDiagnosticKind::InvalidMatchArm)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidMatchArm {
            reason: InvalidMatchArmReason::LegacyColonSyntax
        }
    );
}

#[test]
fn rejects_choice_match_arm_qualifier_for_other_choice() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "Status :: Ready, Busy;\n\
         OtherStatus :: Busy;\n\
         current Status = Status::Ready\n\
         if current is:\n\
             OtherStatus::Busy => io(\"busy\")\n\
         ;\n",
    );

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Rule(RuleDiagnosticKind::InvalidMatchPattern)
    );
    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::InvalidMatchPattern {
                reason: InvalidMatchPatternReason::QualifierDoesNotMatchScrutinee,
                variant_name: None,
                scrutinee_name: Some(_),
            }
        ),
        "{:?}",
        diagnostic.payload
    );
}

#[test]
fn rejects_non_exhaustive_choice_match_without_else() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "Status :: Ready, Busy;\n\
         current Status = Status::Ready\n\
         if current is:\n\
             Ready => io(\"ready\")\n\
         ;\n",
    );

    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::NonExhaustiveMatch {
                reason: NonExhaustiveMatchReason::MissingVariants,
                ref missing_variants,
                ..
            } if missing_variants.len() == 1
        ),
        "{:?}",
        diagnostic.payload
    );
}

#[test]
fn rejects_guarded_choice_match_without_else() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "Status :: Ready, Busy;\n\
         current Status = Status::Ready\n\
         if current is:\n\
             Ready if true => io(\"ready\")\n\
             Busy => io(\"busy\")\n\
         ;\n",
    );

    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::NonExhaustiveMatch {
            reason: NonExhaustiveMatchReason::GuardedArmsRequireElse,
            missing_variants: Vec::new(),
        }
    );
}

// --------------------------
//  Option present patterns
// --------------------------

#[test]
fn parses_option_present_capture_statement_condition() {
    let (ast, string_table) =
        parse_single_file_ast("value Int? = 42\nif value is |amount|:\n    io(amount)\n;\n");

    let body = start_function_body(&ast, &string_table);
    let NodeKind::Match {
        scrutinee,
        arms,
        default,
        exhaustiveness,
    } = &body[1].kind
    else {
        panic!("expected option present capture to parse as a single-arm match statement");
    };

    assert_eq!(
        ast.type_environment.option_inner_type(scrutinee.type_id),
        Some(ast.type_environment.builtins().int),
        "scrutinee should keep semantic Int? identity even when its diagnostic spelling is inferred"
    );
    assert_eq!(arms.len(), 1);
    assert!(
        matches!(arms[0].pattern, MatchPattern::OptionPresentCapture { .. }),
        "single-predicate statement form should use option present capture"
    );
    assert_eq!(arms[0].body.len(), 1);
    assert_eq!(*exhaustiveness, MatchExhaustiveness::HasDefault);
    assert!(
        default.as_ref().is_some_and(Vec::is_empty),
        "statement-only present capture should synthesize an empty default so `none` falls through"
    );
}

#[test]
fn parses_option_match_present_capture_guard_and_none_patterns() {
    let (ast, string_table) = parse_single_file_ast(
        "value Int? = 42\n\
         if value is:\n\
             |positive| if positive > 0 => io(positive)\n\
             |fallback| => io(fallback)\n\
             none => io(\"missing\")\n\
         ;\n",
    );

    let body = start_function_body(&ast, &string_table);
    let NodeKind::Match { arms, default, .. } = &body[1].kind else {
        panic!("expected option full match statement");
    };

    assert_eq!(arms.len(), 3);
    assert!(
        matches!(arms[0].pattern, MatchPattern::OptionPresentCapture { .. }),
        "first arm should capture any present option value"
    );
    assert!(arms[0].guard.is_some(), "first arm should keep its guard");
    assert!(
        matches!(arms[1].pattern, MatchPattern::OptionPresentCapture { .. }),
        "second arm should be the unguarded present-value fallback"
    );
    assert!(
        matches!(arms[2].pattern, MatchPattern::OptionNone { .. }),
        "third arm should cover absence"
    );
    assert!(default.is_none(), "the source did not include an else arm");
}

// --------------------------
//  Relational match patterns
// --------------------------

#[test]
fn parses_relational_match_patterns() {
    let (ast, string_table) = parse_single_file_ast(
        "value = 1\nif value is:\n    < 0 => io(\"neg\")\n    else => io(\"other\")\n;\n",
    );

    let body = start_function_body(&ast, &string_table);
    let NodeKind::Match { arms, .. } = &body[1].kind else {
        panic!("expected match statement in start body");
    };

    assert_eq!(arms.len(), 1);
    assert!(
        matches!(arms[0].pattern, MatchPattern::Relational { .. }),
        "relational pattern should parse successfully"
    );
}

#[test]
fn rejects_semicolon_between_match_arms() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "value = 1\nif value is:\n    1 => io(\"one\");\n    2 => io(\"two\")\n    else => io(\"other\")\n;\n",
    );

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Syntax(SyntaxDiagnosticKind::InvalidMatchArm)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidMatchArm {
            reason: InvalidMatchArmReason::SemicolonDelimiter
        }
    );
}

#[test]
fn allows_semicolons_inside_nested_structures_within_match_arms() {
    let (ast, string_table) = parse_single_file_ast(
        "value = 1\n\
         if value is:\n\
             1 =>\n\
                 if true:\n\
                     io(\"nested\")\n\
                 ;\n\
             else => io(\"other\")\n\
         ;\n",
    );

    let body = start_function_body(&ast, &string_table);

    let NodeKind::Match { arms, .. } = &body[1].kind else {
        panic!("expected match statement in start body");
    };

    assert_eq!(arms.len(), 1);
    assert!(
        matches!(arms[0].body[0].kind, NodeKind::If(_, _, _)),
        "nested if body inside a match arm should parse successfully"
    );
}

#[test]
fn parses_relational_int_patterns() {
    let (ast, string_table) = parse_single_file_ast(
        "value = 5\nif value is:\n    < 0 => io(\"negative\")\n    >= 0 => io(\"non-negative\")\n    else => io(\"fallback\")\n;\n",
    );

    let body = start_function_body(&ast, &string_table);
    let NodeKind::Match { arms, .. } = &body[1].kind else {
        panic!("expected match statement in start body");
    };

    assert_eq!(arms.len(), 2);
    assert!(
        matches!(
            arms[0].pattern,
            MatchPattern::Relational {
                op: RelationalPatternOp::LessThan,
                ..
            }
        ),
        "first arm should be < pattern"
    );
    assert!(
        matches!(
            arms[1].pattern,
            MatchPattern::Relational {
                op: RelationalPatternOp::GreaterThanOrEqual,
                ..
            }
        ),
        "second arm should be >= pattern"
    );
}

#[test]
fn parses_relational_arm_after_single_line_assignment_body() {
    let (ast, string_table) = parse_single_file_ast(
        "value = 5\n\
         result ~= \"\"\n\
         if value is:\n\
             < 0 => result = \"negative\"\n\
             0 => result = \"zero\"\n\
             <= 10 => result = \"small\"\n\
             else => result = \"fallback\"\n\
         ;\n",
    );

    let body = start_function_body(&ast, &string_table);
    let NodeKind::Match { arms, .. } = &body[2].kind else {
        panic!("expected match statement in start body");
    };

    assert_eq!(arms.len(), 3);
    assert!(
        matches!(
            arms[2].pattern,
            MatchPattern::Relational {
                op: RelationalPatternOp::LessThanOrEqual,
                ..
            }
        ),
        "assignment bodies must not consume the next relational arm header"
    );
}

#[test]
fn relational_patterns_without_default_are_not_exhaustive() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "value = 5\nif value is:\n    < 0 => io(\"negative\")\n    >= 0 => io(\"non-negative\")\n;\n",
    );

    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::NonExhaustiveMatch {
            reason: NonExhaustiveMatchReason::MissingElseArm,
            missing_variants: Vec::new(),
        }
    );
}

#[test]
fn relational_pattern_rejects_bool() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "value = true\nif value is:\n    < true => io(\"bad\")\n    else => io(\"fallback\")\n;\n",
    );

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Rule(RuleDiagnosticKind::InvalidMatchPattern)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidMatchPattern {
            reason: InvalidMatchPatternReason::ScrutineeTypeUnsupportedForRelational,
            variant_name: None,
            scrutinee_name: None,
        }
    );
}

#[test]
fn relational_pattern_accepts_string() {
    let (ast, string_table) = parse_single_file_ast(
        "value = \"abc\"\nif value is:\n    < \"def\" => io(\"before\")\n    else => io(\"fallback\")\n;\n",
    );

    let body = start_function_body(&ast, &string_table);
    let NodeKind::Match { arms, .. } = &body[1].kind else {
        panic!("expected match statement in start body");
    };

    assert_eq!(arms.len(), 1);
    assert!(
        matches!(
            arms[0].pattern,
            MatchPattern::Relational {
                op: RelationalPatternOp::LessThan,
                ..
            }
        ),
        "string relational pattern should parse successfully"
    );
}

#[test]
fn parses_multi_statement_match_arm_body_delimited_by_next_arm() {
    let (ast, string_table) = parse_single_file_ast(
        "value = 1\n\
         result ~= \"unset\"\n\
         if value is:\n\
             1 =>\n\
                 result = \"one\"\n\
                 io(result)\n\
             2 =>\n\
                 result = \"two\"\n\
             else => result = \"other\"\n\
         ;\n",
    );

    let body = start_function_body(&ast, &string_table);
    let NodeKind::Match {
        arms,
        default: else_block,
        ..
    } = &body[2].kind
    else {
        panic!("expected match statement in start body");
    };

    assert_eq!(arms.len(), 2, "should have two pattern arms");
    assert_eq!(
        arms[0].body.len(),
        2,
        "first arm should have two statements"
    );
    assert_eq!(
        arms[1].body.len(),
        1,
        "second arm should have one statement"
    );
    assert!(else_block.is_some(), "should have an else default arm");
}

#[test]
fn rejects_same_line_second_match_arm() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "value = 1\nif value is:\n    1 => io(\"one\") 2 => io(\"two\")\n    else => io(\"other\")\n;\n",
    );

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Syntax(SyntaxDiagnosticKind::InvalidMatchArm)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidMatchArm {
            reason: InvalidMatchArmReason::ArmMustStartNewLine
        }
    );
}

#[test]
fn rejects_missing_match_arm_header() {
    let diagnostic = parse_single_file_ast_diagnostic(
        "value = 1\nif value is:\n    => io(\"bad\")\n    else => io(\"other\")\n;\n",
    );

    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Syntax(SyntaxDiagnosticKind::InvalidMatchArm)
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::InvalidMatchArm {
            reason: InvalidMatchArmReason::ExpectedArmHeader
        }
    );
}

#[test]
fn case_is_valid_as_normal_identifier() {
    let (ast, string_table) = parse_single_file_ast("case = 42\nio(case)\n");

    let body = start_function_body(&ast, &string_table);
    assert_eq!(
        body.len(),
        2,
        "should parse declaration and call without treating `case` as a keyword"
    );

    assert!(
        matches!(&body[0].kind, NodeKind::VariableDeclaration { .. }),
        "should parse `case = 42` as a normal variable declaration"
    );
}
