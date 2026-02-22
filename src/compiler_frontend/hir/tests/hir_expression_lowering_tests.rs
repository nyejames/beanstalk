#![cfg(test)]

use crate::backends::function_registry::CallTarget;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, Var};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator,
};
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirBinOp, HirBlock, HirExpressionKind, HirLocal, HirPlace,
    HirStatementKind, HirTerminator, HirUnaryOp, LocalId, RegionId, StructId, ValueKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;

fn setup_builder<'a>(string_table: &'a mut StringTable) -> HirBuilder<'a> {
    let mut builder = HirBuilder::new(string_table);

    let region = RegionId(0);
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

    builder
}

fn register_local(
    builder: &mut HirBuilder<'_>,
    name: InternedPath,
    local_id: LocalId,
    data_type: DataType,
    location: TextLocation,
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

fn symbol(name: &str, string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_single_str(name, string_table)
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

fn operator_node(op: Operator, location: TextLocation) -> AstNode {
    AstNode {
        kind: NodeKind::Operator(op),
        location,
        scope: InternedPath::new(),
    }
}

#[test]
fn lowers_primitive_literals() {
    let mut string_table = StringTable::new();
    let text = string_table.intern("hello");
    let location = TextLocation::new_just_line(1);
    let mut builder = setup_builder(&mut string_table);

    let int_lowered = builder
        .lower_expression(&Expression::int(
            42,
            location.clone(),
            Ownership::ImmutableOwned,
        ))
        .expect("int lowering should succeed");
    assert!(int_lowered.prelude.is_empty());
    assert_eq!(int_lowered.value.value_kind, ValueKind::Const);
    assert!(matches!(int_lowered.value.kind, HirExpressionKind::Int(42)));

    let float_lowered = builder
        .lower_expression(&Expression::float(
            3.25,
            location.clone(),
            Ownership::ImmutableOwned,
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
            Ownership::ImmutableOwned,
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
            Ownership::ImmutableOwned,
        ))
        .expect("char lowering should succeed");
    assert!(char_lowered.prelude.is_empty());
    assert_eq!(char_lowered.value.value_kind, ValueKind::Const);
    assert!(matches!(
        char_lowered.value.kind,
        HirExpressionKind::Char('x')
    ));

    let string_expr = Expression::string_slice(text, location.clone(), Ownership::ImmutableOwned);
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
    let x = symbol("x", &mut string_table);
    let location = TextLocation::new_just_line(2);
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
        Ownership::ImmutableReference,
    );
    let lowered = builder
        .lower_expression(&expr)
        .expect("reference lowering should succeed");

    assert!(lowered.prelude.is_empty());
    assert_eq!(lowered.value.value_kind, ValueKind::Place);
    assert!(matches!(
        lowered.value.kind,
        HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
            LocalId(10)
        ))
    ));
}

