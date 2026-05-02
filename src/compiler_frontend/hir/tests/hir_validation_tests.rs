//! HIR validation regression tests.
//!
//! WHAT: exercises the post-lowering HIR validator against valid and intentionally broken modules.
//! WHY: validator coverage needs focused tests that isolate invariants from the rest of lowering.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind, SourceLocation};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::{AstDocFragment, AstDocFragmentKind};
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::declaration_syntax::choice::{ChoiceVariant, ChoiceVariantPayload};
use crate::compiler_frontend::hir::expressions::{
    HirExpression, HirExpressionKind, HirVariantCarrier, HirVariantField, ValueKind,
};
use crate::compiler_frontend::hir::hir_builder::validate_module_for_tests;
use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind};
use crate::compiler_frontend::hir::ids::{ChoiceId, HirNodeId, HirValueId, RegionId};
use crate::compiler_frontend::hir::patterns::{HirMatchArm, HirPattern};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::regions::HirRegion;
use crate::compiler_frontend::hir::statements::{HirStatement, HirStatementKind};
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::hir::tests::hir_expression_lowering_tests::location;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::value_mode::ValueMode;

fn test_location(line: i32) -> SourceLocation {
    location(line)
}

fn node(kind: NodeKind, location: SourceLocation) -> AstNode {
    AstNode {
        kind,
        location,
        scope: InternedPath::new(),
    }
}

fn make_test_variable(name: InternedPath, value: Expression) -> Declaration {
    Declaration { id: name, value }
}

fn param(
    name: InternedPath,
    data_type: DataType,
    mutable: bool,
    location: SourceLocation,
) -> Declaration {
    let value_mode = if mutable {
        ValueMode::MutableOwned
    } else {
        ValueMode::ImmutableOwned
    };

    Declaration {
        id: name,
        value: Expression::new(ExpressionKind::NoValue, location, data_type, value_mode),
    }
}

fn function_node(
    name: InternedPath,
    signature: FunctionSignature,
    body: Vec<AstNode>,
    location: SourceLocation,
) -> AstNode {
    node(NodeKind::Function(name, signature, body), location)
}

use crate::compiler_frontend::hir::hir_builder::{build_ast, lower_ast};

#[test]
fn valid_module_passes_explicit_validation() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    validate_module_for_tests(&module, &string_table)
        .expect("validator should accept a valid lowered module");
}

#[test]
fn validator_rejects_invalid_jump_target() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let entry_block = module.functions[module.start_function.0 as usize].entry;
    module.blocks[entry_block.0 as usize].terminator = HirTerminator::Jump {
        target: crate::compiler_frontend::hir::ids::BlockId(999),
        args: vec![],
    };

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject invalid jump target");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unknown HIR block id"));
}

#[test]
fn validator_rejects_non_literal_match_pattern() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let x = super::symbol("x", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(x.clone(), DataType::Int, false, test_location(2))],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(3))],
        test_location(2),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &mut module.blocks[start.entry.0 as usize];
    let local_id = start.params[0];
    let local_ty = entry_block.locals[0].ty;
    let region = entry_block.region;
    let scrutinee_id = HirValueId(9000);
    let pattern_id = HirValueId(9001);

    let value_location = test_location(20);
    module
        .side_table
        .map_value(&value_location, scrutinee_id, &value_location);
    module
        .side_table
        .map_value(&value_location, pattern_id, &value_location);

    entry_block.terminator = HirTerminator::Match {
        scrutinee: HirExpression {
            id: scrutinee_id,
            kind: HirExpressionKind::Int(1),
            ty: local_ty,
            value_kind: ValueKind::Const,
            region,
        },
        arms: vec![HirMatchArm {
            pattern: HirPattern::Literal(HirExpression {
                id: pattern_id,
                kind: HirExpressionKind::Load(HirPlace::Local(local_id)),
                ty: local_ty,
                value_kind: ValueKind::Place,
                region,
            }),
            guard: None,
            body: start.entry,
        }],
    };

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject non-literal match pattern");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Match literal pattern"));
}

