//! Expression evaluation and runtime-RPN parsing regression tests.
//!
//! WHAT: validates operator precedence, runtime expression node construction, and template
//!       expression parsing.
//! WHY: expression parsing is dense and easy to break during refactors; targeted tests catch
//!      shape drift before it reaches HIR lowering.

use super::*;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext, TopLevelDeclarationTable};
use crate::compiler_frontend::compiler_messages::{
    DiagnosticKind, DiagnosticPayload, InvalidResultOperandReason, TypeDiagnosticKind,
    UnsupportedOperatorCategory,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::compile_time_paths::{
    CompileTimePath, CompileTimePathBase, CompileTimePathKind, CompileTimePaths,
};
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::parse_support::{
    parse_single_file_ast, parse_single_file_ast_diagnostic,
};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::type_coercion::compatibility::TypeCompatibilityCache;
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use std::rc::Rc;

fn first_start_declaration_expression(source: &str) -> Expression {
    nth_start_declaration_expression(source, 0)
}

fn nth_start_declaration_expression(source: &str, index: usize) -> Expression {
    let (ast, _string_table) = parse_single_file_ast(source);
    let start_function = ast
        .nodes
        .iter()
        .find(|node| matches!(node.kind, NodeKind::Function(_, _, _)))
        .expect("start function should exist");

    let NodeKind::Function(_, _, body) = &start_function.kind else {
        panic!("expected start function body");
    };
    let NodeKind::VariableDeclaration(declaration) = &body[index].kind else {
        panic!("expected start statement {index} to be a variable declaration");
    };

    declaration.value.to_owned()
}

fn assert_unsupported_operator(source: &str, expected_category: UnsupportedOperatorCategory) {
    let diagnostic = parse_single_file_ast_diagnostic(source);

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::UnsupportedOperatorTypes {
            category,
            ..
        } if category == expected_category
    ));
}

fn assert_invalid_result_operand(source: &str, expected_category: UnsupportedOperatorCategory) {
    let diagnostic = parse_single_file_ast_diagnostic(source);

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidResultOperand {
            reason: InvalidResultOperandReason::ResultNotUnwrapped,
            category,
            ..
        } if category == expected_category
    ));
}

#[test]
fn ordinary_expression_rejects_path_string_concatenation() {
    let mut string_table = StringTable::new();
    let source_scope = InternedPath::from_single_str("#page.bst", &mut string_table);
    let asset_path = InternedPath::from_single_str("assets", &mut string_table)
        .join_str("logo.png", &mut string_table);
    let compile_time_paths = CompileTimePaths {
        paths: vec![CompileTimePath {
            source_path: asset_path.clone(),
            filesystem_path: std::env::temp_dir().join("beanstalk_eval_expression_logo.png"),
            public_path: asset_path.clone(),
            base: CompileTimePathBase::EntryRoot,
            kind: CompileTimePathKind::File,
        }],
    };
    let context = ScopeContext::new(
        ContextKind::Template,
        source_scope.clone(),
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        ExternalPackageRegistry::new(),
        vec![],
    )
    .with_source_file_scope(source_scope.clone())
    .with_path_format_config(PathStringFormatConfig {
        origin: String::from("/beanstalk"),
        ..PathStringFormatConfig::default()
    });

    let nodes = vec![
        AstNode {
            kind: NodeKind::Rvalue(Expression::path(
                compile_time_paths,
                SourceLocation::default(),
            )),
            location: SourceLocation::default(),
            scope: source_scope.clone(),
        },
        AstNode {
            kind: NodeKind::Operator(Operator::Add),
            location: SourceLocation::default(),
            scope: source_scope.clone(),
        },
        AstNode {
            kind: NodeKind::Rvalue(Expression::string_slice(
                string_table.get_or_intern(String::from("?v=1")),
                SourceLocation::default(),
                ValueMode::ImmutableOwned,
            )),
            location: SourceLocation::default(),
            scope: source_scope,
        },
    ];

    let mut current_type = ExpectedType::Infer;
    let mut type_environment = TypeEnvironment::new();
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);
    let error = evaluate_expression(
        &context,
        nodes,
        &mut type_interner,
        &mut current_type,
        &ValueMode::ImmutableOwned,
        &mut string_table,
    )
    .expect_err("ordinary expressions should stay strict");

    let crate::compiler_frontend::ast::expressions::eval_expression::ExpressionTypingError::Diagnostic(diagnostic) = error else {
        panic!("expected an expression type diagnostic");
    };
    assert_eq!(
        diagnostic.kind,
        DiagnosticKind::Type(TypeDiagnosticKind::UnsupportedOperatorTypes)
    );
    assert_eq!(diagnostic.kind.code(), "BST-TYPE-0003");
    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::UnsupportedOperatorTypes {
            category: UnsupportedOperatorCategory::Arithmetic,
            ..
        }
    ));

    let recorded = context.take_rendered_path_usages();
    assert!(recorded.is_empty());
}

#[test]
fn unary_not_requires_boolean_operand() {
    assert_unsupported_operator("value = not 1\n", UnsupportedOperatorCategory::Unary);
}

#[test]
fn logical_and_requires_bool_operands() {
    assert_unsupported_operator("value = true and 1\n", UnsupportedOperatorCategory::Logical);
}

#[test]
fn logical_and_reports_found_types_in_operand_order() {
    assert_unsupported_operator("value = 1 and true\n", UnsupportedOperatorCategory::Logical);
}

#[test]
fn logical_or_rejects_string_operands() {
    assert_unsupported_operator(
        "value = \"a\" or \"b\"\n",
        UnsupportedOperatorCategory::Logical,
    );
}

