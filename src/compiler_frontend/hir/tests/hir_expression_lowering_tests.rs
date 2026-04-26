//! HIR expression lowering regression tests.
//!
//! WHAT: covers how typed AST expressions become HIR values, preludes, and places.
//! WHY: expression lowering is broad and subtle enough that behavior changes need focused regression tests.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::{
    CallAccessMode, CallArgument, CallPassingMode,
};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator,
};
use crate::compiler_frontend::ast::templates::template::{SlotKey, SlotPlaceholder, TemplateAtom};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::builtins::error_type::register_builtin_error_types;
use crate::compiler_frontend::builtins::{BuiltinMethodKind, CollectionBuiltinOp};
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant;
use crate::compiler_frontend::external_packages::CallTarget;
use crate::compiler_frontend::hir::blocks::{HirBlock, HirLocal};
use crate::compiler_frontend::hir::expressions::{HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::HirTypeKind;
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
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, SourceLocation};
use crate::compiler_frontend::value_mode::ValueMode;

fn setup_builder(string_table: &'_ mut StringTable) -> HirBuilder<'_> {
    let test_function_name = InternedPath::from_single_str("__expr_test_fn", string_table);
    let mut builder = HirBuilder::new(string_table, PathStringFormatConfig::default());

    let region = RegionId(0);
    let function_id = FunctionId(0);
    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Panic { message: None },
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
    data_type: DataType,
    location: SourceLocation,
) {
    let ty = builder
        .lower_data_type(&data_type, &location)
        .expect("type lowering should succeed in tests");
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

fn register_runtime_struct_nominal(
    builder: &mut HirBuilder<'_>,
    data_type: &DataType,
    struct_id: StructId,
) {
    if let DataType::Struct { nominal_path, .. } = data_type {
        builder.test_register_struct_with_fields(struct_id, nominal_path.to_owned(), vec![]);
    }
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
    let mut template = Template::create_default(vec![]);
    template.location = location.clone();

    for expr in content {
        template.content.add(expr);
    }

    template.resync_runtime_metadata();
    template.kind =
        crate::compiler_frontend::ast::templates::template::TemplateType::StringFunction;

    Expression::template(template, ValueMode::ImmutableOwned)
}

fn builtin_error_related_types(string_table: &mut StringTable) -> (DataType, DataType, DataType) {
    let manifest = register_builtin_error_types(string_table);
    let mut error_type = None;
    let mut location_type = None;
    let mut frame_type = None;

    for declaration in manifest.declarations {
        match declaration.id.name_str(string_table) {
            Some("Error") => error_type = Some(declaration.value.data_type),
            Some("ErrorLocation") => location_type = Some(declaration.value.data_type),
            Some("StackFrame") => frame_type = Some(declaration.value.data_type),
            _ => {}
        }
    }

    (
        error_type.expect("builtin Error type should be registered"),
        location_type.expect("builtin ErrorLocation type should be registered"),
        frame_type.expect("builtin StackFrame type should be registered"),
    )
}

#[test]
fn compile_time_wrapper_templates_lower_as_runtime_templates_when_they_reach_hir() {
    let mut string_table = StringTable::new();
    let before = string_table.intern("before ");
    let after = string_table.intern("after");
    let location = location(1);
    let mut builder = setup_builder(&mut string_table);

    let mut template = Template::create_default(vec![]);
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

    let mut helper = Template::create_default(vec![]);
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
    let mut template = Template::create_default(vec![]);
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
        DataType::Int,
        location.clone(),
    );

    let expr = Expression::reference(
        x,
        DataType::Int,
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

    let expr = Expression::reference(
        third_const,
        DataType::Int,
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
        Expression::reference(
            const_b.clone(),
            DataType::Int,
            location.clone(),
            ValueMode::ImmutableReference,
        ),
    );
    builder.test_register_module_constant(
        const_b.clone(),
        Expression::reference(
            const_a.clone(),
            DataType::Int,
            location.clone(),
            ValueMode::ImmutableReference,
        ),
    );

    let err = builder
        .lower_expression(&Expression::reference(
            const_a,
            DataType::Int,
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
        DataType::Int,
        location.clone(),
    );
    register_local(
        &mut builder,
        y.clone(),
        LocalId(11),
        DataType::Int,
        location.clone(),
    );

    let nodes = vec![
        rvalue_node(Expression::reference(
            x,
            DataType::Int,
            location.clone(),
            ValueMode::ImmutableReference,
        )),
        rvalue_node(Expression::int(
            2,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        rvalue_node(Expression::reference(
            y,
            DataType::Int,
            location.clone(),
            ValueMode::ImmutableReference,
        )),
        operator_node(Operator::Multiply, location.clone()),
        operator_node(Operator::Add, location.clone()),
    ];

    let expr = Expression::runtime(
        nodes,
        DataType::Int,
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
    let expected_float = builder
        .lower_data_type(&DataType::Float, &location)
        .expect("float type should lower in test context");

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

    let expr = Expression::runtime(
        nodes,
        DataType::Float,
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
    let expected_int = builder
        .lower_data_type(&DataType::Int, &location)
        .expect("int type should lower in test context");

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

    let expr = Expression::runtime(
        nodes,
        DataType::Int,
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

    let expr = Expression::runtime(
        nodes,
        DataType::Bool,
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

    let expr = Expression::runtime(
        nodes,
        DataType::Range,
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
        vec![DataType::Int],
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

    builder.test_register_struct_with_fields(StructId(21), receiver_struct.clone(), vec![]);
    builder.test_register_function_name(method_path.clone(), FunctionId(22));

    let receiver_type = DataType::runtime_struct(receiver_struct.clone(), vec![]);
    register_local(
        &mut builder,
        receiver_name.clone(),
        LocalId(23),
        receiver_type.clone(),
        location.clone(),
    );

    let receiver = AstNode {
        kind: NodeKind::Rvalue(Expression::reference(
            receiver_name,
            receiver_type,
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
                builtin: None,
                args: vec![CallArgument::positional(
                    Expression::int(7, location.clone(), ValueMode::ImmutableOwned),
                    CallAccessMode::Shared,
                    location.clone(),
                )],
                result_types: vec![DataType::Int],
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
        DataType::Int,
        location.clone(),
    );

    let receiver = AstNode {
        kind: NodeKind::Rvalue(Expression::reference(
            receiver_name,
            DataType::Int,
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
                builtin: None,
                args: vec![],
                result_types: vec![DataType::Int],
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
fn lowers_builtin_error_with_location_and_push_trace_methods_to_host_calls() {
    let mut string_table = StringTable::new();
    let location = location(13);
    let (error_type, location_type, frame_type) = builtin_error_related_types(&mut string_table);
    let error_name = super::symbol("err_value", &mut string_table);
    let location_name = super::symbol("err_location", &mut string_table);
    let frame_name = super::symbol("err_frame", &mut string_table);
    let with_location_path = super::symbol("__bs_error_with_location", &mut string_table);
    let with_location_name = string_table.intern("with_location");
    let push_trace_path = super::symbol("__bs_error_push_trace", &mut string_table);
    let push_trace_name = string_table.intern("push_trace");

    let mut builder = setup_builder(&mut string_table);
    register_runtime_struct_nominal(&mut builder, &error_type, StructId(200));
    register_runtime_struct_nominal(&mut builder, &location_type, StructId(201));
    register_runtime_struct_nominal(&mut builder, &frame_type, StructId(202));
    register_local(
        &mut builder,
        error_name.clone(),
        LocalId(60),
        error_type.to_owned(),
        location.clone(),
    );
    register_local(
        &mut builder,
        location_name.clone(),
        LocalId(61),
        location_type.to_owned(),
        location.clone(),
    );
    register_local(
        &mut builder,
        frame_name.clone(),
        LocalId(62),
        frame_type.to_owned(),
        location.clone(),
    );

    let receiver = AstNode {
        kind: NodeKind::Rvalue(Expression::reference(
            error_name.to_owned(),
            error_type.to_owned(),
            location.to_owned(),
            ValueMode::ImmutableReference,
        )),
        location: location.to_owned(),
        scope: InternedPath::new(),
    };
    let with_location = AstNode {
        kind: NodeKind::MethodCall {
            receiver: Box::new(receiver),
            method_path: with_location_path,
            method: with_location_name,
            builtin: Some(BuiltinMethodKind::WithLocation),
            args: vec![CallArgument::positional(
                Expression::reference(
                    location_name.to_owned(),
                    location_type.to_owned(),
                    location.to_owned(),
                    ValueMode::ImmutableReference,
                ),
                CallAccessMode::Shared,
                location.clone(),
            )],
            result_types: vec![error_type.to_owned()],
            location: location.to_owned(),
        },
        location: location.to_owned(),
        scope: InternedPath::new(),
    };

    let lowered_with_location = builder
        .lower_ast_node_as_expression(&with_location)
        .expect("with_location lowering should succeed");
    assert_eq!(lowered_with_location.prelude.len(), 1);
    match &lowered_with_location.prelude[0].kind {
        HirStatementKind::Call { target, args, .. } => {
            assert_eq!(target, &CallTarget::ExternalFunction(crate::compiler_frontend::external_packages::ExternalFunctionId::ErrorWithLocation));
            assert_eq!(args.len(), 2);
        }
        other => panic!("expected host call for with_location builtin, got {other:?}"),
    }

    let receiver = AstNode {
        kind: NodeKind::Rvalue(Expression::reference(
            error_name,
            error_type.to_owned(),
            location.to_owned(),
            ValueMode::ImmutableReference,
        )),
        location: location.to_owned(),
        scope: InternedPath::new(),
    };
    let push_trace = AstNode {
        kind: NodeKind::MethodCall {
            receiver: Box::new(receiver),
            method_path: push_trace_path,
            method: push_trace_name,
            builtin: Some(BuiltinMethodKind::PushTrace),
            args: vec![CallArgument::positional(
                Expression::reference(
                    frame_name,
                    frame_type,
                    location.to_owned(),
                    ValueMode::ImmutableReference,
                ),
                CallAccessMode::Shared,
                location.clone(),
            )],
            result_types: vec![error_type],
            location: location.to_owned(),
        },
        location: location.to_owned(),
        scope: InternedPath::new(),
    };

    let lowered_push_trace = builder
        .lower_ast_node_as_expression(&push_trace)
        .expect("push_trace lowering should succeed");
    assert_eq!(lowered_push_trace.prelude.len(), 1);
    match &lowered_push_trace.prelude[0].kind {
        HirStatementKind::Call { target, args, .. } => {
            assert_eq!(
                target,
                &CallTarget::ExternalFunction(
                    crate::compiler_frontend::external_packages::ExternalFunctionId::ErrorPushTrace
                )
            );
            assert_eq!(args.len(), 2);
        }
        other => panic!("expected host call for push_trace builtin, got {other:?}"),
    }
}

#[test]
fn lowers_builtin_error_bubble_with_compiler_supplied_context_args() {
    let mut string_table = StringTable::new();
    let mut call_location = location(14);
    call_location.scope = super::symbol("tests/cases/example/main.bst", &mut string_table);
    let (error_type, _, _) = builtin_error_related_types(&mut string_table);
    let error_name = super::symbol("err_value", &mut string_table);
    let bubble_path = super::symbol("__bs_error_bubble", &mut string_table);
    let bubble_name = string_table.intern("bubble");

    let mut builder = setup_builder(&mut string_table);
    register_runtime_struct_nominal(&mut builder, &error_type, StructId(210));
    register_local(
        &mut builder,
        error_name.clone(),
        LocalId(80),
        error_type.to_owned(),
        call_location.to_owned(),
    );

    let bubble = AstNode {
        kind: NodeKind::MethodCall {
            receiver: Box::new(AstNode {
                kind: NodeKind::Rvalue(Expression::reference(
                    error_name,
                    error_type.to_owned(),
                    call_location.to_owned(),
                    ValueMode::ImmutableReference,
                )),
                location: call_location.to_owned(),
                scope: InternedPath::new(),
            }),
            method_path: bubble_path,
            method: bubble_name,
            builtin: Some(BuiltinMethodKind::Bubble),
            args: vec![],
            result_types: vec![error_type],
            location: call_location.to_owned(),
        },
        location: call_location.to_owned(),
        scope: InternedPath::new(),
    };

    let lowered = builder
        .lower_ast_node_as_expression(&bubble)
        .expect("bubble lowering should succeed");
    assert_eq!(lowered.prelude.len(), 1);

    match &lowered.prelude[0].kind {
        HirStatementKind::Call { target, args, .. } => {
            assert_eq!(
                target,
                &CallTarget::ExternalFunction(
                    crate::compiler_frontend::external_packages::ExternalFunctionId::ErrorBubble
                )
            );
            assert_eq!(
                args.len(),
                5,
                "bubble call should include receiver + context args"
            );
            assert!(matches!(
                args[1].kind,
                HirExpressionKind::StringLiteral(ref value) if value == "tests/cases/example/main.bst"
            ));
            assert!(matches!(args[2].kind, HirExpressionKind::Int(15)));
            assert!(matches!(args[3].kind, HirExpressionKind::Int(1)));
            assert!(matches!(args[4].kind, HirExpressionKind::StringLiteral(..)));
        }
        other => panic!("expected host call for bubble builtin, got {other:?}"),
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
        vec![DataType::Int],
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

    let arg_one =
        Expression::function_call(first.clone(), vec![], vec![DataType::Int], location.clone());
    let arg_two = Expression::function_call(
        second.clone(),
        vec![],
        vec![DataType::Int],
        location.clone(),
    );
    let outer_call = Expression::function_call(
        outer.clone(),
        vec![arg_one, arg_two],
        vec![DataType::Int],
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

    let expr = Expression::runtime(
        vec![operator_node(Operator::Add, location.clone())],
        DataType::Int,
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
        DataType::Int,
        location.clone(),
    );

    let expr = Expression::reference(
        local_b,
        DataType::Int,
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
    let int_type = builder
        .lower_data_type(&DataType::Int, &location)
        .expect("int type lowering should succeed");

    builder.test_register_struct_with_fields(
        StructId(1),
        struct_path.clone(),
        vec![(FieldId(3), field_path.clone(), int_type)],
    );

    let expr_fields = vec![Declaration {
        id: field_path.clone(),
        value: Expression::int(42, location.clone(), ValueMode::ImmutableOwned),
    }];

    let expression = Expression::new(
        ExpressionKind::StructInstance(expr_fields.clone()),
        location.clone(),
        DataType::runtime_struct(struct_path.clone(), expr_fields),
        ValueMode::MutableOwned,
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
        other => panic!("expected StructConstruct, got {:?}", other),
    }
}

#[test]
fn temp_locals_are_not_resolvable_as_user_symbols() {
    let mut string_table = StringTable::new();
    let callee = super::symbol("callee", &mut string_table);
    let temp_name = super::symbol("__hir_tmp_0", &mut string_table);
    let location = location(12);
    let mut builder = setup_builder(&mut string_table);

    builder.test_register_function_name(callee.clone(), FunctionId(8));

    let call_expr =
        Expression::function_call(callee, vec![], vec![DataType::Int], location.clone());
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

    let temp_reference = Expression::reference(
        temp_name,
        DataType::Int,
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
    let int_type = builder
        .lower_data_type(&DataType::Int, &location)
        .expect("int type lowering should succeed");

    builder.test_register_struct_with_fields(
        StructId(10),
        struct_a.clone(),
        vec![(FieldId(100), field_a.clone(), int_type)],
    );
    builder.test_register_struct_with_fields(
        StructId(11),
        struct_b.clone(),
        vec![(FieldId(101), field_b.clone(), int_type)],
    );

    let local_struct_type = DataType::runtime_struct(
        struct_a.clone(),
        vec![Declaration {
            id: field_a.clone(),
            value: Expression::new(
                ExpressionKind::NoValue,
                location.clone(),
                DataType::Int,
                ValueMode::ImmutableOwned,
            ),
        }],
    );

    register_local(
        &mut builder,
        local_name.clone(),
        LocalId(30),
        local_struct_type.clone(),
        location.clone(),
    );

    let base_node = AstNode {
        kind: NodeKind::Rvalue(Expression::reference(
            local_name,
            local_struct_type,
            location.clone(),
            ValueMode::ImmutableReference,
        )),
        location: location.clone(),
        scope: InternedPath::new(),
    };

    let field_access = AstNode {
        kind: NodeKind::FieldAccess {
            base: Box::new(base_node),
            field: field_leaf,
            data_type: DataType::Int,
            value_mode: ValueMode::ImmutableReference,
        },
        location: location.clone(),
        scope: InternedPath::new(),
    };

    let (_prelude, place) = builder
        .lower_ast_node_to_place(&field_access)
        .expect("field access should lower via base struct identity");

    match place {
        HirPlace::Field { field, .. } => assert_eq!(field, FieldId(100)),
        other => panic!("expected field place, got {:?}", other),
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

    let template_type = builder
        .lower_data_type(&DataType::Template, &location)
        .expect("template type lowering should succeed");

    builder.test_register_struct_with_fields(
        StructId(20),
        format_struct.clone(),
        vec![(FieldId(200), center_field.clone(), template_type)],
    );

    builder.test_register_module_constant(
        format_name.clone(),
        Expression::struct_instance(
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
        ),
    );

    let format_struct_type = DataType::runtime_struct(
        format_struct.clone(),
        vec![Declaration {
            id: center_field,
            value: Expression::new(
                ExpressionKind::NoValue,
                location.clone(),
                DataType::Template,
                ValueMode::ImmutableOwned,
            ),
        }],
    );

    let base_node = AstNode {
        kind: NodeKind::Rvalue(Expression::reference(
            format_name,
            format_struct_type,
            location.clone(),
            ValueMode::ImmutableReference,
        )),
        location: location.clone(),
        scope: InternedPath::new(),
    };

    let field_access = AstNode {
        kind: NodeKind::FieldAccess {
            base: Box::new(base_node),
            field: center_leaf,
            data_type: DataType::Template,
            value_mode: ValueMode::ImmutableReference,
        },
        location: location.clone(),
        scope: InternedPath::new(),
    };

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
        other => panic!("expected field load expression, got {:?}", other),
    }
}

#[test]
fn lowers_collection_builtin_host_calls_from_explicit_ast_nodes() {
    let mut string_table = StringTable::new();
    let location = location(15);
    let receiver_name = super::symbol("values", &mut string_table);
    let get_id = crate::compiler_frontend::external_packages::ExternalFunctionId::CollectionGet;
    let push_id = crate::compiler_frontend::external_packages::ExternalFunctionId::CollectionPush;
    let remove_id =
        crate::compiler_frontend::external_packages::ExternalFunctionId::CollectionRemove;
    let length_id =
        crate::compiler_frontend::external_packages::ExternalFunctionId::CollectionLength;
    let mut builder = setup_builder(&mut string_table);

    let receiver_type = DataType::Collection(Box::new(DataType::Int));
    register_local(
        &mut builder,
        receiver_name.clone(),
        LocalId(70),
        receiver_type.clone(),
        location.clone(),
    );

    let receiver = AstNode {
        kind: NodeKind::Rvalue(Expression::reference(
            receiver_name,
            receiver_type,
            location.clone(),
            ValueMode::MutableReference,
        )),
        location: location.clone(),
        scope: InternedPath::new(),
    };

    let cases = vec![
        (
            CollectionBuiltinOp::Get,
            vec![CallArgument::positional(
                Expression::int(1, location.clone(), ValueMode::ImmutableOwned),
                CallAccessMode::Shared,
                location.clone(),
            )],
            vec![DataType::Result {
                ok: Box::new(DataType::Int),
                err: Box::new(DataType::Int),
            }],
            get_id,
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
            vec![],
            remove_id,
        ),
        (
            CollectionBuiltinOp::Length,
            vec![],
            vec![DataType::Int],
            length_id,
        ),
    ];

    for (op, args, result_types, expected_id) in cases {
        let lowered = builder
            .lower_ast_node_as_expression(&AstNode {
                kind: NodeKind::CollectionBuiltinCall {
                    receiver: Box::new(receiver.clone()),
                    op,
                    args,
                    result_types,
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

#[test]
fn lowers_collection_set_builtin_from_explicit_ast_node_to_index_assignment() {
    let mut string_table = StringTable::new();
    let location = location(16);
    let receiver_name = super::symbol("values", &mut string_table);
    let mut builder = setup_builder(&mut string_table);

    let receiver_type = DataType::Collection(Box::new(DataType::Int));
    register_local(
        &mut builder,
        receiver_name.clone(),
        LocalId(71),
        receiver_type.clone(),
        location.clone(),
    );

    let receiver = AstNode {
        kind: NodeKind::Rvalue(Expression::reference(
            receiver_name,
            receiver_type,
            location.clone(),
            ValueMode::MutableReference,
        )),
        location: location.clone(),
        scope: InternedPath::new(),
    };

    let lowered = builder
        .lower_ast_node_as_expression(&AstNode {
            kind: NodeKind::CollectionBuiltinCall {
                receiver: Box::new(receiver),
                op: CollectionBuiltinOp::Set,
                args: vec![
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
                result_types: vec![],
                location: location.clone(),
            },
            location: location.clone(),
            scope: InternedPath::new(),
        })
        .expect("collection set builtin lowering should succeed");

    assert_eq!(lowered.prelude.len(), 1);
    match &lowered.prelude[0].kind {
        HirStatementKind::Assign { target, value } => {
            assert!(matches!(value.kind, HirExpressionKind::Int(99)));
            match target {
                HirPlace::Index { base, index } => {
                    assert!(matches!(**base, HirPlace::Local(LocalId(71))));
                    assert!(matches!(index.kind, HirExpressionKind::Int(0)));
                }
                other => panic!("expected index assignment target, got {other:?}"),
            }
        }
        other => panic!("expected index assignment statement, got {other:?}"),
    }
}

/// Verifies that `ExpressionKind::ChoiceVariant` lowers to `HirExpressionKind::ChoiceVariant`
/// with the correct tag index, and that the result type is `HirTypeKind::Choice`.
///
/// WHY: this is the core contract of the Choice Hardening refactor — choice values must not
/// masquerade as `HirExpressionKind::Int` in HIR.
#[test]
fn lowers_choice_variant_expression_to_hir_choice_variant() {
    let mut string_table = StringTable::new();
    let location = location(1);

    let status_path = InternedPath::from_single_str("Status", &mut string_table);
    let ready_name = string_table.intern("Ready");
    let busy_name = string_table.intern("Busy");

    let mut builder = setup_builder(&mut string_table);

    let choice_type = DataType::Choices {
        nominal_path: status_path.clone(),
        variants: vec![
            ChoiceVariant {
                id: ready_name,
                data_type: DataType::None,
                location: location.clone(),
            },
            ChoiceVariant {
                id: busy_name,
                data_type: DataType::None,
                location: location.clone(),
            },
        ],
    };

    let choice_expr = Expression::new(
        ExpressionKind::ChoiceVariant {
            nominal_path: status_path.clone(),
            variant: ready_name,
            tag: 0,
        },
        location.clone(),
        choice_type,
        ValueMode::ImmutableOwned,
    );

    let lowered = builder
        .lower_expression(&choice_expr)
        .expect("choice variant lowering should succeed");

    assert!(lowered.prelude.is_empty());
    assert_eq!(lowered.value.value_kind, ValueKind::Const);

    let (choice_id, variant_index) = match &lowered.value.kind {
        HirExpressionKind::ChoiceVariant {
            choice_id,
            variant_index,
        } => (*choice_id, *variant_index),
        other => panic!("expected ChoiceVariant, got {other:?}"),
    };

    assert_eq!(variant_index, 0, "expected tag 0 for Ready variant");
    assert_eq!(
        choice_id,
        ChoiceId(0),
        "first choice should receive ChoiceId(0)"
    );

    let hir_type = builder.type_context.get(lowered.value.ty);
    assert!(
        matches!(
            hir_type.kind,
            HirTypeKind::Choice {
                choice_id: ChoiceId(0)
            }
        ),
        "expected Choice type with ChoiceId(0), got {:?}",
        hir_type.kind
    );
}

#[test]
fn collection_lowering_uses_pure_type_identity() {
    let mut string_table = StringTable::new();
    let mut builder = setup_builder(&mut string_table);
    let location = SourceLocation::default();

    let collection_type = DataType::Collection(Box::new(DataType::Int));
    let type_id = builder
        .lower_data_type(&collection_type, &location)
        .unwrap();
    let hir_type = builder.type_context.get(type_id);

    assert!(
        matches!(hir_type.kind, HirTypeKind::Collection { .. }),
        "expected Collection HirTypeKind, got {:?}",
        hir_type.kind
    );
}

#[test]
fn struct_lowering_uses_nominal_identity_only() {
    let mut string_table = StringTable::new();
    let path = InternedPath::from_single_str("User", &mut string_table);
    let mut builder = setup_builder(&mut string_table);
    let location = SourceLocation::default();

    builder.test_register_struct_with_fields(StructId(0), path.clone(), vec![]);

    let struct_type = DataType::runtime_struct(path, vec![]);
    let type_id = builder.lower_data_type(&struct_type, &location).unwrap();
    let hir_type = builder.type_context.get(type_id);

    assert!(
        matches!(
            hir_type.kind,
            HirTypeKind::Struct {
                struct_id: StructId(0)
            }
        ),
        "expected Struct HirTypeKind with StructId(0), got {:?}",
        hir_type.kind
    );
}
