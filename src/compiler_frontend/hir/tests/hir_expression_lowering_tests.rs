//! HIR expression lowering regression tests.
//!
//! WHAT: covers how typed AST expressions become HIR values, preludes, and places.
//! WHY: expression lowering is broad and subtle enough that behavior changes need focused regression tests.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::{
    CallAccessMode, CallArgument, CallPassingMode,
};
use crate::compiler_frontend::ast::expressions::expression::{
    ConstRecordState, Expression, ExpressionKind,
    FallibleCarrierVariant as AstFallibleCarrierVariant, FallibleHandling, Operator,
};
use crate::compiler_frontend::ast::statements::value_production::ProducedValues;
use crate::compiler_frontend::ast::templates::template::{SlotKey, SlotPlaceholder, TemplateAtom};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::builtins::CollectionBuiltinOp;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::datatypes::definitions::{
    BuiltinTypeDefinition, ChoiceTypeDefinition, ConstructedTypeDefinition, TypeDefinition,
};
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::datatypes::ids::{
    BuiltinTypeConstructor, BuiltinTypeKey, NominalTypeId, TypeConstructor, TypeId,
};
use crate::compiler_frontend::declaration_syntax::choice::{ChoiceVariant, ChoiceVariantPayload};
use crate::compiler_frontend::external_packages::{CallTarget, ExternalFunctionId};
use crate::compiler_frontend::hir::blocks::{HirBlock, HirLocal};
use crate::compiler_frontend::hir::expressions::{HirExpressionKind, HirVariantCarrier, ValueKind};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_side_table::HirLocalOriginKind;
use crate::compiler_frontend::hir::ids::{
    BlockId, ChoiceId, FieldId, FunctionId, LocalId, RegionId, StructId,
};
use crate::compiler_frontend::hir::operators::{HirBinOp, HirUnaryOp};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::statements::HirStatementKind;
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::type_id_fixture_support::{
    choice_construct_expr, const_record_reference_expr, field_access_node, handled_result_expr,
    option_none_expr, reference_expr, result_carrier_type_id, runtime_expr,
};
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, SourceLocation};
use crate::compiler_frontend::value_mode::ValueMode;

fn setup_builder(string_table: &'_ mut StringTable) -> HirBuilder<'_> {
    let test_function_name = InternedPath::from_single_str("__expr_test_fn", string_table);
    let mut builder = HirBuilder::new(
        string_table,
        PathStringFormatConfig::default(),
        crate::compiler_frontend::datatypes::environment::TypeEnvironment::new(),
    );

    let region = RegionId(0);
    let function_id = FunctionId(0);
    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Uninitialized,
    };

    builder.test_push_block(block);
    builder.test_set_current_region(region);
    builder.test_set_current_block(BlockId(0));
    builder.test_register_function_name(test_function_name, function_id);
    builder.test_set_current_function(function_id);

    builder
}

pub(crate) fn location(line: i32) -> SourceLocation {
    SourceLocation {
        scope: InternedPath::new(),
        start_pos: CharPosition {
            line_number: line,
            char_column: 0,
        },
        end_pos: CharPosition {
            line_number: line,
            char_column: 120, // Arbitrary number
        },
    }
}

fn register_local(
    builder: &mut HirBuilder<'_>,
    name: InternedPath,
    local_id: LocalId,
    type_id: TypeId,
    location: SourceLocation,
) {
    let ty = type_id;
    builder.test_register_local_in_block(
        HirLocal {
            id: local_id,
            ty,
            mutable: true,
            region: RegionId(0),
            source_info: Some(location),
        },
        name,
    );
}

fn field_symbol(
    parent: &InternedPath,
    field_name: &str,
    string_table: &mut StringTable,
) -> InternedPath {
    parent.append(string_table.intern(field_name))
}

fn rvalue_node(expr: Expression) -> AstNode {
    let location = expr.location.clone();
    AstNode {
        kind: NodeKind::Rvalue(expr),
        location,
        scope: InternedPath::new(),
    }
}

fn operator_node(op: Operator, location: SourceLocation) -> AstNode {
    AstNode {
        kind: NodeKind::Operator(op),
        location,
        scope: InternedPath::new(),
    }
}

fn runtime_template_expression(location: SourceLocation, content: Vec<Expression>) -> Expression {
    let mut template = Template::empty();
    template.location = location.clone();

    for expr in content {
        template.content.add(expr);
    }

    template.resync_runtime_metadata();
    template.kind =
        crate::compiler_frontend::ast::templates::template::TemplateType::StringFunction;

    Expression::template(template, ValueMode::ImmutableOwned)
}

#[test]
fn compile_time_wrapper_templates_lower_as_runtime_templates_when_they_reach_hir() {
    let mut string_table = StringTable::new();
    let before = string_table.intern("before ");
    let after = string_table.intern("after");
    let location = location(1);
    let mut builder = setup_builder(&mut string_table);

    let mut template = Template::empty();
    template.location = location.clone();
    template.content.add(Expression::string_slice(
        before,
        location.clone(),
        ValueMode::ImmutableOwned,
    ));
    template
        .content
        .atoms
        .push(TemplateAtom::Slot(SlotPlaceholder::new(SlotKey::Default)));
    template.content.add(Expression::string_slice(
        after,
        location,
        ValueMode::ImmutableOwned,
    ));
    template.kind = crate::compiler_frontend::ast::templates::template::TemplateType::String;
    template.resync_runtime_metadata();

    let lowered = builder
        .lower_expression(&Expression::template(template, ValueMode::ImmutableOwned))
        .expect("wrapper-shaped runtime templates should lower in HIR");

    assert_eq!(lowered.prelude.len(), 1);
    let call_args = match &lowered.prelude[0].kind {
        HirStatementKind::Call { args, .. } => args,
        other => panic!("expected template call prelude, got {other:?}"),
    };
    assert_eq!(call_args.len(), 2);
    assert!(matches!(
        call_args[0].kind,
        HirExpressionKind::StringLiteral(ref value) if value == "before "
    ));
    assert!(matches!(
        call_args[1].kind,
        HirExpressionKind::StringLiteral(ref value) if value == "after"
    ));
}

#[test]
fn escaped_slot_insert_helpers_fail_when_they_reach_hir_runtime_lowering() {
    let mut string_table = StringTable::new();
    let text = string_table.intern("content");
    let body_slot = string_table.intern("body");
    let location = location(2);
    let mut builder = setup_builder(&mut string_table);

    let mut helper = Template::empty();
    helper.location = location.clone();
    helper.kind = crate::compiler_frontend::ast::templates::template::TemplateType::SlotInsert(
        SlotKey::named(body_slot),
    );
    helper.content.add(Expression::string_slice(
        text,
        location.clone(),
        ValueMode::ImmutableOwned,
    ));
    helper.resync_runtime_metadata();

    let err = builder
        .lower_expression(&Expression::template(helper, ValueMode::ImmutableOwned))
        .expect_err("escaped helper templates should be rejected in HIR");

    assert_eq!(err.error_type, ErrorType::HirTransformation);
    assert!(
        err.msg
            .contains("Template helper reached HIR runtime-template lowering")
    );
}

#[test]
fn runtime_template_without_render_plan_reports_compiler_bug() {
    let mut string_table = StringTable::new();
    let location = location(2);
    let hello = string_table.intern("hello");
    let mut builder = setup_builder(&mut string_table);
    let mut template = Template::empty();
    template.location = location.clone();
    template.content.add(Expression::string_slice(
        hello,
        location.clone(),
        ValueMode::ImmutableOwned,
    ));
    template.kind =
        crate::compiler_frontend::ast::templates::template::TemplateType::StringFunction;

    let err = builder
        .lower_expression(&Expression::template(template, ValueMode::ImmutableOwned))
        .expect_err("runtime templates without render plans should fail");

    assert_eq!(err.error_type, ErrorType::HirTransformation);
    assert!(err.msg.contains("without a render plan"));
}