#[test]
fn logical_mix_rejects_non_bool_rhs_after_comparison() {
    assert_unsupported_operator(
        "value = 1 < 2 and 3\n",
        UnsupportedOperatorCategory::Logical,
    );
}

#[test]
fn ordinary_operators_reject_result_operands_without_handler() {
    assert_invalid_result_operand(
        "value = Int(\"1\") is 1\n",
        UnsupportedOperatorCategory::Comparison,
    );
}

#[test]
fn arithmetic_operator_rejects_result_operands_without_handler() {
    assert_invalid_result_operand(
        "value = Int(\"1\") + 1\n",
        UnsupportedOperatorCategory::Arithmetic,
    );
}

#[test]
fn logical_operator_rejects_result_operands_before_bool_validation() {
    assert_invalid_result_operand(
        "value = true and Int(\"1\")\n",
        UnsupportedOperatorCategory::Logical,
    );
}

#[test]
fn logical_operator_rejects_option_operands_with_precise_found_type() {
    assert_unsupported_operator(
        "maybe String? = none\nvalue = maybe or true\n",
        UnsupportedOperatorCategory::Logical,
    );
}

#[test]
fn comparison_operator_accepts_option_to_scalar_comparison() {
    let value =
        nth_start_declaration_expression("maybe String? = \"x\"\nvalue = maybe is \"x\"\n", 1);

    assert_eq!(value.diagnostic_type, DataType::Bool);
}

#[test]
fn comparison_operator_rejects_none_without_option_context() {
    assert_unsupported_operator(
        "value = none is none\n",
        UnsupportedOperatorCategory::Comparison,
    );
}

#[test]
fn mixed_int_float_arithmetic_resolves_to_float() {
    let value = first_start_declaration_expression("value = 1 + 2.5\n");

    assert_eq!(value.diagnostic_type, DataType::Float);
}

#[test]
fn int_division_resolves_to_float() {
    let value = first_start_declaration_expression("value = 5 / 2\n");

    assert_eq!(value.diagnostic_type, DataType::Float);
}

#[test]
fn integer_division_resolves_to_int() {
    let value = first_start_declaration_expression("value = 5 // 2\n");

    assert_eq!(value.diagnostic_type, DataType::Int);
}

#[test]
fn integer_division_rejects_int_float_operands() {
    assert_unsupported_operator(
        "value = 5 // 2.0\n",
        UnsupportedOperatorCategory::Arithmetic,
    );
}

#[test]
fn integer_division_rejects_float_int_operands() {
    assert_unsupported_operator(
        "value = 5.0 // 2\n",
        UnsupportedOperatorCategory::Arithmetic,
    );
}

#[test]
fn multiline_expression_with_operator_on_next_line_resolves_correctly() {
    let value = first_start_declaration_expression("value = 1\n + 2\n + 3\n");

    assert_eq!(value.diagnostic_type, DataType::Int);
    assert!(
        matches!(value.kind, ExpressionKind::Int(6)),
        "expected folded Int(6), got {:?}",
        value.kind
    );
}

#[test]
fn multiline_expression_with_operator_at_end_of_line_resolves_correctly() {
    let value = first_start_declaration_expression("value = 1 +\n 2 +\n 3\n");

    assert_eq!(value.diagnostic_type, DataType::Int);
    assert!(
        matches!(value.kind, ExpressionKind::Int(6)),
        "expected folded Int(6), got {:?}",
        value.kind
    );
}

#[test]
fn multiline_comparison_expression_resolves_to_bool() {
    let value = first_start_declaration_expression("value = 1\n is\n 1\n");

    assert_eq!(value.diagnostic_type, DataType::Bool);
    assert!(
        matches!(value.kind, ExpressionKind::Bool(true)),
        "expected folded Bool(true), got {:?}",
        value.kind
    );
}

#[test]
fn mixed_int_float_comparison_resolves_to_bool() {
    let value = first_start_declaration_expression("value = 1 <= 2.5\n");

    assert_eq!(value.diagnostic_type, DataType::Bool);
}

#[test]
fn bool_relational_comparison_is_rejected() {
    assert_unsupported_operator(
        "value = true < false\n",
        UnsupportedOperatorCategory::Comparison,
    );
}

#[test]
fn string_equality_comparison_resolves_to_bool() {
    let value = first_start_declaration_expression("value = \"a\" is \"b\"\n");

    assert_eq!(value.diagnostic_type, DataType::Bool);
}

#[test]
fn char_relational_comparison_resolves_to_bool() {
    let value = first_start_declaration_expression("value = 'a' < 'b'\n");

    assert_eq!(value.diagnostic_type, DataType::Bool);
}

#[test]
fn fully_constant_boolean_and_comparison_expressions_fold() {
    let (ast, _string_table) = parse_single_file_ast("flag = not (1 < 2) or (3 < 4 and false)\n");
    let start_function = ast
        .nodes
        .iter()
        .find(|node| matches!(node.kind, NodeKind::Function(_, _, _)))
        .expect("start function should exist");

    let NodeKind::Function(_, _, body) = &start_function.kind else {
        panic!("expected start function body");
    };
    let NodeKind::VariableDeclaration(declaration) = &body[0].kind else {
        panic!("expected folded declaration");
    };

    assert!(
        matches!(declaration.value.kind, ExpressionKind::Bool(false)),
        "expected fully-folded boolean/comparison expression to collapse to Bool(false), got {:?}",
        declaration.value.kind
    );
    assert_eq!(declaration.value.diagnostic_type, DataType::Bool);
}
