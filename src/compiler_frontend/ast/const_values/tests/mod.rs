//! Tests for the shared AST const value resolver.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::const_values::facts::{
    ConstBindingScope, ConstBindingSource, ConstFactValueKind,
};
use crate::compiler_frontend::ast::const_values::resolver::{
    ConstResolutionError, ConstValueEnvironment, ConstValueResolver,
};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator,
};
use crate::compiler_frontend::ast::expressions::expression_rpn::{
    ExpressionRpn, ExpressionRpnItem,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn make_resolver(string_table: &mut StringTable) -> ConstValueResolver<'_> {
    ConstValueResolver::new(string_table)
}

fn make_environment_with(
    path: &str,
    expression: Expression,
    string_table: &mut StringTable,
) -> ConstValueEnvironment {
    let mut env = ConstValueEnvironment::default();
    let interned_path = InternedPath::from_single_str(path, string_table);
    env.insert(interned_path, expression);
    env
}

fn rvalue_item(expression: Expression) -> ExpressionRpnItem {
    ExpressionRpnItem::Operand(expression)
}

fn operator_item(operator: Operator) -> ExpressionRpnItem {
    ExpressionRpnItem::Operator {
        operator,
        location: SourceLocation::default(),
    }
}

// ------------------------------
//  Literal expression resolves
// ------------------------------

#[test]
fn literal_int_resolves_as_const() {
    let mut string_table = StringTable::new();
    let expression = Expression::int(42, empty_location(), ValueMode::ImmutableOwned);
    let env = ConstValueEnvironment::default();
    let mut resolver = make_resolver(&mut string_table);

    let result = resolver
        .resolve_expression(&expression, &env)
        .expect("literal should resolve");

    assert!(matches!(result.kind, ExpressionKind::Int(42)));
}

#[test]
fn literal_string_resolves_as_const() {
    let mut string_table = StringTable::new();
    let string_id = string_table.intern("hello");
    let expression =
        Expression::string_slice(string_id, empty_location(), ValueMode::ImmutableOwned);
    let env = ConstValueEnvironment::default();
    let mut resolver = make_resolver(&mut string_table);

    let result = resolver
        .resolve_expression(&expression, &env)
        .expect("literal should resolve");

    assert!(matches!(result.kind, ExpressionKind::StringSlice(_)));
}

// ------------------------------
//  Folded arithmetic resolves
// ------------------------------

#[test]
fn folded_arithmetic_resolves_to_literal() {
    let mut string_table = StringTable::new();
    let rpn = ExpressionRpn {
        items: vec![
            rvalue_item(Expression::int(
                1,
                empty_location(),
                ValueMode::ImmutableOwned,
            )),
            rvalue_item(Expression::int(
                2,
                empty_location(),
                ValueMode::ImmutableOwned,
            )),
            operator_item(Operator::Add),
        ],
    };
    let expression = Expression::runtime_with_type_id(
        rpn,
        DataType::Int,
        builtin_type_ids::INT,
        empty_location(),
        ValueMode::ImmutableOwned,
    );

    let env = ConstValueEnvironment::default();
    let mut resolver = make_resolver(&mut string_table);

    let result = resolver
        .resolve_expression(&expression, &env)
        .expect("folded arithmetic should resolve");

    assert!(matches!(result.kind, ExpressionKind::Int(3)));
}

#[test]
fn folded_arithmetic_with_reference_substitution_resolves() {
    let mut string_table = StringTable::new();
    let rpn = ExpressionRpn {
        items: vec![
            rvalue_item(Expression::reference(
                InternedPath::from_single_str("x", &mut string_table),
                DataType::Int,
                empty_location(),
                ValueMode::ImmutableReference,
            )),
            rvalue_item(Expression::int(
                5,
                empty_location(),
                ValueMode::ImmutableOwned,
            )),
            operator_item(Operator::Multiply),
        ],
    };
    let expression = Expression::runtime_with_type_id(
        rpn,
        DataType::Int,
        builtin_type_ids::INT,
        empty_location(),
        ValueMode::ImmutableOwned,
    );

    let env = make_environment_with(
        "x",
        Expression::int(3, empty_location(), ValueMode::ImmutableOwned),
        &mut string_table,
    );
    let mut resolver = make_resolver(&mut string_table);

    let result = resolver
        .resolve_expression(&expression, &env)
        .expect("substituted arithmetic should resolve");

    assert!(matches!(result.kind, ExpressionKind::Int(15)));
}