#[test]
fn lowers_primitive_literals() {
    let mut string_table = StringTable::new();
    let text = string_table.intern("hello");
    let location = location(1);
    let mut builder = setup_builder(&mut string_table);

    let int_lowered = builder
        .lower_expression(&Expression::int(
            42,
            location.clone(),
            ValueMode::ImmutableOwned,
        ))
        .expect("int lowering should succeed");
    assert!(int_lowered.prelude.is_empty());
    assert_eq!(int_lowered.value.value_kind, ValueKind::Const);
    assert!(matches!(int_lowered.value.kind, HirExpressionKind::Int(42)));

    let float_lowered = builder
        .lower_expression(&Expression::float(
            3.25,
            location.clone(),
            ValueMode::ImmutableOwned,
        ))
        .expect("float lowering should succeed");
    assert!(float_lowered.prelude.is_empty());
    assert_eq!(float_lowered.value.value_kind, ValueKind::Const);
    assert!(matches!(
        float_lowered.value.kind,
        HirExpressionKind::Float(3.25)
    ));

    let bool_lowered = builder
        .lower_expression(&Expression::bool(
            true,
            location.clone(),
            ValueMode::ImmutableOwned,
        ))
        .expect("bool lowering should succeed");
    assert!(bool_lowered.prelude.is_empty());
    assert_eq!(bool_lowered.value.value_kind, ValueKind::Const);
    assert!(matches!(
        bool_lowered.value.kind,
        HirExpressionKind::Bool(true)
    ));

    let char_lowered = builder
        .lower_expression(&Expression::char(
            'x',
            location.clone(),
            ValueMode::ImmutableOwned,
        ))
        .expect("char lowering should succeed");
    assert!(char_lowered.prelude.is_empty());
    assert_eq!(char_lowered.value.value_kind, ValueKind::Const);
    assert!(matches!(
        char_lowered.value.kind,
        HirExpressionKind::Char('x')
    ));

    let string_expr = Expression::string_slice(text, location.clone(), ValueMode::ImmutableOwned);
    let string_lowered = builder
        .lower_expression(&string_expr)
        .expect("string literal lowering should succeed");
    assert!(string_lowered.prelude.is_empty());
    assert_eq!(string_lowered.value.value_kind, ValueKind::Const);
    assert!(matches!(
        string_lowered.value.kind,
        HirExpressionKind::StringLiteral(ref s) if s == "hello"
    ));
}

#[test]
fn lowers_reference_to_registered_local() {
    let mut string_table = StringTable::new();
    let x = super::symbol("x", &mut string_table);
    let location = location(2);
    let mut builder = setup_builder(&mut string_table);

    register_local(
        &mut builder,
        x.clone(),
        LocalId(10),
        builtin_type_ids::INT,
        location.clone(),
    );

    let expr = reference_expr(
        x,
        builtin_type_ids::INT,
        location.clone(),
        ValueMode::ImmutableReference,
    );
    let lowered = builder
        .lower_expression(&expr)
        .expect("reference lowering should succeed");

    assert!(lowered.prelude.is_empty());
    assert_eq!(lowered.value.value_kind, ValueKind::Place);
    assert!(matches!(
        lowered.value.kind,
        HirExpressionKind::Load(HirPlace::Local(LocalId(10)))
    ));
}

#[test]
fn lowers_reference_to_module_constant_when_local_is_missing() {
    let mut string_table = StringTable::new();
    let third_const = super::symbol("third_const", &mut string_table);
    let location = location(3);
    let mut builder = setup_builder(&mut string_table);

    builder.test_register_module_constant(
        third_const.clone(),
        Expression::int(3, location.clone(), ValueMode::ImmutableOwned),
    );

    let expr = reference_expr(
        third_const,
        builtin_type_ids::INT,
        location.clone(),
        ValueMode::ImmutableReference,
    );
    let lowered = builder
        .lower_expression(&expr)
        .expect("module constant reference lowering should succeed");

    assert!(lowered.prelude.is_empty());
    assert_eq!(lowered.value.value_kind, ValueKind::Const);
    assert!(matches!(lowered.value.kind, HirExpressionKind::Int(3)));
}

#[test]
fn rejects_cyclic_module_constant_dependencies() {
    let mut string_table = StringTable::new();
    let const_a = super::symbol("const_a", &mut string_table);
    let const_b = super::symbol("const_b", &mut string_table);
    let location = location(4);
    let mut builder = setup_builder(&mut string_table);

    builder.test_register_module_constant(
        const_a.clone(),
        reference_expr(
            const_b.clone(),
            builtin_type_ids::INT,
            location.clone(),
            ValueMode::ImmutableReference,
        ),
    );
    builder.test_register_module_constant(
        const_b.clone(),
        reference_expr(
            const_a.clone(),
            builtin_type_ids::INT,
            location.clone(),
            ValueMode::ImmutableReference,
        ),
    );

    let err = builder
        .lower_expression(&reference_expr(
            const_a,
            builtin_type_ids::INT,
            location.clone(),
            ValueMode::ImmutableReference,
        ))
        .expect_err("cyclic module constants should fail during HIR lowering");

    assert_eq!(err.error_type, ErrorType::HirTransformation);
    assert!(err.msg.contains("Cyclic module constant dependency"));
}