#[test]
fn lowers_runtime_rpn_arithmetic_stack_correctly() {
    let mut string_table = StringTable::new();
    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);
    let location = TextLocation::new_just_line(3);
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
            Ownership::ImmutableReference,
        )),
        rvalue_node(Expression::int(
            2,
            location.clone(),
            Ownership::ImmutableOwned,
        )),
        rvalue_node(Expression::reference(
            y,
            DataType::Int,
            location.clone(),
            Ownership::ImmutableReference,
        )),
        operator_node(Operator::Multiply, location.clone()),
        operator_node(Operator::Add, location.clone()),
    ];

    let expr = Expression::runtime(
        nodes,
        DataType::Int,
        location.clone(),
        Ownership::MutableOwned,
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
fn lowers_unary_not_in_runtime_rpn() {
    let mut string_table = StringTable::new();
    let location = TextLocation::new_just_line(4);
    let mut builder = setup_builder(&mut string_table);

    let nodes = vec![
        rvalue_node(Expression::bool(
            true,
            location.clone(),
            Ownership::ImmutableOwned,
        )),
        operator_node(Operator::Not, location.clone()),
    ];

    let expr = Expression::runtime(
        nodes,
        DataType::Bool,
        location.clone(),
        Ownership::MutableOwned,
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
    let location = TextLocation::new_just_line(5);
    let mut builder = setup_builder(&mut string_table);

    let nodes = vec![
        rvalue_node(Expression::int(
            1,
            location.clone(),
            Ownership::ImmutableOwned,
        )),
        rvalue_node(Expression::int(
            9,
            location.clone(),
            Ownership::ImmutableOwned,
        )),
        operator_node(Operator::Range, location.clone()),
    ];

    let expr = Expression::runtime(
        nodes,
        DataType::Range,
        location.clone(),
        Ownership::MutableOwned,
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
    let function_name = symbol("sum", &mut string_table);
    let location = TextLocation::new_just_line(6);
    let mut builder = setup_builder(&mut string_table);
    builder.test_register_function_name(function_name.clone(), FunctionId(2));

    let call_expr = Expression::function_call(
        function_name.clone(),
        vec![Expression::int(
            7,
            location.clone(),
            Ownership::ImmutableOwned,
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
            assert_eq!(target, &CallTarget::UserFunction(function_name.clone()));
            assert_eq!(args.len(), 1);
            result.expect("call with return should bind a temp local")
        }
        _ => panic!("expected lowered call statement"),
    };

    assert!(matches!(
        lowered.value.kind,
        HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(local))
        if local == result_local
    ));
    assert_eq!(lowered.value.value_kind, ValueKind::Place);
}

#[test]
fn lowers_host_call_expression_with_host_target() {
    let mut string_table = StringTable::new();
    let literal_x = string_table.intern("x");
    let io = symbol("io", &mut string_table);
    let location = TextLocation::new_just_line(7);
    let mut builder = setup_builder(&mut string_table);

    let host_call = Expression::host_function_call(
        io.clone(),
        vec![Expression::string_slice(
            literal_x,
            location.clone(),
            Ownership::ImmutableOwned,
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
    assert_eq!(target, &CallTarget::HostFunction(io));
}

#[test]
fn preserves_left_to_right_call_prelude_order_in_nested_call_args() {
    let mut string_table = StringTable::new();
    let first = symbol("first", &mut string_table);
    let second = symbol("second", &mut string_table);
    let outer = symbol("outer", &mut string_table);
    let location = TextLocation::new_just_line(8);
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
            CallTarget::UserFunction(first),
            CallTarget::UserFunction(second),
            CallTarget::UserFunction(outer),
        ]
    );
}

#[test]
fn malformed_runtime_rpn_reports_hir_transformation_error() {
    let mut string_table = StringTable::new();
    let location = TextLocation::new_just_line(9);
    let mut builder = setup_builder(&mut string_table);

    let expr = Expression::runtime(
        vec![operator_node(Operator::Add, location.clone())],
        DataType::Int,
        location,
        Ownership::MutableOwned,
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
fn runtime_template_expression_reports_explicit_hir_transformation_error() {
    let mut string_table = StringTable::new();
    let mut builder = setup_builder(&mut string_table);

    let expr = Expression::template(Template::create_default(None), Ownership::ImmutableOwned);
    let err = builder
        .lower_expression(&expr)
        .expect_err("template lowering should fail in this phase");
    assert_eq!(err.error_type, ErrorType::HirTransformation);
    assert!(
        err.msg
            .contains("Runtime template expressions are not lowered in this phase"),
        "unexpected error message: {}",
        err.msg
    );
}

#[test]
fn local_resolution_uses_full_path_identity_not_leaf_name() {
    let mut string_table = StringTable::new();
    let x_leaf = string_table.intern("x");
    let scope_a = InternedPath::from_single_str("scope_a", &mut string_table);
    let scope_b = InternedPath::from_single_str("scope_b", &mut string_table);
    let local_a = scope_a.append(x_leaf);
    let local_b = scope_b.append(x_leaf);
    let location = TextLocation::new_just_line(10);
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
        Ownership::ImmutableReference,
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
    let location = TextLocation::new_just_line(11);
    let struct_path = symbol("MyStruct", &mut string_table);
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

    let expr_fields = vec![Var {
        id: field_path.clone(),
        value: Expression::int(42, location.clone(), Ownership::ImmutableOwned),
    }];

    let expression = Expression::new(
        ExpressionKind::StructInstance(expr_fields.clone()),
        location.clone(),
        DataType::Struct(expr_fields, Ownership::MutableOwned),
        Ownership::MutableOwned,
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
    let callee = symbol("callee", &mut string_table);
    let temp_name = symbol("__hir_tmp_0", &mut string_table);
    let location = TextLocation::new_just_line(12);
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
        Ownership::ImmutableReference,
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
    let location = TextLocation::new_just_line(13);
    let struct_a = symbol("StructA", &mut string_table);
    let struct_b = symbol("StructB", &mut string_table);
    let field_leaf = string_table.intern("value");
    let field_a = struct_a.append(field_leaf);
    let field_b = struct_b.append(field_leaf);
    let local_name = symbol("my_struct", &mut string_table);
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

    let local_struct_type = DataType::Struct(
        vec![Var {
            id: field_a.clone(),
            value: Expression::new(
                ExpressionKind::None,
                location.clone(),
                DataType::Int,
                Ownership::ImmutableOwned,
            ),
        }],
        Ownership::MutableOwned,
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
            Ownership::ImmutableReference,
        )),
        location: location.clone(),
        scope: InternedPath::new(),
    };

    let field_access = AstNode {
        kind: NodeKind::FieldAccess {
            base: Box::new(base_node),
            field: field_leaf,
            data_type: DataType::Int,
            ownership: Ownership::ImmutableReference,
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