#[test]
fn validator_rejects_missing_side_table_mappings() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let x = super::symbol("x", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x,
                    Expression::int(1, test_location(4), ValueMode::ImmutableOwned),
                )),
                test_location(4),
            ),
            node(NodeKind::Return(vec![]), test_location(5)),
        ],
        test_location(3),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    module.side_table.clear();

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject missing side-table mappings");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("side-table mapping"));
}

#[test]
fn validator_rejects_invalid_doc_fragment_location() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let mut ast = build_ast(vec![start_fn], entry_path);
    let mut invalid_location = test_location(10);
    invalid_location.end_pos.line_number = 9;
    ast.doc_fragments.push(AstDocFragment {
        kind: AstDocFragmentKind::Doc,
        value: string_table.intern("broken"),
        location: invalid_location,
    });

    let error = lower_ast(ast, &mut string_table)
        .expect_err("validator should reject invalid doc fragment locations");
    assert!(
        error
            .errors
            .iter()
            .any(|diagnostic| diagnostic.msg.contains("Doc fragment"))
    );
}

#[test]
fn validator_rejects_placeholder_terminator() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let entry = module.functions[module.start_function.0 as usize].entry;
    module.blocks[entry.0 as usize].terminator = HirTerminator::Panic { message: None };

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject placeholder terminators");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("placeholder terminator"));
}

#[test]
fn validator_rejects_region_cycle() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let region_id = module.regions[0].id();
    module.regions[0] = HirRegion::lexical(region_id, Some(region_id));

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject cyclic region parents");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("cycle"));
}

#[test]
fn validator_rejects_missing_region_parent() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let region_id = module.regions[0].id();
    module.regions[0] = HirRegion::lexical(region_id, Some(RegionId(9999)));

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject missing region parents");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("missing parent"));
}

#[test]
fn validator_rejects_out_of_range_return_alias_metadata() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let p = super::symbol("p", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(p.clone(), DataType::Int, false, test_location(1))],
            returns: vec![ReturnSlot::success(FunctionReturn::Value(DataType::Int))],
        },
        vec![node(
            NodeKind::Return(vec![Expression::reference(
                p,
                DataType::Int,
                test_location(2),
                ValueMode::ImmutableReference,
            )]),
            test_location(2),
        )],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let start_index = module.start_function.0 as usize;
    module.functions[start_index].return_aliases = vec![Some(vec![1])];

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject out-of-range return alias indices");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("out-of-range parameter index"));
}

#[test]
fn validator_rejects_cross_function_cfg_edges() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let helper_name = super::symbol("helper", &mut string_table);

    let helper = function_node(
        helper_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(2))],
        test_location(2),
    );
    let start = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![helper, start], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let start_entry = module.functions[module.start_function.0 as usize].entry;
    let helper_entry = module
        .functions
        .iter()
        .find(|function| function.id != module.start_function)
        .map(|function| function.entry)
        .expect("helper function should exist");

    module.blocks[start_entry.0 as usize].terminator = HirTerminator::Jump {
        target: helper_entry,
        args: vec![],
    };

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject cross-function CFG edges");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error.msg.contains("multiple functions") || error.msg.contains("crosses function boundary")
    );
}

#[test]
fn lowering_errors_preserve_string_table_context() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let missing_function = super::symbol("missing_fn", &mut string_table);

    let mut call_location = test_location(2);
    call_location.scope = entry_path.clone();

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::Rvalue(Expression::function_call(
                    missing_function,
                    Vec::new(),
                    Vec::new(),
                    call_location.clone(),
                )),
                call_location.clone(),
            ),
            node(NodeKind::Return(vec![]), test_location(3)),
        ],
        test_location(1),
    );

    let messages = lower_ast(build_ast(vec![start_fn], entry_path), &mut string_table)
        .expect_err("unknown function call should fail HIR lowering");

    let resolved_scope = messages.errors[0]
        .location
        .scope
        .to_portable_string(&messages.string_table);
    assert!(
        resolved_scope.ends_with("main.bst"),
        "HIR lowering errors should preserve the source path in the returned StringTable, got '{resolved_scope}'",
    );
}

