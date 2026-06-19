//! Unit tests for compile-time template folding.
//!
//! WHAT: exercises the borrow-first fold-binding resolver introduced in Phase A4
//!       so the common no-substitution path returns a borrowed reference instead
//!       of cloning the whole expression tree.
//! WHY: these tests are intentionally narrow: they assert the resolver's
//!      allocation behaviour, not end-to-end fold output. End-to-end parity is
//!      protected by the existing template integration suite.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::expression_kind::Operator;
use crate::compiler_frontend::ast::expressions::expression_rpn::{
    ExpressionRpn, ExpressionRpnItem,
};
use crate::compiler_frontend::ast::templates::template_control_flow::TemplateFoldBinding;
use crate::compiler_frontend::ast::templates::template_folding::{
    FoldResolvedExpression, TemplateFoldContext, resolve_fold_bindings_in_expression,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, SourceLocation};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::projects::settings::DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS;

fn test_location(line: i32) -> SourceLocation {
    SourceLocation {
        scope: InternedPath::new(),
        start_pos: CharPosition {
            line_number: line,
            char_column: 0,
        },
        end_pos: CharPosition {
            line_number: line,
            char_column: 120,
        },
    }
}

fn test_project_path_resolver() -> ProjectPathResolver {
    let cwd = std::env::temp_dir();
    ProjectPathResolver::new(
        cwd.clone(),
        cwd,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
    )
    .expect("test path resolver should be valid")
}

// -------------------------------------------------------
//  Borrow-first: no-substitution path returns Borrowed
// -------------------------------------------------------

#[test]
fn bool_condition_with_no_bindings_returns_borrowed() {
    let mut string_table = StringTable::new();
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings: vec![],
    };

    let condition = Expression::bool(true, test_location(1), ValueMode::ImmutableOwned);
    let resolved = resolve_fold_bindings_in_expression(&condition, &mut fold_context)
        .expect("resolution should succeed");

    assert!(
        matches!(resolved, FoldResolvedExpression::Borrowed(_)),
        "bool literal with no bindings should return Borrowed, not Owned"
    );
}

#[test]
fn string_slice_with_no_bindings_returns_borrowed() {
    let mut string_table = StringTable::new();
    let text_id = string_table.intern("hello");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings: vec![],
    };

    let text = Expression::string_slice(text_id, test_location(1), ValueMode::ImmutableOwned);
    let resolved = resolve_fold_bindings_in_expression(&text, &mut fold_context)
        .expect("resolution should succeed");

    assert!(
        matches!(resolved, FoldResolvedExpression::Borrowed(_)),
        "string slice with no bindings should return Borrowed"
    );
}

// -------------------------------------------------------
//  Borrow-first: binding substitution returns Owned
// -------------------------------------------------------

#[test]
fn bool_condition_binding_substitution_returns_owned() {
    let mut string_table = StringTable::new();
    let path = InternedPath::from_single_str("show", &mut string_table);

    let binding_value = Expression::bool(true, test_location(2), ValueMode::ImmutableOwned);
    let bindings = vec![TemplateFoldBinding {
        path: path.clone(),
        value: binding_value,
    }];

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let condition = Expression::reference(
        path,
        DataType::Bool,
        test_location(1),
        ValueMode::ImmutableOwned,
    );

    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings,
    };

    let resolved = resolve_fold_bindings_in_expression(&condition, &mut fold_context)
        .expect("resolution should succeed");

    assert!(
        matches!(resolved, FoldResolvedExpression::Owned(_)),
        "reference with a matching binding should return Owned"
    );

    let owned = resolved.into_owned();
    assert!(
        matches!(owned.kind, ExpressionKind::Bool(true)),
        "substituted expression should be the bound bool literal"
    );
}

// -------------------------------------------------------
//  Borrow-first: option-present capture substitution
// -------------------------------------------------------