#[test]
fn folded_arithmetic_with_coerced_reference_substitution_resolves() {
    let mut string_table = StringTable::new();
    let reference = Expression::reference(
        InternedPath::from_single_str("x", &mut string_table),
        DataType::Int,
        empty_location(),
        ValueMode::ImmutableReference,
    );
    let rpn = ExpressionRpn {
        items: vec![
            rvalue_item(Expression::coerced(reference, builtin_type_ids::INT)),
            rvalue_item(Expression::int(
                2,
                empty_location(),
                ValueMode::ImmutableOwned,
            )),
            operator_item(Operator::Add),
        ],
    };
    let expression = Expression::runtime_with_type_id(
        rpn,
        DataType::Int,
        builtin_type_ids::INT,
        empty_location(),
        ValueMode::ImmutableOwned,
    );

    let env = make_environment_with(
        "x",
        Expression::int(40, empty_location(), ValueMode::ImmutableOwned),
        &mut string_table,
    );
    let mut resolver = make_resolver(&mut string_table);

    let result = resolver
        .resolve_expression(&expression, &env)
        .expect("coerced reference arithmetic should resolve");

    assert!(matches!(result.kind, ExpressionKind::Int(42)));
}

// ------------------------------
//  Reference to known const resolves
// ------------------------------

#[test]
fn reference_to_known_const_resolves() {
    let mut string_table = StringTable::new();
    let path = InternedPath::from_single_str("ratio", &mut string_table);
    let expression = Expression::reference_with_type_id(
        path.clone(),
        DataType::Float,
        builtin_type_ids::FLOAT,
        empty_location(),
        ValueMode::ImmutableReference,
        crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState::RuntimeValue,
    );

    let env = make_environment_with(
        "ratio",
        Expression::float(2.71, empty_location(), ValueMode::ImmutableOwned),
        &mut string_table,
    );
    let mut resolver = make_resolver(&mut string_table);

    let result = resolver
        .resolve_expression(&expression, &env)
        .expect("reference should resolve");

    assert!(
        matches!(result.kind, ExpressionKind::Float(value) if (value - 2.71).abs() < f64::EPSILON)
    );
}

// ------------------------------
//  Forward reference fails
// ------------------------------

#[test]
fn unresolved_reference_fails() {
    let mut string_table = StringTable::new();
    let path = InternedPath::from_single_str("unknown", &mut string_table);
    let expression = Expression::reference_with_type_id(
        path,
        DataType::Int,
        builtin_type_ids::INT,
        empty_location(),
        ValueMode::ImmutableReference,
        crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState::RuntimeValue,
    );

    let env = ConstValueEnvironment::default();
    let mut resolver = make_resolver(&mut string_table);

    let error = resolver
        .resolve_expression(&expression, &env)
        .expect_err("unresolved reference should fail");

    assert_eq!(error, ConstResolutionError::UnresolvedReference);
}

// ------------------------------
//  Function call fails
// ------------------------------

#[test]
fn function_call_fails_const_resolution() {
    let mut string_table = StringTable::new();
    let expression = Expression::function_call(
        InternedPath::from_single_str("foo", &mut string_table),
        vec![],
        vec![builtin_type_ids::INT],
        empty_location(),
    );

    let env = ConstValueEnvironment::default();
    let mut resolver = make_resolver(&mut string_table);

    let error = resolver
        .resolve_expression(&expression, &env)
        .expect_err("function call should fail");

    assert_eq!(error, ConstResolutionError::CallInConstContext);
}

// ------------------------------
//  Mutable declaration fails
// ------------------------------

#[test]
fn mutable_declaration_fails_private_const_resolution() {
    let mut string_table = StringTable::new();
    let declaration = Declaration {
        id: InternedPath::from_single_str("value", &mut string_table),
        value: Expression::int(1, empty_location(), ValueMode::MutableOwned),
    };

    let env = ConstValueEnvironment::default();
    let mut resolver = make_resolver(&mut string_table);

    let error = resolver
        .resolve_private_top_level_declaration(&declaration, &env)
        .expect_err("mutable declaration should fail");

    assert_eq!(error, ConstResolutionError::MutableDeclaration);
}