// ---------------------------------------------------------------------------
// VariantConstruct validation
// ---------------------------------------------------------------------------

#[test]
fn hir_variant_construct_option_invalid_index_rejected() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let entry_block =
        &mut module.blocks[module.functions[module.start_function.0 as usize].entry.0 as usize];
    let region = entry_block.region;

    let int_ty = module.type_context.insert(HirType {
        kind: HirTypeKind::Int,
    });
    let option_ty = module.type_context.insert(HirType {
        kind: HirTypeKind::Option { inner: int_ty },
    });

    let expr_id = HirValueId(9000);
    let stmt_id = HirNodeId(9000);
    let location = test_location(10);

    let expression = HirExpression {
        id: expr_id,
        kind: HirExpressionKind::VariantConstruct {
            carrier: HirVariantCarrier::Option,
            variant_index: 99,
            fields: vec![],
        },
        ty: option_ty,
        value_kind: ValueKind::Const,
        region,
    };

    let statement = HirStatement {
        id: stmt_id,
        kind: HirStatementKind::Expr(expression),
        location: location.clone(),
    };

    module.side_table.map_statement(&location, &statement);
    module.side_table.map_value(&location, expr_id, &location);
    entry_block.statements.push(statement);

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject out-of-range Option variant index");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error.msg.contains("out of range"),
        "expected 'out of range' in error, got: {}",
        error.msg
    );
}

#[test]
fn hir_variant_construct_result_invalid_index_rejected() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let entry_block =
        &mut module.blocks[module.functions[module.start_function.0 as usize].entry.0 as usize];
    let region = entry_block.region;

    let int_ty = module.type_context.insert(HirType {
        kind: HirTypeKind::Int,
    });
    let result_ty = module.type_context.insert(HirType {
        kind: HirTypeKind::Result {
            ok: int_ty,
            err: int_ty,
        },
    });

    let expr_id = HirValueId(9000);
    let stmt_id = HirNodeId(9000);
    let location = test_location(10);

    let expression = HirExpression {
        id: expr_id,
        kind: HirExpressionKind::VariantConstruct {
            carrier: HirVariantCarrier::Result,
            variant_index: 99,
            fields: vec![],
        },
        ty: result_ty,
        value_kind: ValueKind::Const,
        region,
    };

    let statement = HirStatement {
        id: stmt_id,
        kind: HirStatementKind::Expr(expression),
        location: location.clone(),
    };

    module.side_table.map_statement(&location, &statement);
    module.side_table.map_value(&location, expr_id, &location);
    entry_block.statements.push(statement);

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject out-of-range Result variant index");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error.msg.contains("out of range"),
        "expected 'out of range' in error, got: {}",
        error.msg
    );
}