#[test]
fn option_present_capture_substitution_returns_owned() {
    let mut string_table = StringTable::new();
    let path = InternedPath::from_single_str("maybe_name", &mut string_table);

    let inner_value = Expression::string_slice(
        string_table.intern("Alice"),
        test_location(2),
        ValueMode::ImmutableOwned,
    );
    let option_value = Expression::coerced(inner_value, builtin_type_ids::STRING);

    let bindings = vec![TemplateFoldBinding {
        path: path.clone(),
        value: option_value,
    }];

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let scrutinee = Expression::reference(
        path,
        DataType::StringSlice,
        test_location(1),
        ValueMode::ImmutableOwned,
    );

    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings,
    };

    let resolved = resolve_fold_bindings_in_expression(&scrutinee, &mut fold_context)
        .expect("resolution should succeed");

    assert!(
        matches!(resolved, FoldResolvedExpression::Owned(_)),
        "option reference with a matching binding should return Owned"
    );
}

// -------------------------------------------------------
//  Borrow-first: coerced expression stays Borrowed when inner unchanged
// -------------------------------------------------------

#[test]
fn coerced_expression_with_no_bindings_returns_borrowed() {
    let mut string_table = StringTable::new();
    let inner = Expression::string_slice(
        string_table.intern("value"),
        test_location(1),
        ValueMode::ImmutableOwned,
    );
    let coerced = Expression::coerced(inner, builtin_type_ids::STRING);

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings: vec![],
    };

    let resolved = resolve_fold_bindings_in_expression(&coerced, &mut fold_context)
        .expect("resolution should succeed");

    assert!(
        matches!(resolved, FoldResolvedExpression::Borrowed(_)),
        "coerced expression with no bindings should return Borrowed"
    );
}

// -------------------------------------------------------
//  Borrow-first: RPN substitution inside const template loops
// -------------------------------------------------------

#[test]
fn rpn_with_no_substitutable_operands_returns_borrowed() {
    let mut string_table = StringTable::new();
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings: vec![],
    };

    let rpn = ExpressionRpn {
        items: vec![
            ExpressionRpnItem::Operand(Expression::int(
                1,
                test_location(1),
                ValueMode::ImmutableOwned,
            )),
            ExpressionRpnItem::Operator {
                operator: Operator::Add,
                location: test_location(1),
            },
            ExpressionRpnItem::Operand(Expression::int(
                2,
                test_location(1),
                ValueMode::ImmutableOwned,
            )),
        ],
    };
    let runtime_expr = Expression::runtime(
        rpn,
        DataType::Int,
        test_location(1),
        ValueMode::ImmutableOwned,
    );

    let resolved = resolve_fold_bindings_in_expression(&runtime_expr, &mut fold_context)
        .expect("resolution should succeed");

    assert!(
        matches!(resolved, FoldResolvedExpression::Borrowed(_)),
        "RPN with only literal operands should return Borrowed"
    );
}

#[test]
fn rpn_with_bound_reference_operand_returns_owned() {
    let mut string_table = StringTable::new();
    let path = InternedPath::from_single_str("counter", &mut string_table);

    let binding_value = Expression::int(5, test_location(2), ValueMode::ImmutableOwned);
    let bindings = vec![TemplateFoldBinding {
        path: path.clone(),
        value: binding_value,
    }];

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let rpn = ExpressionRpn {
        items: vec![
            ExpressionRpnItem::Operand(Expression::reference(
                path,
                DataType::Int,
                test_location(1),
                ValueMode::ImmutableOwned,
            )),
            ExpressionRpnItem::Operator {
                operator: Operator::Add,
                location: test_location(1),
            },
            ExpressionRpnItem::Operand(Expression::int(
                1,
                test_location(1),
                ValueMode::ImmutableOwned,
            )),
        ],
    };
    let runtime_expr = Expression::runtime(
        rpn,
        DataType::Int,
        test_location(1),
        ValueMode::ImmutableOwned,
    );

    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings,
    };

    let resolved = resolve_fold_bindings_in_expression(&runtime_expr, &mut fold_context)
        .expect("resolution should succeed");

    assert!(
        matches!(resolved, FoldResolvedExpression::Owned(_)),
        "RPN with a bound reference operand should return Owned"
    );
}