#[test]
fn lowers_runtime_rpn_arithmetic_stack_correctly() {
    let mut string_table = StringTable::new();
    let x = super::symbol("x", &mut string_table);
    let y = super::symbol("y", &mut string_table);
    let location = location(3);
    let mut builder = setup_builder(&mut string_table);

    register_local(
        &mut builder,
        x.clone(),
        LocalId(10),
        builtin_type_ids::INT,
        location.clone(),
    );
    register_local(
        &mut builder,
        y.clone(),
        LocalId(11),
        builtin_type_ids::INT,
        location.clone(),
    );

    let nodes = vec![
        rvalue_node(reference_expr(
            x,
            builtin_type_ids::INT,
            location.clone(),
            ValueMode::ImmutableReference,
        )),
        rvalue_node(Expression::int(
            2,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        rvalue_node(reference_expr(
            y,
            builtin_type_ids::INT,
            location.clone(),
            ValueMode::ImmutableReference,
        )),
        operator_node(Operator::Multiply, location.clone()),
        operator_node(Operator::Add, location.clone()),
    ];

    let expr = runtime_expr(
        nodes,
        builtin_type_ids::INT,
        location.clone(),
        ValueMode::MutableOwned,
    );
    let lowered = builder
        .lower_expression(&expr)
        .expect("runtime arithmetic lowering should succeed");

    assert!(lowered.prelude.is_empty());
    assert!(matches!(
        lowered.value.kind,
        HirExpressionKind::BinOp {
            op: HirBinOp::Add,
            ..
        }
    ));
}

#[test]
fn runtime_division_subexpression_infers_float_type_in_hir() {
    let mut string_table = StringTable::new();
    let location = location(3);
    let mut builder = setup_builder(&mut string_table);
    let expected_float = builtin_type_ids::FLOAT;

    let nodes = vec![
        rvalue_node(Expression::int(
            5,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        rvalue_node(Expression::int(
            2,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        operator_node(Operator::Divide, location.clone()),
        rvalue_node(Expression::int(
            1,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        operator_node(Operator::Add, location.clone()),
    ];

    let expr = runtime_expr(
        nodes,
        builtin_type_ids::FLOAT,
        location.clone(),
        ValueMode::MutableOwned,
    );
    let lowered = builder
        .lower_expression(&expr)
        .expect("runtime division lowering should succeed");

    let HirExpressionKind::BinOp { left, op, .. } = &lowered.value.kind else {
        panic!("expected outer addition binop");
    };
    assert!(matches!(op, HirBinOp::Add));
    let HirExpressionKind::BinOp { op: inner_op, .. } = &left.kind else {
        panic!("expected left operand to be division binop");
    };
    assert!(matches!(inner_op, HirBinOp::Div));
    assert_eq!(left.ty, expected_float);
    assert_eq!(lowered.value.ty, expected_float);
}

#[test]
fn runtime_integer_division_lowers_to_hir_int_div_with_int_type() {
    let mut string_table = StringTable::new();
    let location = location(3);
    let mut builder = setup_builder(&mut string_table);
    let expected_int = builtin_type_ids::INT;

    let nodes = vec![
        rvalue_node(Expression::int(
            5,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        rvalue_node(Expression::int(
            2,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        operator_node(Operator::IntDivide, location.clone()),
    ];

    let expr = runtime_expr(
        nodes,
        builtin_type_ids::INT,
        location.clone(),
        ValueMode::MutableOwned,
    );
    let lowered = builder
        .lower_expression(&expr)
        .expect("runtime integer division lowering should succeed");

    let HirExpressionKind::BinOp { op, .. } = &lowered.value.kind else {
        panic!("expected integer division binop");
    };
    assert!(matches!(op, HirBinOp::IntDiv));
    assert_eq!(lowered.value.ty, expected_int);
}

#[test]
fn lowers_unary_not_in_runtime_rpn() {
    let mut string_table = StringTable::new();
    let location = location(4);
    let mut builder = setup_builder(&mut string_table);

    let nodes = vec![
        rvalue_node(Expression::bool(
            true,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        operator_node(Operator::Not, location.clone()),
    ];

    let expr = runtime_expr(
        nodes,
        builtin_type_ids::BOOL,
        location.clone(),
        ValueMode::MutableOwned,
    );
    let lowered = builder
        .lower_expression(&expr)
        .expect("unary not lowering should succeed");

    assert!(matches!(
        lowered.value.kind,
        HirExpressionKind::UnaryOp {
            op: HirUnaryOp::Not,
            ..
        }
    ));
}

#[test]
fn lowers_range_operator_in_runtime_rpn() {
    let mut string_table = StringTable::new();
    let location = location(5);
    let mut builder = setup_builder(&mut string_table);

    let nodes = vec![
        rvalue_node(Expression::int(
            1,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        rvalue_node(Expression::int(
            9,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        operator_node(Operator::Range, location.clone()),
    ];

    let expr = runtime_expr(
        nodes,
        builtin_type_ids::RANGE,
        location.clone(),
        ValueMode::MutableOwned,
    );
    let lowered = builder
        .lower_expression(&expr)
        .expect("range lowering should succeed");

    assert!(matches!(
        lowered.value.kind,
        HirExpressionKind::Range { .. }
    ));
}

#[test]
fn lowers_function_call_to_call_statement_and_temp_load() {
    let mut string_table = StringTable::new();
    let function_name = super::symbol("sum", &mut string_table);
    let location = location(6);
    let mut builder = setup_builder(&mut string_table);
    builder.test_register_function_name(function_name.clone(), FunctionId(2));

    let call_expr = Expression::function_call(
        function_name.clone(),
        vec![Expression::int(
            7,
            location.clone(),
            ValueMode::ImmutableOwned,
        )],
        vec![builtin_type_ids::INT],
        location.clone(),
    );

    let lowered = builder
        .lower_expression(&call_expr)
        .expect("function call lowering should succeed");
    assert_eq!(lowered.prelude.len(), 1);

    let statement = &lowered.prelude[0];
    let result_local = match &statement.kind {
        HirStatementKind::Call {
            target,
            args,
            result,
        } => {
            assert_eq!(target, &CallTarget::UserFunction(FunctionId(2)));
            assert_eq!(args.len(), 1);
            result.expect("call with return should bind a temp local")
        }
        _ => panic!("expected lowered call statement"),
    };

    assert!(matches!(
        lowered.value.kind,
        HirExpressionKind::Load(HirPlace::Local(local))
        if local == result_local
    ));
    assert_eq!(lowered.value.value_kind, ValueKind::RValue);
}

#[test]
fn expression_function_call_uses_variant_result_type_ids_for_single_return() {
    let mut string_table = StringTable::new();
    let function_name = super::symbol("typed_result", &mut string_table);
    let location = location(6);
    let mut builder = setup_builder(&mut string_table);
    builder.test_register_function_name(function_name.clone(), FunctionId(32));

    let call_expr = Expression::function_call_with_typed_arguments(
        function_name,
        vec![],
        vec![builtin_type_ids::INT],
        &mut builder.type_environment,
        location.clone(),
    );
    assert_eq!(call_expr.type_id, builtin_type_ids::INT);
    assert!(
        matches!(
            &call_expr.kind,
            ExpressionKind::FunctionCall { result_type_ids, .. }
                if result_type_ids.as_slice() == [builtin_type_ids::INT]
        ),
        "typed function-call construction should store canonical result TypeIds immediately"
    );

    let expected_int = builder
        .lower_type_id(builtin_type_ids::INT, &location)
        .expect("builtin Int TypeId should lower in test context");
    let lowered = builder
        .lower_expression(&call_expr)
        .expect("function call lowering should use variant result TypeIds");

    assert_eq!(lowered.value.ty, expected_int);
}

#[test]
fn expression_function_call_uses_variant_result_type_ids_for_no_return() {
    let mut string_table = StringTable::new();
    let function_name = super::symbol("no_result", &mut string_table);
    let location = location(6);
    let mut builder = setup_builder(&mut string_table);
    builder.test_register_function_name(function_name.clone(), FunctionId(33));

    let call_expr = Expression::function_call(function_name, vec![], vec![], location.clone());

    let lowered = builder
        .lower_expression(&call_expr)
        .expect("no-return call lowering should use empty variant result TypeIds");
    let lowered_type = builder.type_environment.get(lowered.value.ty);

    assert!(
        matches!(
            lowered_type,
            Some(TypeDefinition::Builtin(BuiltinTypeDefinition {
                key: BuiltinTypeKey::None,
            }))
        ),
        "expected no-return expression call to lower as Unit, got {lowered_type:?}"
    );
}

#[test]
fn expression_function_call_uses_variant_result_type_ids_for_multi_return() {
    let mut string_table = StringTable::new();
    let function_name = super::symbol("multi_result", &mut string_table);
    let location = location(6);
    let mut builder = setup_builder(&mut string_table);
    builder.test_register_function_name(function_name.clone(), FunctionId(34));

    let call_expr = Expression::function_call_with_typed_arguments(
        function_name,
        vec![],
        vec![builtin_type_ids::INT, builtin_type_ids::BOOL],
        &mut builder.type_environment,
        location.clone(),
    );
    assert!(
        matches!(
            &call_expr.kind,
            ExpressionKind::FunctionCall { result_type_ids, .. }
                if result_type_ids.as_slice() == [builtin_type_ids::INT, builtin_type_ids::BOOL]
        ),
        "typed function-call construction should preserve canonical multi-return TypeIds"
    );

    let lowered = builder
        .lower_expression(&call_expr)
        .expect("multi-return call lowering should use variant result TypeIds");
    let int_type = builder
        .lower_type_id(builtin_type_ids::INT, &location)
        .expect("builtin Int TypeId should lower in test context");
    let bool_type = builder
        .lower_type_id(builtin_type_ids::BOOL, &location)
        .expect("builtin Bool TypeId should lower in test context");
    let lowered_type = builder.type_environment.get(lowered.value.ty);

    assert!(
        matches!(
            lowered_type,
            Some(TypeDefinition::Constructed(ConstructedTypeDefinition {
                constructor: TypeConstructor::Builtin(BuiltinTypeConstructor::Tuple),
                arguments,
            })) if arguments.as_ref() == [int_type, bool_type]
        ),
        "expected multi-return expression call to lower as tuple(Int, Bool), got {lowered_type:?}"
    );
}

#[test]
fn expression_host_call_uses_variant_result_type_ids() {
    let mut string_table = StringTable::new();
    let location = location(6);
    let mut builder = setup_builder(&mut string_table);

    let call_expr = Expression::host_function_call_with_typed_arguments(
        ExternalFunctionId::Io,
        vec![],
        vec![builtin_type_ids::INT],
        &mut builder.type_environment,
        location.clone(),
    );
    assert!(
        matches!(
            &call_expr.kind,
            ExpressionKind::HostFunctionCall { result_type_ids, .. }
                if result_type_ids.as_slice() == [builtin_type_ids::INT]
        ),
        "typed host-call construction should store canonical result TypeIds immediately"
    );

    let expected_int = builder
        .lower_type_id(builtin_type_ids::INT, &location)
        .expect("builtin Int TypeId should lower in test context");
    let lowered = builder
        .lower_expression(&call_expr)
        .expect("host call lowering should use variant result TypeIds");

    assert_eq!(lowered.value.ty, expected_int);
}

#[test]
fn expression_handled_fallible_call_fallback_uses_variant_result_type_ids() {
    let mut string_table = StringTable::new();
    let function_name = super::symbol("handled_result", &mut string_table);
    let location = location(6);
    let mut builder = setup_builder(&mut string_table);
    let ok_type = builder
        .lower_type_id(builtin_type_ids::INT, &location)
        .expect("builtin Int TypeId should lower in test context");
    let err_type = builder
        .lower_type_id(builtin_type_ids::STRING, &location)
        .expect("builtin String TypeId should lower in test context");
    let carrier_type = result_carrier_type_id(&mut builder.type_environment, ok_type, err_type);
    builder.test_register_function_with_return_type(
        function_name.clone(),
        FunctionId(35),
        carrier_type,
    );
    let test_scope = function_name.clone();

    let call_expr = Expression::handled_fallible_function_call_with_typed_arguments(
        function_name,
        vec![],
        vec![builtin_type_ids::INT],
        FallibleHandling::Handler {
            error: None,
            body: vec![AstNode {
                kind: NodeKind::ThenValue(ProducedValues {
                    expressions: vec![Expression::int(
                        7,
                        location.clone(),
                        ValueMode::ImmutableOwned,
                    )],
                    location: location.clone(),
                }),
                location: location.clone(),
                scope: test_scope,
            }],
        },
        &mut builder.type_environment,
        location.clone(),
    );
    assert!(
        matches!(
            &call_expr.kind,
            ExpressionKind::HandledFallibleFunctionCall { result_type_ids, .. }
                if result_type_ids.as_slice() == [builtin_type_ids::INT]
        ),
        "typed handled fallible call construction should store canonical result TypeIds immediately"
    );

    let lowered = builder
        .lower_expression(&call_expr)
        .expect("handled fallible call lowering should use variant result TypeIds");

    assert_eq!(lowered.value.ty, ok_type);
}

#[test]
fn expression_handled_result_derives_success_slots_from_tuple_type_id() {
    let mut string_table = StringTable::new();
    let function_name = super::symbol("handled_result_expr", &mut string_table);
    let location = location(6);
    let mut builder = setup_builder(&mut string_table);
    builder.test_register_function_name(function_name.clone(), FunctionId(36));

    let ok_type = builder
        .type_environment
        .intern_tuple(vec![builtin_type_ids::INT, builtin_type_ids::BOOL]);
    let err_type = builtin_type_ids::STRING;
    let carrier_type = result_carrier_type_id(&mut builder.type_environment, ok_type, err_type);

    let result_expr = Expression::function_call_with_typed_arguments(
        function_name,
        vec![],
        vec![carrier_type],
        &mut builder.type_environment,
        location.clone(),
    );
    assert_eq!(result_expr.type_id, carrier_type);
    let test_scope = InternedPath::new();

    let handled_expr = handled_result_expr(
        result_expr,
        FallibleHandling::Handler {
            error: None,
            body: vec![AstNode {
                kind: NodeKind::ThenValue(ProducedValues {
                    expressions: vec![
                        Expression::int(7, location.clone(), ValueMode::ImmutableOwned),
                        Expression::bool(false, location.clone(), ValueMode::ImmutableOwned),
                    ],
                    location: location.clone(),
                }),
                location: location.clone(),
                scope: test_scope,
            }],
        },
        ok_type,
        location.clone(),
    );

    let lowered = builder
        .lower_expression(&handled_expr)
        .expect("handled Result expression should preserve multi-success tuple typing");

    assert_eq!(lowered.value.ty, ok_type);
}

#[test]
fn lowers_fresh_mutable_call_argument_via_hidden_local_with_origin_metadata() {
    let mut string_table = StringTable::new();
    let function_name = super::symbol("mutate", &mut string_table);
    let location = location(6);
    let mut builder = setup_builder(&mut string_table);
    builder.test_register_function_name(function_name.clone(), FunctionId(24));

    let fresh_argument = CallArgument::positional(
        Expression::int(7, location.clone(), ValueMode::ImmutableOwned),
        CallAccessMode::Shared,
        location.clone(),
    )
    .with_passing_mode(CallPassingMode::FreshMutableValue);

    let call_expr = Expression::function_call_with_arguments(
        function_name,
        vec![fresh_argument],
        vec![],
        location.clone(),
    );

    let lowered = builder
        .lower_expression(&call_expr)
        .expect("fresh mutable argument lowering should succeed");

    assert_eq!(
        lowered.prelude.len(),
        2,
        "fresh mutable args should materialize assignment before call"
    );

    let temp_local = match &lowered.prelude[0].kind {
        HirStatementKind::Assign { target, value } => {
            assert!(matches!(value.kind, HirExpressionKind::Int(7)));
            match target {
                HirPlace::Local(local) => *local,
                other => panic!("expected local assignment target, got {other:?}"),
            }
        }
        other => panic!("expected first prelude statement to assign fresh arg temp, got {other:?}"),
    };

    match &lowered.prelude[1].kind {
        HirStatementKind::Call { args, .. } => {
            assert_eq!(args.len(), 1);
            assert!(
                matches!(
                    args[0].kind,
                    HirExpressionKind::Load(HirPlace::Local(local)) if local == temp_local
                ),
                "call argument should load synthesized fresh-arg local"
            );
        }
        other => panic!("expected second prelude statement to be call, got {other:?}"),
    }

    let origin = builder
        .side_table
        .local_origin(temp_local)
        .expect("fresh mutable arg local should have side-table origin metadata");
    assert_eq!(origin.kind, HirLocalOriginKind::CompilerFreshMutableArg);
    assert_eq!(origin.argument_index, Some(0));

    let call_location = origin
        .call_location
        .and_then(|id| builder.side_table.source_location(id))
        .expect("fresh mutable arg local should record originating call location");
    assert_eq!(
        call_location.start_pos.line_number,
        location.start_pos.line_number
    );
}

#[test]
fn lowers_receiver_method_call_with_receiver_as_first_argument() {
    let mut string_table = StringTable::new();
    let method_path = super::symbol("Vector2/reset", &mut string_table);
    let method_name = string_table.intern("reset");
    let receiver_name = super::symbol("vec", &mut string_table);
    let receiver_struct = super::symbol("Vector2", &mut string_table);
    let location = location(6);
    let mut builder = setup_builder(&mut string_table);

    builder.test_register_function_name(method_path.clone(), FunctionId(22));

    let receiver_type_id =
        builder.test_register_nominal_struct_type(receiver_struct.clone(), vec![], false);
    builder.test_register_struct_with_fields(
        StructId(21),
        receiver_struct.clone(),
        receiver_type_id,
        vec![],
    );
    register_local(
        &mut builder,
        receiver_name.clone(),
        LocalId(23),
        receiver_type_id,
        location.clone(),
    );

    let receiver = AstNode {
        kind: NodeKind::Rvalue(reference_expr(
            receiver_name,
            receiver_type_id,
            location.clone(),
            ValueMode::MutableReference,
        )),
        location: location.clone(),
        scope: InternedPath::new(),
    };

    let lowered = builder
        .lower_ast_node_as_expression(&AstNode {
            kind: NodeKind::MethodCall {
                receiver: Box::new(receiver),
                method_path: method_path.clone(),
                method: method_name,
                args: vec![CallArgument::positional(
                    Expression::int(7, location.clone(), ValueMode::ImmutableOwned),
                    CallAccessMode::Shared,
                    location.clone(),
                )],
                result_type_ids: vec![builtin_type_ids::INT],
                location: location.clone(),
            },
            location: location.clone(),
            scope: InternedPath::new(),
        })
        .expect("receiver method call lowering should succeed");

    assert_eq!(lowered.prelude.len(), 1);

    match &lowered.prelude[0].kind {
        HirStatementKind::Call { target, args, .. } => {
            assert_eq!(target, &CallTarget::UserFunction(FunctionId(22)));
            assert_eq!(args.len(), 2);
            assert!(matches!(
                args[0].kind,
                HirExpressionKind::Load(HirPlace::Local(LocalId(23)))
            ));
            assert!(matches!(args[1].kind, HirExpressionKind::Int(7)));
        }
        other => panic!("expected lowered receiver call statement, got {other:?}"),
    }
}

#[test]
fn lowers_builtin_scalar_receiver_method_call_with_receiver_as_first_argument() {
    let mut string_table = StringTable::new();
    let method_path = super::symbol("Int/double", &mut string_table);
    let method_name = string_table.intern("double");
    let receiver_name = super::symbol("value", &mut string_table);
    let location = location(12);
    let mut builder = setup_builder(&mut string_table);

    builder.test_register_function_name(method_path.clone(), FunctionId(41));

    register_local(
        &mut builder,
        receiver_name.clone(),
        LocalId(42),
        builtin_type_ids::INT,
        location.clone(),
    );

    let receiver = AstNode {
        kind: NodeKind::Rvalue(reference_expr(
            receiver_name,
            builtin_type_ids::INT,
            location.clone(),
            ValueMode::ImmutableReference,
        )),
        location: location.clone(),
        scope: InternedPath::new(),
    };

    let lowered = builder
        .lower_ast_node_as_expression(&AstNode {
            kind: NodeKind::MethodCall {
                receiver: Box::new(receiver),
                method_path: method_path.clone(),
                method: method_name,
                args: vec![],
                result_type_ids: vec![builtin_type_ids::INT],
                location: location.clone(),
            },
            location: location.clone(),
            scope: InternedPath::new(),
        })
        .expect("builtin scalar receiver method call lowering should succeed");

    assert_eq!(lowered.prelude.len(), 1);
    match &lowered.prelude[0].kind {
        HirStatementKind::Call { target, args, .. } => {
            assert_eq!(target, &CallTarget::UserFunction(FunctionId(41)));
            assert_eq!(args.len(), 1);
            assert!(matches!(
                args[0].kind,
                HirExpressionKind::Load(HirPlace::Local(LocalId(42)))
            ));
        }
        other => panic!("expected lowered builtin scalar receiver call statement, got {other:?}"),
    }
}

#[test]
fn lowers_host_call_expression_with_host_target() {
    let mut string_table = StringTable::new();
    let literal_x = string_table.intern("x");
    let location = location(7);
    let mut builder = setup_builder(&mut string_table);

    let host_call = Expression::host_function_call(
        crate::compiler_frontend::external_packages::ExternalFunctionId::Io,
        vec![Expression::string_slice(
            literal_x,
            location.clone(),
            ValueMode::ImmutableOwned,
        )],
        vec![builtin_type_ids::INT],
        location.clone(),
    );

    let lowered = builder
        .lower_expression(&host_call)
        .expect("host call lowering should succeed");
    assert_eq!(lowered.prelude.len(), 1);
    let target = match &lowered.prelude[0].kind {
        HirStatementKind::Call { target, .. } => target,
        _ => panic!("expected call statement for host call"),
    };
    assert_eq!(
        target,
        &CallTarget::ExternalFunction(
            crate::compiler_frontend::external_packages::ExternalFunctionId::Io
        )
    );
}

#[test]
fn preserves_left_to_right_call_prelude_order_in_nested_call_args() {
    let mut string_table = StringTable::new();
    let first = super::symbol("first", &mut string_table);
    let second = super::symbol("second", &mut string_table);
    let outer = super::symbol("outer", &mut string_table);
    let location = location(8);
    let mut builder = setup_builder(&mut string_table);

    builder.test_register_function_name(first.clone(), FunctionId(1));
    builder.test_register_function_name(second.clone(), FunctionId(2));
    builder.test_register_function_name(outer.clone(), FunctionId(3));

    let arg_one = Expression::function_call(
        first.clone(),
        vec![],
        vec![builtin_type_ids::INT],
        location.clone(),
    );
    let arg_two = Expression::function_call(
        second.clone(),
        vec![],
        vec![builtin_type_ids::INT],
        location.clone(),
    );
    let outer_call = Expression::function_call(
        outer.clone(),
        vec![arg_one, arg_two],
        vec![builtin_type_ids::INT],
        location,
    );

    let lowered = builder
        .lower_expression(&outer_call)
        .expect("nested call lowering should succeed");

    assert_eq!(lowered.prelude.len(), 3);

    let targets = lowered
        .prelude
        .iter()
        .map(|statement| match &statement.kind {
            HirStatementKind::Call { target, .. } => target.clone(),
            _ => panic!("expected call statement in nested call prelude"),
        })
        .collect::<Vec<_>>();

    assert_eq!(
        targets,
        vec![
            CallTarget::UserFunction(FunctionId(1)),
            CallTarget::UserFunction(FunctionId(2)),
            CallTarget::UserFunction(FunctionId(3)),
        ]
    );
}

#[test]
fn malformed_runtime_rpn_reports_hir_transformation_error() {
    let mut string_table = StringTable::new();
    let location = location(9);
    let mut builder = setup_builder(&mut string_table);

    let expr = runtime_expr(
        vec![operator_node(Operator::Add, location.clone())],
        builtin_type_ids::INT,
        location,
        ValueMode::MutableOwned,
    );

    let err = builder
        .lower_expression(&expr)
        .expect_err("malformed rpn should fail");
    assert_eq!(err.error_type, ErrorType::HirTransformation);
    assert!(
        err.msg.contains("underflow"),
        "expected stack underflow message, got: {}",
        err.msg
    );
}

#[test]
fn runtime_template_expression_lowers_to_synthetic_function_call() {
    let mut string_table = StringTable::new();
    let hello = string_table.intern("hello");
    let location = location(10);
    let mut builder = setup_builder(&mut string_table);

    let expr = runtime_template_expression(
        location.clone(),
        vec![Expression::string_slice(
            hello,
            location,
            ValueMode::ImmutableOwned,
        )],
    );

    let lowered = builder
        .lower_expression(&expr)
        .expect("runtime template lowering should succeed");
    assert_eq!(lowered.prelude.len(), 1);

    let template_target = match &lowered.prelude[0].kind {
        HirStatementKind::Call {
            target: CallTarget::UserFunction(function_id),
            result: Some(_),
            ..
        } => *function_id,
        other => panic!("expected synthetic template call, got {other:?}"),
    };

    assert!(
        builder
            .side_table
            .resolve_function_name(template_target, &string_table)
            .is_some_and(|name| name.starts_with("__template_fn_"))
    );
    assert!(matches!(
        lowered.value.kind,
        HirExpressionKind::Load(HirPlace::Local(_))
    ));
}

#[test]
fn runtime_template_generated_function_coerces_non_string_segments() {
    let mut string_table = StringTable::new();
    let location = location(11);
    let mut builder = setup_builder(&mut string_table);

    let expr = runtime_template_expression(
        location.clone(),
        vec![Expression::int(5, location, ValueMode::ImmutableOwned)],
    );

    let lowered = builder
        .lower_expression(&expr)
        .expect("runtime template lowering should succeed");
    assert_eq!(lowered.prelude.len(), 1);

    let template_target = match &lowered.prelude[0].kind {
        HirStatementKind::Call {
            target: CallTarget::UserFunction(function_id),
            ..
        } => *function_id,
        other => panic!("expected synthetic template call, got {other:?}"),
    };

    let template_function = builder
        .module
        .functions
        .iter()
        .find(|function| function.id == template_target)
        .expect("synthetic template function should be present");
    let template_entry = builder
        .module
        .blocks
        .iter()
        .find(|block| block.id == template_function.entry)
        .expect("synthetic template entry block should exist");

    let returned = match &template_entry.terminator {
        HirTerminator::Return(value) => value,
        other => panic!("expected template function return terminator, got {other:?}"),
    };

    let coerced_chunk = match &returned.kind {
        HirExpressionKind::BinOp {
            op: HirBinOp::Add,
            right,
            ..
        } => right,
        other => {
            panic!("expected template return to concatenate accumulated string, got {other:?}")
        }
    };

    let (left, right) = match &coerced_chunk.kind {
        HirExpressionKind::BinOp {
            op: HirBinOp::Add,
            left,
            right,
        } => (left, right),
        other => panic!("expected coercion concat for non-string template segment, got {other:?}"),
    };

    assert!(matches!(
        left.kind,
        HirExpressionKind::StringLiteral(ref value) if value.is_empty()
    ));
    assert!(matches!(
        right.kind,
        HirExpressionKind::Load(HirPlace::Local(_))
    ));
}

#[test]
fn runtime_template_lowers_nested_templates_in_order() {
    let mut string_table = StringTable::new();
    let a = string_table.intern("A");
    let b = string_table.intern("B");
    let c = string_table.intern("C");
    let location = location(12);
    let mut builder = setup_builder(&mut string_table);

    let nested = runtime_template_expression(
        location.clone(),
        vec![Expression::string_slice(
            b,
            location.clone(),
            ValueMode::ImmutableOwned,
        )],
    );

    let expr = runtime_template_expression(
        location.clone(),
        vec![
            Expression::string_slice(a, location.clone(), ValueMode::ImmutableOwned),
            nested,
            Expression::string_slice(c, location, ValueMode::ImmutableOwned),
        ],
    );

    let lowered = builder
        .lower_expression(&expr)
        .expect("nested runtime template lowering should succeed");
    assert_eq!(lowered.prelude.len(), 2);

    assert!(matches!(
        lowered.prelude[0].kind,
        HirStatementKind::Call {
            target: CallTarget::UserFunction(_),
            result: Some(_),
            ..
        }
    ));
    assert!(matches!(
        lowered.prelude[1].kind,
        HirStatementKind::Call {
            target: CallTarget::UserFunction(_),
            result: Some(_),
            ..
        }
    ));

    let outer_call_args = match &lowered.prelude[1].kind {
        HirStatementKind::Call { args, .. } => args,
        other => panic!("expected outer template call statement, got {other:?}"),
    };
    assert_eq!(outer_call_args.len(), 3);
    assert!(matches!(
        outer_call_args[0].kind,
        HirExpressionKind::StringLiteral(ref value) if value == "A"
    ));
    assert!(matches!(
        outer_call_args[1].kind,
        HirExpressionKind::Load(HirPlace::Local(_))
    ));
    assert!(matches!(
        outer_call_args[2].kind,
        HirExpressionKind::StringLiteral(ref value) if value == "C"
    ));

    assert_eq!(builder.module.functions.len(), 2);
}

#[test]
fn local_resolution_uses_full_path_identity_not_leaf_name() {
    let mut string_table = StringTable::new();
    let x_leaf = string_table.intern("x");
    let scope_a = InternedPath::from_single_str("scope_a", &mut string_table);
    let scope_b = InternedPath::from_single_str("scope_b", &mut string_table);
    let local_a = scope_a.append(x_leaf);
    let local_b = scope_b.append(x_leaf);
    let location = location(10);
    let mut builder = setup_builder(&mut string_table);

    register_local(
        &mut builder,
        local_a,
        LocalId(22),
        builtin_type_ids::INT,
        location.clone(),
    );

    let expr = reference_expr(
        local_b,
        builtin_type_ids::INT,
        location.clone(),
        ValueMode::ImmutableReference,
    );
    let err = builder
        .lower_expression(&expr)
        .expect_err("unregistered full-path symbol should not resolve by leaf name");

    assert_eq!(err.error_type, ErrorType::HirTransformation);
    assert!(err.msg.contains("Unresolved local"));
}

#[test]
fn nominal_struct_identity_uses_field_parent_path() {
    let mut string_table = StringTable::new();
    let location = location(11);
    let struct_path = super::symbol("MyStruct", &mut string_table);
    let field_path = field_symbol(&struct_path, "value", &mut string_table);
    let mut builder = setup_builder(&mut string_table);
    let int_type = builtin_type_ids::INT;

    builder.test_register_struct_with_fields(
        StructId(1),
        struct_path.clone(),
        int_type,
        vec![(FieldId(3), field_path.clone(), int_type)],
    );

    let struct_type_id = builder.test_register_nominal_struct_type(
        struct_path.clone(),
        vec![(field_path.clone(), int_type, location.clone())],
        false,
    );

    let expr_fields = vec![Declaration {
        id: field_path.clone(),
        value: Expression::int(42, location.clone(), ValueMode::ImmutableOwned),
    }];

    let expression = Expression::struct_instance(
        struct_path.clone(),
        expr_fields.clone(),
        location.clone(),
        ValueMode::MutableOwned,
        false,
        None,
        struct_type_id,
    );

    let lowered = builder
        .lower_expression(&expression)
        .expect("struct instance lowering should succeed");

    match lowered.value.kind {
        HirExpressionKind::StructConstruct { struct_id, fields } => {
            assert_eq!(struct_id, StructId(1));
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].0, FieldId(3));
        }
        other => panic!("expected StructConstruct, got {other:?}"),
    }
}

#[test]
fn rejects_const_record_struct_instance_runtime_lowering() {
    let mut string_table = StringTable::new();
    let location = location(12);
    let struct_path = super::symbol("Palette", &mut string_table);
    let field_path = field_symbol(&struct_path, "red", &mut string_table);
    let field_value = string_table.intern("red");
    let mut builder = setup_builder(&mut string_table);

    let const_record_type_id = builder.test_register_nominal_struct_type(
        struct_path.clone(),
        vec![(
            field_path.clone(),
            builtin_type_ids::STRING,
            location.clone(),
        )],
        true,
    );

    let expression = Expression::struct_instance(
        struct_path,
        vec![Declaration {
            id: field_path,
            value: Expression::string_slice(
                field_value,
                location.clone(),
                ValueMode::ImmutableOwned,
            ),
        }],
        location.clone(),
        ValueMode::ImmutableOwned,
        true,
        None,
        const_record_type_id,
    );

    let error = builder
        .lower_expression(&expression)
        .expect_err("const record should not lower as a runtime struct construct");

    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error
            .msg
            .contains("Const record reached runtime HIR struct lowering")
    );
}

#[test]
fn temp_locals_are_not_resolvable_as_user_symbols() {
    let mut string_table = StringTable::new();
    let callee = super::symbol("callee", &mut string_table);
    let temp_name = super::symbol("__hir_tmp_0", &mut string_table);
    let location = location(12);
    let mut builder = setup_builder(&mut string_table);

    builder.test_register_function_name(callee.clone(), FunctionId(8));

    let call_expr = Expression::function_call(
        callee,
        vec![],
        vec![builtin_type_ids::INT],
        location.clone(),
    );
    let lowered = builder
        .lower_expression(&call_expr)
        .expect("call lowering should succeed");

    assert_eq!(lowered.prelude.len(), 1);
    assert!(matches!(
        lowered.prelude[0].kind,
        HirStatementKind::Call {
            result: Some(_),
            ..
        }
    ));

    let temp_reference = reference_expr(
        temp_name,
        builtin_type_ids::INT,
        location.clone(),
        ValueMode::ImmutableReference,
    );

    let error = builder
        .lower_expression(&temp_reference)
        .expect_err("compiler temp local should not resolve through locals_by_name");

    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unresolved local"));
}

#[test]
fn field_access_uses_base_struct_identity_not_global_leaf_lookup() {
    let mut string_table = StringTable::new();
    let location = location(13);
    let struct_a = super::symbol("StructA", &mut string_table);
    let struct_b = super::symbol("StructB", &mut string_table);
    let field_leaf = string_table.intern("value");
    let field_a = struct_a.append(field_leaf);
    let field_b = struct_b.append(field_leaf);
    let local_name = super::symbol("my_struct", &mut string_table);
    let mut builder = setup_builder(&mut string_table);
    let int_type = builtin_type_ids::INT;

    builder.test_register_struct_with_fields(
        StructId(10),
        struct_a.clone(),
        int_type,
        vec![(FieldId(100), field_a.clone(), int_type)],
    );
    builder.test_register_struct_with_fields(
        StructId(11),
        struct_b.clone(),
        int_type,
        vec![(FieldId(101), field_b.clone(), int_type)],
    );

    let local_struct_type_id = builder.test_register_nominal_struct_type(
        struct_a.clone(),
        vec![(field_a.clone(), int_type, location.clone())],
        false,
    );
    register_local(
        &mut builder,
        local_name.clone(),
        LocalId(30),
        local_struct_type_id,
        location.clone(),
    );

    let base_node = AstNode {
        kind: NodeKind::Rvalue(reference_expr(
            local_name,
            local_struct_type_id,
            location.clone(),
            ValueMode::ImmutableReference,
        )),
        location: location.clone(),
        scope: InternedPath::new(),
    };

    let field_access = field_access_node(
        base_node,
        field_leaf,
        builtin_type_ids::INT,
        ConstRecordState::RuntimeValue,
        ValueMode::ImmutableReference,
        location.clone(),
    );

    let (_prelude, place) = builder
        .lower_ast_node_to_place(&field_access)
        .expect("field access should lower via base struct identity");

    match place {
        HirPlace::Field { field, .. } => assert_eq!(field, FieldId(100)),
        other => panic!("expected field place, got {other:?}"),
    }
}

#[test]
fn field_access_from_module_constant_base_materializes_temp_place() {
    let mut string_table = StringTable::new();
    let location = location(14);
    let format_name = super::symbol("format", &mut string_table);
    let format_struct = super::symbol("Format", &mut string_table);
    let center_leaf = string_table.intern("center");
    let center_field = format_struct.append(center_leaf);
    let center_value = string_table.intern("<div></div>");
    let mut builder = setup_builder(&mut string_table);

    let template_type = builtin_type_ids::STRING;

    builder.test_register_struct_with_fields(
        StructId(20),
        format_struct.clone(),
        template_type,
        vec![(FieldId(200), center_field.clone(), template_type)],
    );

    let format_type_id = builder.test_register_nominal_struct_type(
        format_struct.clone(),
        vec![(
            center_field.clone(),
            builtin_type_ids::STRING,
            location.clone(),
        )],
        false,
    );

    let format_constant = Expression::struct_instance(
        format_struct.clone(),
        vec![Declaration {
            id: center_field.clone(),
            value: Expression::string_slice(
                center_value,
                location.clone(),
                ValueMode::ImmutableOwned,
            ),
        }],
        location.clone(),
        ValueMode::ImmutableOwned,
        false,
        None,
        format_type_id,
    );

    builder.test_register_module_constant(format_name.clone(), format_constant);

    let format_reference = reference_expr(
        format_name,
        format_type_id,
        location.clone(),
        ValueMode::ImmutableReference,
    );
    let base_node = AstNode {
        kind: NodeKind::Rvalue(format_reference),
        location: location.clone(),
        scope: InternedPath::new(),
    };

    let field_access = field_access_node(
        base_node,
        center_leaf,
        builtin_type_ids::STRING,
        ConstRecordState::RuntimeValue,
        ValueMode::ImmutableReference,
        location.clone(),
    );

    let lowered = builder
        .lower_ast_node_as_expression(&field_access)
        .expect("module constant field access should lower");

    assert!(
        lowered
            .prelude
            .iter()
            .any(|statement| matches!(statement.kind, HirStatementKind::Assign { .. })),
        "expected module constant base to be materialized into a temporary local"
    );

    match lowered.value.kind {
        HirExpressionKind::Load(HirPlace::Field { field, base }) => {
            assert_eq!(field, FieldId(200));
            assert!(matches!(*base, HirPlace::Local(_)));
        }
        other => panic!("expected field load expression, got {other:?}"),
    }
}

#[test]
fn const_record_module_constant_field_access_lowers_field_value_without_struct_construct() {
    let mut string_table = StringTable::new();
    let location = location(15);
    let palette_name = super::symbol("palette", &mut string_table);
    let palette_struct = super::symbol("Palette", &mut string_table);
    let red_leaf = string_table.intern("red");
    let red_field = palette_struct.append(red_leaf);
    let red_value = string_table.intern("red");
    let mut builder = setup_builder(&mut string_table);

    let palette_type_id = builder.test_register_nominal_struct_type(
        palette_struct.clone(),
        vec![(
            red_field.clone(),
            builtin_type_ids::STRING,
            location.clone(),
        )],
        true,
    );

    let palette_constant = Expression::struct_instance(
        palette_struct.clone(),
        vec![Declaration {
            id: red_field,
            value: Expression::string_slice(red_value, location.clone(), ValueMode::ImmutableOwned),
        }],
        location.clone(),
        ValueMode::ImmutableOwned,
        true,
        None,
        palette_type_id,
    );

    builder.test_register_module_constant(palette_name.clone(), palette_constant);

    let palette_reference = const_record_reference_expr(
        palette_name,
        palette_type_id,
        location.clone(),
        ValueMode::ImmutableReference,
    );

    let field_access = field_access_node(
        AstNode {
            kind: NodeKind::Rvalue(palette_reference),
            location: location.clone(),
            scope: InternedPath::new(),
        },
        red_leaf,
        builtin_type_ids::STRING,
        ConstRecordState::RuntimeValue,
        ValueMode::ImmutableOwned,
        location.clone(),
    );

    let lowered = builder
        .lower_ast_node_as_expression(&field_access)
        .expect("const-record field access should lower the selected field value");

    assert!(
        lowered.prelude.is_empty(),
        "const-record field access should not materialize the whole record"
    );

    match lowered.value.kind {
        HirExpressionKind::StringLiteral(ref value) if value == "red" => {}
        other => panic!("expected direct string field value, got {other:?}"),
    }
}

#[test]
fn lowers_collection_builtin_host_calls_from_explicit_ast_nodes() {
    let mut string_table = StringTable::new();
    let location = location(15);
    let receiver_name = super::symbol("values", &mut string_table);
    let get_id = crate::compiler_frontend::external_packages::ExternalFunctionId::CollectionGet;
    let set_id = crate::compiler_frontend::external_packages::ExternalFunctionId::CollectionSet;
    let push_id = crate::compiler_frontend::external_packages::ExternalFunctionId::CollectionPush;
    let remove_id =
        crate::compiler_frontend::external_packages::ExternalFunctionId::CollectionRemove;
    let length_id =
        crate::compiler_frontend::external_packages::ExternalFunctionId::CollectionLength;
    let mut builder = setup_builder(&mut string_table);

    let receiver_type_id = builder
        .type_environment
        .intern_collection(builtin_type_ids::INT);
    register_local(
        &mut builder,
        receiver_name.clone(),
        LocalId(70),
        receiver_type_id,
        location.clone(),
    );

    let receiver = AstNode {
        kind: NodeKind::Rvalue(reference_expr(
            receiver_name,
            receiver_type_id,
            location.clone(),
            ValueMode::MutableReference,
        )),
        location: location.clone(),
        scope: InternedPath::new(),
    };

    let fallible_int_result = result_carrier_type_id(
        &mut builder.type_environment,
        builtin_type_ids::INT,
        builtin_type_ids::INT,
    );
    let fallible_none_result = result_carrier_type_id(
        &mut builder.type_environment,
        builtin_type_ids::NONE,
        builtin_type_ids::INT,
    );

    let cases = vec![
        (
            CollectionBuiltinOp::Get,
            vec![CallArgument::positional(
                Expression::int(1, location.clone(), ValueMode::ImmutableOwned),
                CallAccessMode::Shared,
                location.clone(),
            )],
            vec![fallible_int_result],
            get_id,
        ),
        (
            CollectionBuiltinOp::Set,
            vec![
                CallArgument::positional(
                    Expression::int(0, location.clone(), ValueMode::ImmutableOwned),
                    CallAccessMode::Shared,
                    location.clone(),
                ),
                CallArgument::positional(
                    Expression::int(99, location.clone(), ValueMode::ImmutableOwned),
                    CallAccessMode::Shared,
                    location.clone(),
                ),
            ],
            vec![fallible_none_result],
            set_id,
        ),
        (
            CollectionBuiltinOp::Push,
            vec![CallArgument::positional(
                Expression::int(4, location.clone(), ValueMode::ImmutableOwned),
                CallAccessMode::Shared,
                location.clone(),
            )],
            vec![],
            push_id,
        ),
        (
            CollectionBuiltinOp::Remove,
            vec![CallArgument::positional(
                Expression::int(0, location.clone(), ValueMode::ImmutableOwned),
                CallAccessMode::Shared,
                location.clone(),
            )],
            vec![fallible_int_result],
            remove_id,
        ),
        (
            CollectionBuiltinOp::Length,
            vec![],
            vec![builtin_type_ids::INT],
            length_id,
        ),
    ];

    for (op, args, result_type_ids, expected_id) in cases {
        let lowered = builder
            .lower_ast_node_as_expression(&AstNode {
                kind: NodeKind::CollectionBuiltinCall {
                    receiver: Box::new(receiver.clone()),
                    op,
                    args,
                    result_type_ids,
                    location: location.clone(),
                },
                location: location.clone(),
                scope: InternedPath::new(),
            })
            .expect("collection builtin call lowering should succeed");

        assert_eq!(lowered.prelude.len(), 1);
        match &lowered.prelude[0].kind {
            HirStatementKind::Call { target, args, .. } => {
                assert_eq!(target, &CallTarget::ExternalFunction(expected_id));
                assert!(
                    !args.is_empty(),
                    "collection host calls should include receiver as first argument"
                );
            }
            other => panic!("expected host call statement for collection builtin, got {other:?}"),
        }
    }
}

/// Verifies that `ExpressionKind::ChoiceConstruct` lowers to `HirExpressionKind::VariantConstruct`
/// with the correct tag index, and that the result type is registered as a choice in
/// `TypeEnvironment`.
///
/// WHY: this is the core contract of the Choice Hardening refactor — choice values must not
/// masquerade as `HirExpressionKind::Int` in HIR.
#[test]
fn lowers_choice_variant_expression_to_hir_variant_construct() {
    let mut string_table = StringTable::new();
    let location = location(1);

    let status_path = InternedPath::from_single_str("Status", &mut string_table);
    let ready_name = string_table.intern("Ready");
    let busy_name = string_table.intern("Busy");

    let mut builder = setup_builder(&mut string_table);

    let choice_variants = vec![
        ChoiceVariant {
            id: ready_name,
            payload: ChoiceVariantPayload::Unit,
            location: location.clone(),
        },
        ChoiceVariant {
            id: busy_name,
            payload: ChoiceVariantPayload::Unit,
            location: location.clone(),
        },
    ];
    let choice_type_id =
        builder.test_register_nominal_choice_type(status_path.clone(), &choice_variants);
    builder.register_choice_id(&status_path, &location).unwrap();

    let choice_expr = choice_construct_expr(
        status_path.clone(),
        ready_name,
        0,
        vec![],
        choice_type_id,
        location.clone(),
        ValueMode::ImmutableOwned,
    );

    let lowered = builder
        .lower_expression(&choice_expr)
        .expect("choice variant lowering should succeed");

    assert!(lowered.prelude.is_empty());
    assert_eq!(lowered.value.value_kind, ValueKind::Const);

    let (choice_id, variant_index) = match &lowered.value.kind {
        HirExpressionKind::VariantConstruct {
            carrier: HirVariantCarrier::Choice { choice_id },
            variant_index,
            fields,
        } => {
            assert!(
                fields.is_empty(),
                "unit variant should have no payload fields"
            );
            (*choice_id, *variant_index)
        }
        other => panic!("expected VariantConstruct, got {other:?}"),
    };

    assert_eq!(variant_index, 0, "expected tag 0 for Ready variant");
    assert_eq!(
        choice_id,
        ChoiceId(0),
        "first choice should receive ChoiceId(0)"
    );

    let hir_type = builder.type_environment.get(lowered.value.ty);
    assert!(
        matches!(
            hir_type,
            Some(TypeDefinition::Choice(ChoiceTypeDefinition {
                id: NominalTypeId(0),
                ..
            }))
        ),
        "expected Choice type with NominalTypeId(0), got {hir_type:?}",
    );
}

#[test]
fn collection_lowering_uses_pure_type_identity() {
    let mut string_table = StringTable::new();
    let mut builder = setup_builder(&mut string_table);
    let _location = SourceLocation::default();

    let type_id = builder
        .type_environment
        .intern_collection(builtin_type_ids::INT);
    let hir_type = builder.type_environment.get(type_id);

    assert!(
        matches!(
            hir_type,
            Some(TypeDefinition::Constructed(ConstructedTypeDefinition {
                constructor: TypeConstructor::Builtin(BuiltinTypeConstructor::Collection),
                ..
            }))
        ),
        "expected Collection type, got {hir_type:?}"
    );
}

#[test]
fn returns_lowering_interns_multi_return_tuple_type_id() {
    let mut string_table = StringTable::new();
    let mut builder = setup_builder(&mut string_table);
    let _location = SourceLocation::default();

    let type_id = builder
        .type_environment
        .intern_tuple(vec![builtin_type_ids::INT, builtin_type_ids::BOOL]);

    assert_eq!(
        builder.type_environment.tuple_field_ids(type_id),
        Some(
            [
                builder.type_environment.builtins().int,
                builder.type_environment.builtins().bool
            ]
            .as_slice()
        )
    );
}

#[test]
fn struct_lowering_uses_nominal_identity_only() {
    let mut string_table = StringTable::new();
    let path = InternedPath::from_single_str("User", &mut string_table);
    let mut builder = setup_builder(&mut string_table);
    let _location = SourceLocation::default();

    let struct_type_id = builder.test_register_nominal_struct_type(path.clone(), vec![], false);
    builder.test_register_struct_with_fields(StructId(0), path.clone(), struct_type_id, vec![]);

    let type_id = struct_type_id;
    let hir_type = builder.type_environment.get(type_id);

    assert!(
        matches!(
            hir_type,
            Some(TypeDefinition::Struct(
                crate::compiler_frontend::datatypes::definitions::StructTypeDefinition {
                    id: NominalTypeId(0),
                    ..
                }
            ))
        ),
        "expected Struct type with NominalTypeId(0), got {hir_type:?}"
    );
}

/// Verifies that `ExpressionKind::OptionNone` lowers to `HirExpressionKind::VariantConstruct`
/// with `HirVariantCarrier::Option` and zero fields.
#[test]
fn lowers_option_none_to_hir_variant_construct() {
    let mut string_table = StringTable::new();
    let location = location(1);
    let mut builder = setup_builder(&mut string_table);

    let option_expr = option_none_expr(
        builtin_type_ids::STRING,
        &mut builder.type_environment,
        location.clone(),
    );

    let lowered = builder
        .lower_expression(&option_expr)
        .expect("option none lowering should succeed");

    assert!(lowered.prelude.is_empty());

    match &lowered.value.kind {
        HirExpressionKind::VariantConstruct {
            carrier: HirVariantCarrier::Option,
            variant_index: 0,
            fields,
        } => {
            assert!(fields.is_empty(), "Option none should have no fields");
        }
        other => panic!("expected VariantConstruct(Option, 0, []), got {other:?}"),
    }
}

/// Verifies that `ExpressionKind::FallibleCarrierConstruct` lowers to `HirExpressionKind::VariantConstruct`
/// with `HirVariantCarrier::Fallible` and a single value field.
#[test]
fn lowers_fallible_success_to_hir_variant_construct() {
    let mut string_table = StringTable::new();
    let location = location(1);
    let mut builder = setup_builder(&mut string_table);

    let ok_type_id = builtin_type_ids::INT;
    let err_type_id = builtin_type_ids::STRING;
    let result_type_id =
        result_carrier_type_id(&mut builder.type_environment, ok_type_id, err_type_id);

    let value_expr = Expression::int(42, location.clone(), ValueMode::ImmutableOwned);

    let result_expr = Expression::result_construct(
        AstFallibleCarrierVariant::Success,
        value_expr,
        result_type_id,
        location.clone(),
        ValueMode::ImmutableOwned,
    );

    let lowered = builder
        .lower_expression(&result_expr)
        .expect("result ok lowering should succeed");

    assert!(lowered.prelude.is_empty());

    match &lowered.value.kind {
        HirExpressionKind::VariantConstruct {
            carrier: HirVariantCarrier::Fallible,
            variant_index: 0,
            fields,
        } => {
            assert_eq!(fields.len(), 1, "Result Ok should have one field");
            assert!(
                fields[0].name.is_some(),
                "Result Ok field should have a name"
            );
            assert!(
                matches!(fields[0].value.kind, HirExpressionKind::Int(42)),
                "Result Ok field value should be Int(42)"
            );
        }
        other => panic!("expected VariantConstruct(Result, 0, [_]), got {other:?}"),
    }
}