#[test]
fn hir_variant_construct_choice_wrong_field_name_rejected() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let response_param = super::symbol("response", &mut string_table);
    let ok_name = string_table.intern("Ok");
    let err_name = string_table.intern("Err");
    let wrong_name = string_table.intern("content");

    let choice_type = DataType::Choices {
        nominal_path: InternedPath::from_single_str("Response", &mut string_table),
        variants: vec![
            ChoiceVariant {
                id: ok_name,
                payload: ChoiceVariantPayload::Record {
                    fields: vec![Declaration {
                        id: InternedPath::from_single_str("message", &mut string_table),
                        value: Expression::new(
                            ExpressionKind::NoValue,
                            test_location(2),
                            DataType::StringSlice,
                            ValueMode::ImmutableOwned,
                        ),
                    }],
                },
                location: test_location(2),
            },
            ChoiceVariant {
                id: err_name,
                payload: ChoiceVariantPayload::Unit,
                location: test_location(2),
            },
        ],
        generic_instance_key: None,
    };

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(response_param, choice_type, false, test_location(2))],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(3))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let entry_block =
        &mut module.blocks[module.functions[module.start_function.0 as usize].entry.0 as usize];
    let region = entry_block.region;

    let string_ty = module.type_context.insert(HirType {
        kind: HirTypeKind::String,
    });

    let expr_id = HirValueId(9000);
    let stmt_id = HirNodeId(9000);
    let location = test_location(10);

    let expression = HirExpression {
        id: expr_id,
        kind: HirExpressionKind::VariantConstruct {
            carrier: HirVariantCarrier::Choice {
                choice_id: ChoiceId(0),
            },
            variant_index: 0,
            fields: vec![HirVariantField {
                name: Some(wrong_name),
                value: HirExpression {
                    id: HirValueId(9001),
                    kind: HirExpressionKind::StringLiteral("hello".to_owned()),
                    ty: string_ty,
                    value_kind: ValueKind::Const,
                    region,
                },
            }],
        },
        ty: string_ty,
        value_kind: ValueKind::Const,
        region,
    };

    let statement = HirStatement {
        id: stmt_id,
        kind: HirStatementKind::Expr(expression),
        location: location.clone(),
    };

    module.side_table.map_statement(&location, &statement);
    module.side_table.map_value(&location, expr_id, &location);
    entry_block.statements.push(statement);

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject wrong field name in choice VariantConstruct");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error.msg.contains("field name"),
        "expected 'field name' in error, got: {}",
        error.msg
    );
}

#[test]
fn hir_variant_construct_choice_wrong_field_type_rejected() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let response_param = super::symbol("response", &mut string_table);
    let ok_name = string_table.intern("Ok");
    let err_name = string_table.intern("Err");
    let message_name = string_table.intern("message");

    let choice_type = DataType::Choices {
        nominal_path: InternedPath::from_single_str("Response", &mut string_table),
        variants: vec![
            ChoiceVariant {
                id: ok_name,
                payload: ChoiceVariantPayload::Record {
                    fields: vec![Declaration {
                        id: InternedPath::from_single_str("message", &mut string_table),
                        value: Expression::new(
                            ExpressionKind::NoValue,
                            test_location(2),
                            DataType::StringSlice,
                            ValueMode::ImmutableOwned,
                        ),
                    }],
                },
                location: test_location(2),
            },
            ChoiceVariant {
                id: err_name,
                payload: ChoiceVariantPayload::Unit,
                location: test_location(2),
            },
        ],
        generic_instance_key: None,
    };

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(response_param, choice_type, false, test_location(2))],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(3))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let mut module = lower_ast(ast, &mut string_table).expect("lowering should succeed");
    let entry_block =
        &mut module.blocks[module.functions[module.start_function.0 as usize].entry.0 as usize];
    let region = entry_block.region;

    let string_ty = module.type_context.insert(HirType {
        kind: HirTypeKind::String,
    });
    let bool_ty = module.type_context.insert(HirType {
        kind: HirTypeKind::Bool,
    });

    let expr_id = HirValueId(9000);
    let stmt_id = HirNodeId(9000);
    let location = test_location(10);

    let expression = HirExpression {
        id: expr_id,
        kind: HirExpressionKind::VariantConstruct {
            carrier: HirVariantCarrier::Choice {
                choice_id: ChoiceId(0),
            },
            variant_index: 0,
            fields: vec![HirVariantField {
                name: Some(message_name),
                value: HirExpression {
                    id: HirValueId(9001),
                    kind: HirExpressionKind::Bool(true),
                    ty: bool_ty,
                    value_kind: ValueKind::Const,
                    region,
                },
            }],
        },
        ty: string_ty,
        value_kind: ValueKind::Const,
        region,
    };

    let statement = HirStatement {
        id: stmt_id,
        kind: HirStatementKind::Expr(expression),
        location: location.clone(),
    };

    module.side_table.map_statement(&location, &statement);
    module.side_table.map_value(&location, expr_id, &location);
    entry_block.statements.push(statement);

    let error = validate_module_for_tests(&module, &string_table)
        .expect_err("validator should reject wrong field type in choice VariantConstruct");
    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(
        error.msg.contains("field type mismatch"),
        "expected 'field type mismatch' in error, got: {}",
        error.msg
    );
}
