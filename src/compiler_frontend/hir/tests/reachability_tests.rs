//! HIR reachability regression tests.
//!
//! WHAT: exercises the backend-neutral HIR reachability helper against hand-built CFGs.
//! WHY: runtime metadata consumers need deterministic function/block/external-call facts without
//! coupling these tests to AST lowering or backend emission.

use crate::compiler_frontend::builtins::casts::targets::BuiltinCastPolicyId;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::external_packages::{CallTarget, ExternalFunctionId};
use crate::compiler_frontend::hir::blocks::HirBlock;
use crate::compiler_frontend::hir::functions::HirFunction;
use crate::compiler_frontend::hir::ids::{BlockId, FunctionId, HirNodeId, HirValueId, RegionId};
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::hir::patterns::{HirMatchArm, HirPattern};
use crate::compiler_frontend::hir::reachability::{
    HirReachability, HirReachabilityInput, ReachableMapUseKind, collect_hir_reachability,
    collect_reachability_from_start,
};
use crate::compiler_frontend::hir::statements::{HirStatement, HirStatementKind};
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::hir::{
    expressions::HirExpression, expressions::HirExpressionKind, expressions::HirMapEntry,
    expressions::HirMapOp,
};
use crate::compiler_frontend::hir::{expressions::ValueKind, ids::LocalId};
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, SourceLocation};

#[test]
fn start_reachability_ignores_unreachable_function_external_calls() {
    let reachable_external_function = ExternalFunctionId::Synthetic(99);
    let unreachable_external_function = ExternalFunctionId::Synthetic(100);
    let reachable_location = location_at(10, 4);
    let unreachable_location = location_at(20, 8);
    let module = hir_module(
        FunctionId(0),
        vec![
            function(FunctionId(0), BlockId(0)),
            function(FunctionId(1), BlockId(1)),
        ],
        vec![
            block(
                BlockId(0),
                vec![call_statement_at(
                    0,
                    CallTarget::ExternalFunction(reachable_external_function),
                    reachable_location.clone(),
                )],
                HirTerminator::Return(unit_expression(0)),
            ),
            block(
                BlockId(1),
                vec![call_statement_at(
                    0,
                    CallTarget::ExternalFunction(unreachable_external_function),
                    unreachable_location,
                )],
                HirTerminator::Return(unit_expression(1)),
            ),
        ],
    );

    let reachability =
        collect_reachability_from_start(&module).expect("reachability should collect from start");

    assert_reachability(&reachability, &[0], &[0], &[reachable_external_function]);
    assert_reachable_external_calls(
        &reachability,
        &[(
            reachable_external_function,
            HirNodeId(0),
            reachable_location,
        )],
    );
}

#[test]
fn user_function_calls_make_transitive_functions_and_external_calls_reachable() {
    let external_function = ExternalFunctionId::Synthetic(200);
    let module = hir_module(
        FunctionId(0),
        vec![
            function(FunctionId(0), BlockId(0)),
            function(FunctionId(1), BlockId(2)),
        ],
        vec![
            block(
                BlockId(0),
                vec![call_statement(0, CallTarget::UserFunction(FunctionId(1)))],
                HirTerminator::Jump {
                    target: BlockId(1),
                    args: vec![],
                },
            ),
            block(
                BlockId(1),
                vec![],
                HirTerminator::Return(unit_expression(1)),
            ),
            block(
                BlockId(2),
                vec![call_statement(
                    1,
                    CallTarget::ExternalFunction(external_function),
                )],
                HirTerminator::Return(unit_expression(2)),
            ),
        ],
    );

    let reachability =
        collect_reachability_from_start(&module).expect("reachability should follow call graph");

    assert_reachability(&reachability, &[0, 1], &[0, 1, 2], &[external_function]);
}