#[test]
fn explicit_top_level_constant_ignores_value_mode() {
    let mut string_table = StringTable::new();
    let declaration = Declaration {
        id: InternedPath::from_single_str("value", &mut string_table),
        value: Expression::int(1, empty_location(), ValueMode::MutableOwned),
    };

    let env = ConstValueEnvironment::default();
    let mut resolver = make_resolver(&mut string_table);

    // Explicit constants are const by syntax; the resolver does not check mutability.
    let fact = resolver
        .resolve_explicit_top_level_constant(&declaration, &env)
        .expect("explicit constant should resolve");

    assert_eq!(fact.scope, ConstBindingScope::ExplicitTopLevel);
    assert_eq!(fact.source, ConstBindingSource::ExplicitHash);
    assert!(matches!(
        fact.resolved_expression.kind,
        ExpressionKind::Int(1)
    ));
}

// ------------------------------
//  Fact value kinds
// ------------------------------

#[test]
fn fact_value_kind_from_literal_is_literal() {
    let expression = Expression::int(1, SourceLocation::default(), ValueMode::ImmutableOwned);
    assert_eq!(
        ConstFactValueKind::from_expression(&expression),
        ConstFactValueKind::Literal
    );
}

#[test]
fn fact_value_kind_from_runtime_is_non_const() {
    let expression = Expression::runtime_with_type_id(
        ExpressionRpn { items: vec![] },
        DataType::Int,
        builtin_type_ids::INT,
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
    );
    assert_eq!(
        ConstFactValueKind::from_expression(&expression),
        ConstFactValueKind::NonConst
    );
}

// ------------------------------
//  Coerced expression resolution
// ------------------------------

#[test]
fn coerced_expression_resolves_inner_value() {
    let mut string_table = StringTable::new();
    let inner = Expression::int(7, empty_location(), ValueMode::ImmutableOwned);
    let coerced = Expression::coerced(inner, builtin_type_ids::FLOAT);

    let env = ConstValueEnvironment::default();
    let mut resolver = make_resolver(&mut string_table);

    let result = resolver
        .resolve_expression(&coerced, &env)
        .expect("coerced literal should resolve");

    // The fast path preserves the Coerced wrapper because it is_compile_time_constant.
    assert!(matches!(result.kind, ExpressionKind::Coerced { .. }));
}

// ------------------------------
//  Runtime RPN with non-const reference fails
// ------------------------------

#[test]
fn runtime_rpn_with_unresolved_reference_fails() {
    let mut string_table = StringTable::new();
    let rpn = ExpressionRpn {
        items: vec![
            rvalue_item(Expression::reference(
                InternedPath::from_single_str("missing", &mut string_table),
                DataType::Int,
                empty_location(),
                ValueMode::ImmutableReference,
            )),
            rvalue_item(Expression::int(
                2,
                empty_location(),
                ValueMode::ImmutableOwned,
            )),
            operator_item(Operator::Add),
        ],
    };
    let expression = Expression::runtime_with_type_id(
        rpn,
        DataType::Int,
        builtin_type_ids::INT,
        empty_location(),
        ValueMode::ImmutableOwned,
    );

    let env = ConstValueEnvironment::default();
    let mut resolver = make_resolver(&mut string_table);

    let error = resolver
        .resolve_expression(&expression, &env)
        .expect_err("unresolved reference in RPN should fail");

    assert_eq!(error, ConstResolutionError::UnresolvedReference);
}

// ------------------------------
//  Body-local declaration resolution
// ------------------------------

#[test]
fn body_local_immutable_literal_resolves() {
    let mut string_table = StringTable::new();
    let declaration = Declaration {
        id: InternedPath::from_single_str("local", &mut string_table),
        value: Expression::int(99, empty_location(), ValueMode::ImmutableOwned),
    };

    let env = ConstValueEnvironment::default();
    let mut resolver = make_resolver(&mut string_table);

    let fact = resolver
        .resolve_body_local_declaration(&declaration, &env)
        .expect("body-local immutable literal should resolve");

    assert_eq!(fact.scope, ConstBindingScope::BodyLocal);
    assert_eq!(fact.source, ConstBindingSource::InferredImmutable);
    assert!(matches!(
        fact.resolved_expression.kind,
        ExpressionKind::Int(99)
    ));
}

#[test]
fn body_local_mutable_declaration_fails() {
    let mut string_table = StringTable::new();
    let declaration = Declaration {
        id: InternedPath::from_single_str("local", &mut string_table),
        value: Expression::int(99, empty_location(), ValueMode::MutableOwned),
    };

    let env = ConstValueEnvironment::default();
    let mut resolver = make_resolver(&mut string_table);

    let error = resolver
        .resolve_body_local_declaration(&declaration, &env)
        .expect_err("body-local mutable declaration should fail");

    assert_eq!(error, ConstResolutionError::MutableDeclaration);
}