#[test]
fn cfg_successors_cover_branch_match_break_continue_and_terminal_edges() {
    let module = hir_module(
        FunctionId(0),
        vec![function(FunctionId(0), BlockId(0))],
        vec![
            block(
                BlockId(0),
                vec![],
                HirTerminator::If {
                    condition: bool_expression(0),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            ),
            block(
                BlockId(1),
                vec![],
                HirTerminator::Jump {
                    target: BlockId(3),
                    args: vec![],
                },
            ),
            block(
                BlockId(2),
                vec![],
                HirTerminator::FallibleBranch {
                    result: bool_expression(1),
                    success_block: BlockId(4),
                    error_block: BlockId(5),
                },
            ),
            block(
                BlockId(3),
                vec![],
                HirTerminator::Match {
                    scrutinee: int_expression(2),
                    arms: vec![match_arm(BlockId(6)), match_arm(BlockId(7))],
                },
            ),
            block(
                BlockId(4),
                vec![],
                HirTerminator::Break { target: BlockId(8) },
            ),
            block(
                BlockId(5),
                vec![],
                HirTerminator::Continue { target: BlockId(9) },
            ),
            block(
                BlockId(6),
                vec![],
                HirTerminator::ReturnSuccess(unit_expression(3)),
            ),
            block(
                BlockId(7),
                vec![],
                HirTerminator::ReturnError(unit_expression(4)),
            ),
            block(
                BlockId(8),
                vec![],
                HirTerminator::AssertFailure {
                    message: Some("stop".to_owned()),
                },
            ),
            block(
                BlockId(9),
                vec![],
                HirTerminator::RuntimeFailure {
                    message: "stop".to_owned(),
                },
            ),
        ],
    );

    let reachability =
        collect_reachability_from_start(&module).expect("reachability should follow CFG edges");

    assert_reachability(&reachability, &[0], &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9], &[]);
}

#[test]
fn custom_roots_are_supported_without_using_module_start() {
    let external_function = ExternalFunctionId::Synthetic(300);
    let module = hir_module(
        FunctionId(0),
        vec![
            function(FunctionId(0), BlockId(0)),
            function(FunctionId(1), BlockId(1)),
        ],
        vec![
            block(
                BlockId(0),
                vec![],
                HirTerminator::Return(unit_expression(0)),
            ),
            block(
                BlockId(1),
                vec![call_statement(
                    0,
                    CallTarget::ExternalFunction(external_function),
                )],
                HirTerminator::Return(unit_expression(1)),
            ),
        ],
    );

    let reachability = collect_hir_reachability(HirReachabilityInput {
        hir: &module,
        root_functions: vec![FunctionId(1)],
    })
    .expect("reachability should collect from explicit roots");

    assert_reachability(&reachability, &[1], &[1], &[external_function]);
    assert_reachable_external_calls(
        &reachability,
        &[(external_function, HirNodeId(0), SourceLocation::default())],
    );
}

#[test]
fn reachability_records_reachable_map_uses_only() {
    let literal_location = location_at(40, 2);
    let operation_location = location_at(41, 4);
    let unreachable_location = location_at(50, 6);
    let module = hir_module(
        FunctionId(0),
        vec![
            function(FunctionId(0), BlockId(0)),
            function(FunctionId(1), BlockId(1)),
        ],
        vec![
            block(
                BlockId(0),
                vec![
                    HirStatement {
                        id: HirNodeId(10),
                        kind: HirStatementKind::Expr(map_literal_expression(10)),
                        location: literal_location.clone(),
                    },
                    map_statement_at(11, HirMapOp::Contains, operation_location.clone()),
                ],
                HirTerminator::Return(unit_expression(0)),
            ),
            block(
                BlockId(1),
                vec![
                    HirStatement {
                        id: HirNodeId(12),
                        kind: HirStatementKind::Expr(map_literal_expression(12)),
                        location: unreachable_location.clone(),
                    },
                    map_statement_at(13, HirMapOp::Clear, unreachable_location),
                ],
                HirTerminator::Return(unit_expression(1)),
            ),
        ],
    );

    let reachability =
        collect_reachability_from_start(&module).expect("reachability should collect map uses");

    assert_reachability(&reachability, &[0], &[0], &[]);
    assert_eq!(
        reachable_map_use_summaries(&reachability),
        vec![
            ("literal".to_owned(), 40, 2),
            ("contains".to_owned(), 41, 4)
        ],
        "only map uses in reachable blocks should be reported"
    );
}

#[test]
fn missing_function_references_are_internal_hir_errors() {
    let module = hir_module(FunctionId(0), vec![], vec![]);

    let error = collect_hir_reachability(HirReachabilityInput {
        hir: &module,
        root_functions: vec![FunctionId(99)],
    })
    .expect_err("missing root function should fail");

    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unknown HIR function id"));
}

#[test]
fn missing_block_references_are_internal_hir_errors() {
    let module = hir_module(
        FunctionId(0),
        vec![function(FunctionId(0), BlockId(0))],
        vec![block(
            BlockId(0),
            vec![],
            HirTerminator::Jump {
                target: BlockId(99),
                args: vec![],
            },
        )],
    );

    let error =
        collect_reachability_from_start(&module).expect_err("missing target block should fail");

    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Unknown HIR block id"));
}

#[test]
fn uninitialized_terminators_are_internal_hir_errors() {
    let module = hir_module(
        FunctionId(0),
        vec![function(FunctionId(0), BlockId(0))],
        vec![block(BlockId(0), vec![], HirTerminator::Uninitialized)],
    );

    let error =
        collect_reachability_from_start(&module).expect_err("uninitialized terminator should fail");

    assert_eq!(error.error_type, ErrorType::HirTransformation);
    assert!(error.msg.contains("Uninitialized HIR terminator"));
}

fn hir_module(
    start_function: FunctionId,
    functions: Vec<HirFunction>,
    blocks: Vec<HirBlock>,
) -> HirModule {
    let mut module = HirModule::new();
    module.start_function = start_function;
    module.functions = functions;
    module.blocks = blocks;
    module
}

fn function(id: FunctionId, entry: BlockId) -> HirFunction {
    HirFunction {
        id,
        entry,
        params: vec![],
        return_type: builtin_type_ids::NONE,
        return_aliases: vec![],
    }
}

fn block(id: BlockId, statements: Vec<HirStatement>, terminator: HirTerminator) -> HirBlock {
    HirBlock {
        id,
        region: RegionId(0),
        locals: vec![],
        statements,
        terminator,
    }
}

fn call_statement(id: u32, target: CallTarget) -> HirStatement {
    call_statement_at(id, target, SourceLocation::default())
}

fn call_statement_at(id: u32, target: CallTarget, location: SourceLocation) -> HirStatement {
    HirStatement {
        id: HirNodeId(id),
        kind: HirStatementKind::Call {
            target,
            args: vec![],
            result: None::<LocalId>,
        },
        location,
    }
}

fn map_statement_at(id: u32, op: HirMapOp, location: SourceLocation) -> HirStatement {
    HirStatement {
        id: HirNodeId(id),
        kind: HirStatementKind::MapOp {
            op,
            receiver: int_expression(id + 100),
            args: vec![int_expression(id + 200)],
            result: None::<LocalId>,
        },
        location,
    }
}

fn match_arm(body: BlockId) -> HirMatchArm {
    HirMatchArm {
        pattern: HirPattern::Wildcard,
        guard: None,
        body,
    }
}

fn unit_expression(id: u32) -> HirExpression {
    HirExpression {
        id: HirValueId(id),
        kind: HirExpressionKind::TupleConstruct { elements: vec![] },
        ty: builtin_type_ids::NONE,
        value_kind: ValueKind::RValue,
        region: RegionId(0),
    }
}

fn bool_expression(id: u32) -> HirExpression {
    HirExpression {
        id: HirValueId(id),
        kind: HirExpressionKind::Bool(true),
        ty: builtin_type_ids::BOOL,
        value_kind: ValueKind::Const,
        region: RegionId(0),
    }
}

fn int_expression(id: u32) -> HirExpression {
    HirExpression {
        id: HirValueId(id),
        kind: HirExpressionKind::Int(1),
        ty: builtin_type_ids::INT,
        value_kind: ValueKind::Const,
        region: RegionId(0),
    }
}

fn cast_expression(id: u32) -> HirExpression {
    HirExpression {
        id: HirValueId(id),
        kind: HirExpressionKind::Cast {
            source: Box::new(int_expression(id + 1)),
            policy: BuiltinCastPolicyId::IntToString,
        },
        ty: builtin_type_ids::STRING,
        value_kind: ValueKind::RValue,
        region: RegionId(0),
    }
}

#[test]
fn reachability_records_reachable_runtime_casts_only() {
    let reachable_location = location_at(30, 2);
    let unreachable_location = location_at(50, 4);
    let module = hir_module(
        FunctionId(0),
        vec![
            function(FunctionId(0), BlockId(0)),
            function(FunctionId(1), BlockId(1)),
        ],
        vec![
            block(
                BlockId(0),
                vec![HirStatement {
                    id: HirNodeId(10),
                    kind: HirStatementKind::Expr(cast_expression(10)),
                    location: reachable_location.clone(),
                }],
                HirTerminator::Return(unit_expression(0)),
            ),
            block(
                BlockId(1),
                vec![HirStatement {
                    id: HirNodeId(11),
                    kind: HirStatementKind::Expr(cast_expression(11)),
                    location: unreachable_location.clone(),
                }],
                HirTerminator::Return(unit_expression(1)),
            ),
        ],
    );

    let reachability =
        collect_reachability_from_start(&module).expect("reachability should collect casts");

    assert_eq!(
        reachability.reachable_runtime_casts.len(),
        1,
        "only casts in reachable blocks should be reported"
    );
    assert_eq!(
        reachability.reachable_runtime_casts[0]
            .location
            .start_pos
            .line_number,
        30
    );
    assert_eq!(
        reachability.reachable_runtime_casts[0]
            .location
            .start_pos
            .char_column,
        2
    );
}

fn map_literal_expression(id: u32) -> HirExpression {
    HirExpression {
        id: HirValueId(id),
        kind: HirExpressionKind::MapLiteral(vec![HirMapEntry {
            key: int_expression(id + 1),
            value: int_expression(id + 2),
        }]),
        ty: builtin_type_ids::INT,
        value_kind: ValueKind::RValue,
        region: RegionId(0),
    }
}

fn assert_reachability(
    reachability: &HirReachability,
    function_ids: &[u32],
    block_ids: &[u32],
    external_function_ids: &[ExternalFunctionId],
) {
    assert_eq!(
        sorted_function_ids(reachability),
        function_ids,
        "reachable functions differ"
    );
    assert_eq!(
        sorted_block_ids(reachability),
        block_ids,
        "reachable blocks differ"
    );
    assert_eq!(
        sorted_external_function_ids(reachability),
        sorted_external_ids(external_function_ids),
        "reachable external functions differ"
    );
}

fn assert_reachable_external_calls(
    reachability: &HirReachability,
    expected_calls: &[(ExternalFunctionId, HirNodeId, SourceLocation)],
) {
    let actual_calls = reachability
        .reachable_external_calls
        .iter()
        .map(|call| {
            (
                external_id_sort_key(&call.function_id),
                call.statement_id.0,
                call.location.start_pos.line_number,
                call.location.start_pos.char_column,
            )
        })
        .collect::<Vec<_>>();

    let expected_calls = expected_calls
        .iter()
        .map(|(function_id, statement_id, location)| {
            (
                external_id_sort_key(function_id),
                statement_id.0,
                location.start_pos.line_number,
                location.start_pos.char_column,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        actual_calls, expected_calls,
        "reachable external call sites differ"
    );
}

fn reachable_map_use_summaries(reachability: &HirReachability) -> Vec<(String, i32, i32)> {
    reachability
        .reachable_map_uses
        .iter()
        .map(|map_use| {
            (
                match &map_use.kind {
                    ReachableMapUseKind::Literal => "literal".to_owned(),
                    ReachableMapUseKind::Operation(op) => op.source_name().to_owned(),
                },
                map_use.location.start_pos.line_number,
                map_use.location.start_pos.char_column,
            )
        })
        .collect()
}

fn location_at(line_number: i32, char_column: i32) -> SourceLocation {
    SourceLocation {
        start_pos: CharPosition {
            line_number,
            char_column,
        },
        end_pos: CharPosition {
            line_number,
            char_column: char_column + 1,
        },
        ..SourceLocation::default()
    }
}

fn sorted_function_ids(reachability: &HirReachability) -> Vec<u32> {
    let mut ids = reachability
        .reachable_functions
        .iter()
        .map(|function_id| function_id.0)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

fn sorted_block_ids(reachability: &HirReachability) -> Vec<u32> {
    let mut ids = reachability
        .reachable_blocks
        .iter()
        .map(|block_id| block_id.0)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

fn sorted_external_function_ids(reachability: &HirReachability) -> Vec<String> {
    let mut ids = reachability
        .reachable_external_functions
        .iter()
        .map(external_id_sort_key)
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn sorted_external_ids(ids: &[ExternalFunctionId]) -> Vec<String> {
    let mut ids = ids.iter().map(external_id_sort_key).collect::<Vec<_>>();
    ids.sort();
    ids
}

fn external_id_sort_key(id: &ExternalFunctionId) -> String {
    match id {
        ExternalFunctionId::Io => "builtin:io".to_owned(),
        ExternalFunctionId::CollectionGet => "builtin:collection_get".to_owned(),
        ExternalFunctionId::CollectionSet => "builtin:collection_set".to_owned(),
        ExternalFunctionId::CollectionPush => "builtin:collection_push".to_owned(),
        ExternalFunctionId::CollectionRemove => "builtin:collection_remove".to_owned(),
        ExternalFunctionId::CollectionLength => "builtin:collection_length".to_owned(),
        ExternalFunctionId::Synthetic(id) => format!("synthetic:{id}"),
    }
}
